// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console service: owns the screen and the keyboard state, redraws
//! on every command, answers `Dump`, and reports Enter and function
//! keys on screens whose meaning belongs to its caller. Runs on its own
//! thread. What names the product on its screens is the caller's
//! ([`Identity`]).

use std::sync::mpsc::{Receiver, Sender};

use crate::display::Display;
use crate::editor::{self, Outcome};
use crate::fb::{FbError, Framebuffer};
use crate::keyboard::KeyEvent;
use crate::keyboard::map::{Key, Modifiers, translate, update_modifiers};
use crate::render::Renderer;
use crate::scale::Scale;
use crate::screen::{Attr, COLS, ROWS, Screen};
use crate::screens;

/// Input-inhibited indicator, bottom left.
pub const INHIBITED: &str = "X II";

/// Who the console speaks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// The top line of the SRC display: the product, its release and
    /// its build, as the caller words them.
    pub banner: String,
    /// The reference code after which a sign-on display may show; until
    /// it is shown the SRC display stays up. Empty: at once.
    pub ready_code: String,
}

/// What the caller asks of the console.
#[derive(Debug)]
pub enum Command {
    /// Show the SRC display with this code current.
    ShowSrc {
        /// System name for the top line.
        system: String,
        /// The code.
        code: String,
        /// Its text.
        text: String,
    },
    /// Show a screen built elsewhere; Enter reports every field.
    ShowScreen(Screen),
    /// Show the sign-on display.
    #[cfg(feature = "session")]
    ShowSignOn {
        /// System name.
        system: String,
        /// Subsystem name.
        subsystem: String,
        /// Display name.
        display: String,
        /// Password level 2 or 3: the long password field.
        long_password: bool,
    },
    /// Replace the message line.
    Message {
        /// Text.
        text: String,
        /// Shown in red.
        error: bool,
    },
    /// A key from a keyboard reader.
    Key(KeyEvent),
    /// A key from elsewhere (a service console's `key` and `type`, a
    /// serial terminal), already a 5250 key.
    Virtual(Key),
    /// Reply with the 24 lines of the grid.
    Dump(Sender<String>),
    /// A TN5250E session to the system's telnet server is up: the
    /// console becomes its terminal; the operator's records go out
    /// on the sender.
    #[cfg(feature = "session")]
    Attach(Sender<Vec<u8>>),
    /// A record from the session.
    #[cfg(feature = "session")]
    Record(Vec<u8>),
    /// Reply with the display's mode and scale, asking the framebuffer
    /// for its mode again first.
    Describe(Sender<String>),
    /// The session ended: the console's own screens again.
    #[cfg(feature = "session")]
    Detach,
}

/// What the console reports back to its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Enter on an application screen, with every input field in order.
    Enter(Vec<String>),
    /// A function key on any screen, with an application screen's
    /// input fields in order (a prompt reads them on F10 as on Enter).
    Function(u8, Vec<String>),
    /// Enter on the sign-on display: user, password, program, menu,
    /// current library.
    #[cfg(feature = "session")]
    SignOn(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Src,
    App,
    #[cfg(feature = "session")]
    SignOn,
    /// A 5250 terminal of the system's own telnet server.
    #[cfg(feature = "session")]
    Terminal,
}

/// The console state.
pub struct Console {
    identity: Identity,
    renderer: Option<Renderer>,
    /// Displays besides the framebuffer: a serial terminal.
    displays: Vec<Box<dyn Display>>,
    screen: Screen,
    kind: Kind,
    recent: Vec<(String, String)>,
    system: String,
    mods: Modifiers,
    locked: bool,
    out: Option<Sender<Input>>,
    /// The terminal, while attached to a session.
    #[cfg(feature = "session")]
    term: Option<crate::terminal::ConsoleTerminal>,
    /// Where the terminal's records go.
    #[cfg(feature = "session")]
    link: Option<Sender<Vec<u8>>>,
    /// The ready code was shown: sign-on may show from now on.
    #[cfg(feature = "session")]
    complete: bool,
    /// A sign-on asked for before that, shown when it comes.
    #[cfg(feature = "session")]
    pending: Option<Command>,
}

impl Console {
    /// Open the framebuffer. Without one the console still keeps a screen,
    /// so `Dump` works over serial.
    pub fn open(identity: Identity) -> (Self, Result<(), FbError>) {
        Self::open_with(identity, true)
    }

    /// Open with or without the display: the serial console mode keeps
    /// the screen in memory only and never touches the framebuffer.
    pub fn open_with(identity: Identity, display: bool) -> (Self, Result<(), FbError>) {
        Self::open_scaled(identity, display, Scale::Auto)
    }

    /// Open with the scale chosen on the command line.
    pub fn open_scaled(
        identity: Identity,
        display: bool,
        scale: Scale,
    ) -> (Self, Result<(), FbError>) {
        let opened = if display {
            Framebuffer::open()
                .and_then(|fb| Renderer::new(fb, ROWS, COLS, scale).map_err(FbError::from))
        } else {
            Err(FbError::NotWanted)
        };
        let (renderer, status) = match opened {
            Ok(r) => (Some(r), Ok(())),
            Err(FbError::NotWanted) => (None, Ok(())),
            Err(e) => (None, Err(e)),
        };
        (Self::with(identity, renderer), status)
    }

    fn with(identity: Identity, renderer: Option<Renderer>) -> Self {
        Self {
            identity,
            renderer,
            displays: Vec::new(),
            screen: Screen::new(),
            kind: Kind::Src,
            recent: Vec::new(),
            system: String::new(),
            mods: Modifiers::default(),
            locked: false,
            out: None,
            #[cfg(feature = "session")]
            term: None,
            #[cfg(feature = "session")]
            link: None,
            #[cfg(feature = "session")]
            complete: false,
            #[cfg(feature = "session")]
            pending: None,
        }
    }

    /// Show the screens on another display too (a serial terminal), from
    /// the next redraw on. Its keys come in as [`Command::Virtual`].
    pub fn add_display(&mut self, display: Box<dyn Display>) {
        self.displays.push(display);
    }

    /// Whether the console is a session's terminal.
    #[cfg(feature = "session")]
    pub fn attached(&self) -> bool {
        self.kind == Kind::Terminal
    }

    /// Whether a display is attached.
    pub fn has_display(&self) -> bool {
        self.renderer.is_some()
    }

    /// Geometry and whether fbcon was detached, for the serial log.
    pub fn describe(&self) -> String {
        match &self.renderer {
            Some(r) => r.describe(),
            None => "no display".to_string(),
        }
    }

    /// Serve commands until the channel closes. `out` receives `Input`.
    pub fn run(mut self, rx: &Receiver<Command>, out: Sender<Input>) {
        self.out = Some(out);
        while let Ok(cmd) = rx.recv() {
            self.handle(cmd);
        }
    }

    fn src_screen(&self, code: &str, text: &str) -> Screen {
        screens::src_screen(
            &self.identity.banner,
            &self.system,
            code,
            text,
            &self.recent,
        )
    }

    /// Apply one command and redraw.
    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::ShowSrc { system, code, text } => {
                self.system = system;
                self.recent.push((code.clone(), text.clone()));
                self.screen = self.src_screen(&code, &text);
                self.kind = Kind::Src;
                self.locked = false;
                #[cfg(feature = "session")]
                if code == self.identity.ready_code {
                    self.complete = true;
                    if let Some(sign_on) = self.pending.take() {
                        return self.handle(sign_on);
                    }
                }
            }
            // Until the system is ready the SRC display stays up.
            #[cfg(feature = "session")]
            cmd @ Command::ShowSignOn { .. }
                if !self.complete && !self.identity.ready_code.is_empty() =>
            {
                self.pending = Some(cmd);
                return;
            }
            Command::ShowScreen(screen) => {
                self.screen = screen;
                self.kind = Kind::App;
                self.locked = false;
                if self.screen.fields.is_empty() {
                    self.screen.cursor = (0, 0);
                } else {
                    editor::home(&mut self.screen);
                }
            }
            #[cfg(feature = "session")]
            Command::ShowSignOn {
                system,
                subsystem,
                display,
                long_password,
            } => {
                self.system = system;
                self.screen = screens::sign_on(&self.system, &subsystem, &display, long_password);
                self.kind = Kind::SignOn;
                self.locked = false;
                editor::home(&mut self.screen);
            }
            #[cfg(feature = "session")]
            Command::Message { .. } if self.pending.is_some() => return,
            Command::Message { text, error } => {
                screens::set_message(&mut self.screen, &text, error);
            }
            Command::Key(ev) => {
                if !self.key(ev) {
                    return;
                }
            }
            Command::Virtual(key) => {
                if !self.apply(key) {
                    return;
                }
            }
            Command::Dump(reply) => {
                let _ = reply.send(self.dump());
                return;
            }
            #[cfg(feature = "session")]
            Command::Attach(link) => {
                self.term = Some(crate::terminal::ConsoleTerminal::new());
                self.link = Some(link);
                self.kind = Kind::Terminal;
                self.locked = false;
                self.screen = Screen::new();
            }
            #[cfg(feature = "session")]
            Command::Record(bytes) => {
                let (Some(t), Ok(rec)) =
                    (self.term.as_mut(), tn5250::record::Record::decode(&bytes))
                else {
                    return;
                };
                t.apply(&rec);
                self.screen = t.screen();
            }
            Command::Describe(reply) => {
                // A mode change shows at once.
                if self.renderer.as_mut().is_some_and(Renderer::check_mode) {
                    self.redraw();
                }
                let _ = reply.send(self.describe());
                return;
            }
            #[cfg(feature = "session")]
            Command::Detach => self.detach(),
        }
        self.redraw();
    }

    /// The grid as text: the terminal's while attached.
    fn dump(&self) -> String {
        #[cfg(feature = "session")]
        if let Some(t) = &self.term
            && self.kind == Kind::Terminal
        {
            let mut s = t.text_rows().join("\n");
            s.push('\n');
            return s;
        }
        self.screen.dump()
    }

    /// The session ended: the last SRC display again.
    #[cfg(feature = "session")]
    fn detach(&mut self) {
        // The mode may have changed while the session drew.
        if let Some(r) = self.renderer.as_mut() {
            r.check_mode();
        }
        self.term = None;
        self.link = None;
        if self.kind == Kind::Terminal {
            self.kind = Kind::Src;
            self.screen = match self.recent.last() {
                Some((code, text)) => self.src_screen(code, text),
                None => Screen::new(),
            };
        }
    }

    /// The current screen.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Keyboard locked (input inhibited)?
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Handle a key event. Returns whether the screen changed.
    fn key(&mut self, ev: KeyEvent) -> bool {
        if update_modifiers(ev, &mut self.mods) || !ev.pressed {
            return false;
        }
        let Some(key) = translate(ev.code, self.mods) else {
            return false;
        };
        self.apply(key)
    }

    /// Apply a 5250 key. Returns whether the screen changed.
    fn apply(&mut self, key: Key) -> bool {
        #[cfg(feature = "session")]
        if self.kind == Kind::Terminal {
            let Some(t) = self.term.as_mut() else {
                return false;
            };
            for bytes in t.key(key) {
                if let Some(link) = &self.link {
                    let _ = link.send(bytes);
                }
            }
            self.screen = t.screen();
            return true;
        }
        if self.locked {
            if matches!(editor::apply(&mut self.screen.clone(), key), Outcome::Reset) {
                self.unlock();
                return true;
            }
            return false;
        }
        match editor::apply(&mut self.screen, key) {
            Outcome::Edited => true,
            Outcome::Enter => self.enter(),
            Outcome::Function(n) => {
                let fields = if self.kind == Kind::App {
                    self.screen
                        .fields
                        .iter()
                        .map(|f| self.screen.field_text(f))
                        .collect()
                } else {
                    Vec::new()
                };
                self.send(Input::Function(n, fields));
                false
            }
            Outcome::Reset | Outcome::Attention | Outcome::Ignored => false,
        }
    }

    /// Enter: an application screen's fields go to the caller; sign-on
    /// locks the keyboard; a screen without fields ignores Enter.
    fn enter(&mut self) -> bool {
        match self.kind {
            Kind::App => {
                if !self.screen.fields.is_empty() {
                    let fields = self
                        .screen
                        .fields
                        .iter()
                        .map(|f| self.screen.field_text(f))
                        .collect();
                    self.send(Input::Enter(fields));
                }
                false
            }
            #[cfg(feature = "session")]
            Kind::SignOn => {
                // The caller signs on and shows the job's first screen,
                // or this one again with the reason.
                let fields = self
                    .screen
                    .fields
                    .iter()
                    .map(|f| self.screen.field_text(f))
                    .collect();
                self.send(Input::SignOn(fields));
                self.lock("");
                true
            }
            Kind::Src => false,
            #[cfg(feature = "session")]
            Kind::Terminal => false,
        }
    }

    #[cfg(feature = "session")]
    fn lock(&mut self, message: &str) {
        self.locked = true;
        screens::set_message(&mut self.screen, message, true);
        self.screen.put(24, 1, INHIBITED, Attr::WHITE);
    }

    fn unlock(&mut self) {
        self.locked = false;
        screens::set_message(&mut self.screen, "", false);
        self.screen.put(24, 1, "    ", Attr::GREEN);
    }

    fn send(&self, input: Input) {
        if let Some(out) = &self.out {
            let _ = out.send(input);
        }
    }

    fn redraw(&mut self) {
        if let Some(r) = self.renderer.as_mut() {
            let _ = r.draw(&self.screen);
        }
        for d in &mut self.displays {
            let _ = d.draw(&self.screen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QS/400's, as slic passes it.
    #[cfg(feature = "session")]
    const IPL_COMPLETE: &str = "C6001090";

    fn console() -> Console {
        Console::with(
            Identity {
                banner: "QS/400 V1R1M0 build test".into(),
                ready_code: "C6001090".into(),
            },
            None,
        )
    }

    fn press(c: &mut Console, code: u16) {
        c.handle(Command::Key(KeyEvent {
            code,
            pressed: true,
        }));
        c.handle(Command::Key(KeyEvent {
            code,
            pressed: false,
        }));
    }

    #[cfg(feature = "session")]
    fn sign_on() -> Command {
        Command::ShowSignOn {
            system: "QS400".into(),
            subsystem: "QINTER".into(),
            display: "QSCONSOLE".into(),
            long_password: false,
        }
    }

    #[cfg(feature = "session")]
    fn src(c: &mut Console, code: &str) {
        c.handle(Command::ShowSrc {
            system: "QS400".into(),
            code: code.into(),
            text: "text".into(),
        });
    }

    #[cfg(feature = "session")]
    fn complete(c: &mut Console) {
        src(c, IPL_COMPLETE);
    }

    #[test]
    #[cfg(feature = "session")]
    fn sign_on_waits_for_the_ipl_to_complete() {
        let mut c = console();
        src(&mut c, "C6001010");
        c.handle(sign_on());
        c.handle(Command::Message {
            text: "early".into(),
            error: true,
        });
        assert_eq!(c.kind, Kind::Src);
        assert!(!c.screen().dump().contains("Sign On"));
        assert!(!c.screen().dump().contains("early"));
        src(&mut c, "C6001050");
        assert_eq!(c.kind, Kind::Src);
        complete(&mut c);
        assert_eq!(c.kind, Kind::SignOn);
        assert!(c.screen().dump().contains("Sign On"));
        // From then on it shows at once.
        src(&mut c, "C6001090");
        c.handle(sign_on());
        assert_eq!(c.kind, Kind::SignOn);
    }

    #[test]
    #[cfg(feature = "session")]
    fn sign_on_enter_sends_the_fields_and_locks() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut c = console();
        c.out = Some(tx);
        complete(&mut c);
        c.handle(sign_on());
        press(&mut c, 30); // a
        press(&mut c, 28); // Enter
        assert!(c.locked());
        assert_eq!(
            rx.try_recv().unwrap(),
            Input::SignOn(vec![
                "a".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new()
            ])
        );
        let dump = c.screen().dump();
        assert!(dump.lines().nth(23).unwrap().starts_with(INHIBITED));
        press(&mut c, 31); // s: ignored while locked
        assert_eq!(c.screen().field_text(&c.screen().fields[0]), "a");
        // Ctrl+Left resets.
        c.handle(Command::Key(KeyEvent {
            code: 29,
            pressed: true,
        }));
        press(&mut c, 105);
        c.handle(Command::Key(KeyEvent {
            code: 29,
            pressed: false,
        }));
        assert!(!c.locked());
        assert!(c.screen().dump().lines().nth(22).unwrap().trim().is_empty());
    }

    #[test]
    fn application_screens_report_every_field_and_virtual_keys_edit() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut c = console();
        c.out = Some(tx);
        let mut s = Screen::new();
        s.put(1, 1, "User", Attr::GREEN);
        s.field(1, 10, 10, false);
        s.field(2, 10, 10, true);
        c.handle(Command::ShowScreen(s));
        press(&mut c, 2); // 1
        c.handle(Command::Virtual(Key::FieldAdvance));
        for ch in "pw".chars() {
            c.handle(Command::Virtual(Key::Char(ch)));
        }
        c.handle(Command::Virtual(Key::Enter));
        assert_eq!(
            rx.try_recv(),
            Ok(Input::Enter(vec!["1".into(), "pw".into()]))
        );
        c.handle(Command::Virtual(Key::Function(12)));
        assert_eq!(
            rx.try_recv(),
            Ok(Input::Function(12, vec!["1".into(), "pw".into()]))
        );
        // No fields: Enter reports nothing.
        c.handle(Command::ShowScreen(Screen::new()));
        c.handle(Command::Virtual(Key::Enter));
        assert!(rx.try_recv().is_err());
    }

    /// A writer the test keeps a hold of.
    #[derive(Clone, Default)]
    struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_banner_is_the_callers_and_a_serial_display_shows_the_same_screen() {
        let serial = Shared::default();
        let mut c = Console::with(
            Identity {
                banner: "QSDEMO V0R3M0 build 1".into(),
                ready_code: String::new(),
            },
            None,
        );
        c.add_display(Box::new(crate::ansi::AnsiDisplay::new(serial.clone())));
        c.handle(Command::ShowSrc {
            system: "DEMO1".into(),
            code: "C1001010".into(),
            text: "starting".into(),
        });
        let dump = c.screen().dump();
        assert!(dump.starts_with("QSDEMO V0R3M0 build 1"));
        assert!(!dump.contains("QS/400"));
        let sent = String::from_utf8_lossy(&serial.0.lock().unwrap()).into_owned();
        assert!(sent.contains("QSDEMO V0R3M0 build 1"), "{sent:?}");
        assert!(sent.contains("starting"));
        // A key from the serial terminal is a virtual key.
        let (tx, rx) = std::sync::mpsc::channel();
        c.out = Some(tx);
        c.handle(Command::Virtual(Key::Function(3)));
        assert_eq!(rx.try_recv(), Ok(Input::Function(3, Vec::new())));
    }
}

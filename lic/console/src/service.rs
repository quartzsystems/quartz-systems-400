// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console service: owns the screen and the keyboard state, redraws
//! on every command, answers `Dump`, and reports Enter and function
//! keys on screens whose meaning belongs to slic. Runs on its own
//! thread.

use std::sync::mpsc::{Receiver, Sender};

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

/// The code after which the sign-on display may show.
pub const IPL_COMPLETE: &str = "C6001090";

/// What slic asks of the console.
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
    /// A key from the service console (`key`, `type`), already a 5250 key.
    Virtual(Key),
    /// Reply with the 24 lines of the grid.
    Dump(Sender<String>),
    /// A TN5250E session to the system's telnet server is up: the
    /// console becomes its terminal; the operator's records go out
    /// on the sender.
    Attach(Sender<Vec<u8>>),
    /// A record from the session.
    Record(Vec<u8>),
    /// Reply with the display's mode and scale, asking the framebuffer
    /// for its mode again first.
    Describe(Sender<String>),
    /// The session ended: the console's own screens again.
    Detach,
}

/// What the console reports back to slic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Enter on an application screen, with every input field in order.
    Enter(Vec<String>),
    /// A function key on any screen, with an application screen's
    /// input fields in order (a prompt reads them on F10 as on Enter).
    Function(u8, Vec<String>),
    /// Enter on the sign-on display: user, password, program, menu,
    /// current library.
    SignOn(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Src,
    App,
    SignOn,
    /// A 5250 terminal of the system's own telnet server.
    Terminal,
}

/// The console state.
pub struct Console {
    renderer: Option<Renderer>,
    screen: Screen,
    kind: Kind,
    recent: Vec<(String, String)>,
    system: String,
    mods: Modifiers,
    locked: bool,
    out: Option<Sender<Input>>,
    /// The terminal, while attached to a session.
    term: Option<crate::terminal::ConsoleTerminal>,
    /// Where the terminal's records go.
    link: Option<Sender<Vec<u8>>>,
    /// `C6001090` was shown: sign-on may show from now on.
    complete: bool,
    /// A sign-on asked for before that, shown when it comes.
    pending: Option<Command>,
}

impl Console {
    /// Open the framebuffer. Without one the console still keeps a screen,
    /// so `Dump` works over serial.
    pub fn open() -> (Self, Result<(), FbError>) {
        Self::open_with(true)
    }

    /// Open with or without the display: the serial console mode keeps
    /// the screen in memory only and never touches the framebuffer.
    pub fn open_with(display: bool) -> (Self, Result<(), FbError>) {
        Self::open_scaled(display, Scale::Auto)
    }

    /// Open with the scale chosen on the command line.
    pub fn open_scaled(display: bool, scale: Scale) -> (Self, Result<(), FbError>) {
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
        (
            Self {
                renderer,
                screen: Screen::new(),
                kind: Kind::Src,
                recent: Vec::new(),
                system: String::new(),
                mods: Modifiers::default(),
                locked: false,
                out: None,
                term: None,
                link: None,
                complete: false,
                pending: None,
            },
            status,
        )
    }

    /// Whether the console is a session's terminal.
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

    /// Apply one command and redraw.
    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::ShowSrc { system, code, text } => {
                self.system = system;
                self.recent.push((code.clone(), text.clone()));
                self.screen = screens::src_screen(&self.system, &code, &text, &self.recent);
                self.kind = Kind::Src;
                self.locked = false;
                if code == IPL_COMPLETE {
                    self.complete = true;
                    if let Some(sign_on) = self.pending.take() {
                        return self.handle(sign_on);
                    }
                }
            }
            // Until the IPL completes the SRC display stays up.
            cmd @ Command::ShowSignOn { .. } if !self.complete => {
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
            Command::Attach(link) => {
                self.term = Some(crate::terminal::ConsoleTerminal::new());
                self.link = Some(link);
                self.kind = Kind::Terminal;
                self.locked = false;
                self.screen = Screen::new();
            }
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
            Command::Detach => self.detach(),
        }
        self.redraw();
    }

    /// The grid as text: the terminal's while attached.
    fn dump(&self) -> String {
        match &self.term {
            Some(t) if self.kind == Kind::Terminal => {
                let mut s = t.text_rows().join("\n");
                s.push('\n');
                s
            }
            _ => self.screen.dump(),
        }
    }

    /// The session ended: the last SRC display again.
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
                Some((code, text)) => screens::src_screen(&self.system, code, text, &self.recent),
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

    /// Enter: an application screen's fields go to slic; sign-on locks
    /// the keyboard; a screen without fields ignores Enter.
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
            Kind::SignOn => {
                // slic signs on and shows the job's first screen, or this
                // one again with the reason.
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
            Kind::Src | Kind::Terminal => false,
        }
    }

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console() -> Console {
        Console {
            renderer: None,
            screen: Screen::new(),
            kind: Kind::Src,
            recent: Vec::new(),
            system: String::new(),
            mods: Modifiers::default(),
            locked: false,
            out: None,
            term: None,
            link: None,
            complete: false,
            pending: None,
        }
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

    fn sign_on() -> Command {
        Command::ShowSignOn {
            system: "QS400".into(),
            subsystem: "QINTER".into(),
            display: "QSCONSOLE".into(),
            long_password: false,
        }
    }

    fn src(c: &mut Console, code: &str) {
        c.handle(Command::ShowSrc {
            system: "QS400".into(),
            code: code.into(),
            text: "text".into(),
        });
    }

    fn complete(c: &mut Console) {
        src(c, IPL_COMPLETE);
    }

    #[test]
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
            Input::SignOn(vec!["a".into(), String::new(), String::new(), String::new(), String::new()])
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
        assert_eq!(rx.try_recv(), Ok(Input::Function(12, vec!["1".into(), "pw".into()])));
        // No fields: Enter reports nothing.
        c.handle(Command::ShowScreen(Screen::new()));
        c.handle(Command::Virtual(Key::Enter));
        assert!(rx.try_recv().is_err());
    }
}

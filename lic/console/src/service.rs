// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console service: owns the screen and the keyboard state, redraws
//! on every command, answers `Dump`, and reports Enter and function
//! keys on screens whose meaning belongs to its caller. Runs on its own
//! thread. What names the product on its screens is the caller's
//! ([`Identity`]). A pointer is the caller's choice too
//! ([`Console::set_pointer`]).

use std::sync::mpsc::{Receiver, Sender};

use crate::display::{Display, View};
use crate::editor::{self, Outcome};
use crate::fb::{FbError, Framebuffer};
use crate::keyboard::KeyEvent;
use crate::keyboard::map::{Key, Modifiers, translate, update_modifiers};
use crate::pointer::{PointerEvent, Position};
use crate::render::Renderer;
use crate::scale::{Layout, Scale};
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
    /// A mouse or a tablet on the framebuffer display, from an event
    /// device reader ([`crate::keyboard::decode`]); ignored unless the
    /// pointer is on.
    Pointer(PointerEvent),
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
    /// The console's own screens take over from its session for a
    /// while (service tools on a running system): the session's records
    /// keep its terminal current without showing it.
    Interrupt,
    /// The session's screen again after [`Command::Interrupt`].
    Resume,
    /// Draw again: a display asked for a whole frame.
    Redraw,
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

/// The pointer on the framebuffer display.
#[derive(Debug, Default)]
struct PointerState {
    /// Pointer events are taken.
    on: bool,
    /// Where it is, once it reported.
    at: Option<Position>,
    /// The cell under it is marked: a mouse moved it (a tablet's host
    /// shows a pointer of its own).
    shown: bool,
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
    /// The session waits behind the console's own screens.
    interrupted: bool,
    /// The pointer, when the caller wants one.
    pointer: PointerState,
    /// The display's size and grid, for tests without a framebuffer.
    #[cfg(test)]
    geometry: Option<((usize, usize), Layout)>,
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
            interrupted: false,
            pointer: PointerState::default(),
            #[cfg(test)]
            geometry: None,
        }
    }

    /// Show the screens on another display too (a serial terminal), from
    /// the next redraw on. Its keys come in as [`Command::Virtual`].
    pub fn add_display(&mut self, mut display: Box<dyn Display>) {
        display.set_pointer(self.pointer.on);
        self.displays.push(display);
    }

    /// Take a pointer, or not (the default). On, [`Command::Pointer`] moves
    /// and clicks on the framebuffer, a mouse's cell shown in reverse
    /// video, and the other displays ask their terminals for the mouse
    /// ([`Display::set_pointer`]); a click is [`Key::Pointer`] and the
    /// wheel Page Up and Page Down. Off, pointer events are ignored.
    pub fn set_pointer(&mut self, on: bool) {
        self.pointer.on = on;
        if !on {
            self.pointer.shown = false;
        }
        for d in &mut self.displays {
            d.set_pointer(on);
        }
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
            Command::Key(ev) if !self.key(ev) => return,
            Command::Pointer(ev) if !self.pointer(ev) => return,
            Command::Key(_) | Command::Pointer(_) => {}
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
                // Behind the console's own screens: kept, not shown.
                if !self.interrupted {
                    self.screen = t.screen();
                }
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
            cmd @ (Command::Interrupt | Command::Resume | Command::Redraw) => {
                if !self.over_session(&cmd) {
                    return;
                }
            }
        }
        self.redraw();
    }

    /// The console's own screens over its session, the session again, a
    /// plain redraw. Whether to draw.
    fn over_session(&mut self, cmd: &Command) -> bool {
        match cmd {
            Command::Interrupt => {
                self.interrupted = true;
                return false;
            }
            Command::Resume => {
                self.interrupted = false;
                #[cfg(feature = "session")]
                if let Some(t) = &self.term {
                    self.kind = Kind::Terminal;
                    self.locked = false;
                    self.screen = t.screen();
                }
            }
            _ => {}
        }
        true
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

    /// The display's size in pixels and where the grid sits on it.
    fn geometry(&self) -> Option<((usize, usize), Layout)> {
        #[cfg(test)]
        if self.geometry.is_some() {
            return self.geometry;
        }
        let r = self.renderer.as_ref()?;
        let m = r.mode();
        Some(((m.xres, m.yres), r.layout()))
    }

    /// The pointer, fitted to the display as it is now.
    fn pointer_here(&self) -> Option<(Position, Layout)> {
        let ((w, h), layout) = self.geometry()?;
        let at = self
            .pointer
            .at
            .unwrap_or_else(|| Position::new(w, h))
            .fitted(w, h);
        Some((at, layout))
    }

    /// The cell to mark under a mouse's pointer, 0-based.
    fn pointer_mark(&self) -> Option<(usize, usize)> {
        if !(self.pointer.on && self.pointer.shown) {
            return None;
        }
        let (at, layout) = self.pointer_here()?;
        let (row, col) = at.cell(&layout)?;
        Some((usize::from(row), usize::from(col)))
    }

    /// Handle a pointer event. Returns whether the screen changed.
    fn pointer(&mut self, ev: PointerEvent) -> bool {
        if !self.pointer.on {
            return false;
        }
        if let PointerEvent::Wheel { up } = ev {
            return self.apply(if up { Key::PageUp } else { Key::PageDown });
        }
        let Some((mut at, layout)) = self.pointer_here() else {
            return false;
        };
        let before = (self.pointer.shown, at.cell(&layout));
        match ev {
            PointerEvent::Move { dx, dy } => {
                at.moved(dx, dy);
                self.pointer.shown = true;
            }
            PointerEvent::Absolute { axis, at: v } => {
                at.place(axis, v);
                self.pointer.shown = false;
            }
            PointerEvent::Button { pressed } => {
                self.pointer.at = Some(at);
                // A press outside the grid does nothing.
                return pressed
                    && at
                        .cell(&layout)
                        .is_some_and(|(row, col)| self.apply(Key::Pointer { row, col }));
            }
            PointerEvent::Wheel { .. } => {}
        }
        self.pointer.at = Some(at);
        (self.pointer.shown, at.cell(&layout)) != before
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
        let mark = self.pointer_mark();
        if let Some(r) = self.renderer.as_mut() {
            let _ = match mark {
                Some((row, col)) => r.draw(&marked(&self.screen, row, col)),
                None => r.draw(&self.screen),
            };
        }
        if self.displays.is_empty() {
            return;
        }
        let view = self.view();
        for d in &mut self.displays {
            let _ = d.draw_view(&self.screen, &view);
        }
    }

    /// What shows beside the cells.
    fn view(&self) -> View {
        #[cfg(feature = "session")]
        let (session, term_locked) = match &self.term {
            Some(t) if self.kind == Kind::Terminal => (true, t.locked()),
            _ => (false, false),
        };
        #[cfg(not(feature = "session"))]
        let (session, term_locked) = (false, false);
        View {
            src: if self.kind == Kind::Src {
                self.recent.last().cloned()
            } else {
                None
            },
            inhibited: self.locked || term_locked,
            insert: self.screen.insert,
            session,
        }
    }
}

/// The screen with one cell in reverse video: the pointer's.
fn marked(screen: &Screen, row: usize, col: usize) -> Screen {
    let mut s = screen.clone();
    let cell = s.cell_mut(row, col);
    cell.attr.reverse = !cell.attr.reverse;
    s
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
    #[cfg(feature = "session")]
    fn service_tools_over_a_session_and_back() {
        let (tx, rx) = std::sync::mpsc::channel();
        let (link, records) = std::sync::mpsc::channel();
        let mut c = console();
        c.out = Some(tx);
        complete(&mut c);
        c.handle(Command::Attach(link));
        assert!(c.attached());
        c.handle(Command::Interrupt);
        let mut s = Screen::new();
        s.put(1, 1, "Dedicated Service Tools", Attr::WHITE);
        s.field(5, 10, 10, false);
        c.handle(Command::ShowScreen(s));
        assert!(!c.attached());
        assert!(c.dump().contains("Dedicated Service Tools"));
        // Keys are the service tools' now, not the session's.
        c.handle(Command::Virtual(Key::Char('1')));
        c.handle(Command::Virtual(Key::Enter));
        assert_eq!(rx.try_recv(), Ok(Input::Enter(vec!["1".into()])));
        assert!(records.try_recv().is_err());
        c.handle(Command::Resume);
        assert!(c.attached());
        assert!(!c.dump().contains("Dedicated Service Tools"));
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

    /// A screen with two fields and a keys line, its fields reported.
    fn fields_screen(c: &mut Console) -> std::sync::mpsc::Receiver<Input> {
        let (tx, rx) = std::sync::mpsc::channel();
        c.out = Some(tx);
        let mut s = Screen::new();
        s.field(5, 10, 10, false);
        s.field(6, 10, 10, false);
        s.put(23, 2, "F3=Exit   Enter=Continue", Attr::GREEN);
        c.handle(Command::ShowScreen(s));
        rx
    }

    #[test]
    fn a_click_from_a_terminal_moves_the_cursor_or_presses_a_label() {
        let mut c = console();
        let rx = fields_screen(&mut c);
        c.handle(Command::Virtual(Key::Pointer { row: 5, col: 12 }));
        assert_eq!(c.screen().cursor, (5, 12));
        c.handle(Command::Virtual(Key::Char('x')));
        assert_eq!(c.screen().field_text(&c.screen().fields[1]), "   x");
        c.handle(Command::Virtual(Key::Pointer { row: 22, col: 3 }));
        assert_eq!(
            rx.try_recv(),
            Ok(Input::Function(3, vec![String::new(), "   x".into()]))
        );
        c.handle(Command::Virtual(Key::Pointer { row: 22, col: 15 }));
        assert_eq!(
            rx.try_recv(),
            Ok(Input::Enter(vec![String::new(), "   x".into()]))
        );
        // Off the fields and the labels: nothing.
        c.handle(Command::Virtual(Key::Pointer { row: 0, col: 0 }));
        assert_eq!(c.screen().cursor, (5, 13));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    #[cfg(feature = "session")]
    fn a_click_while_the_keyboard_is_locked_does_nothing() {
        let mut c = console();
        complete(&mut c);
        c.handle(sign_on());
        press(&mut c, 28); // Enter: locked
        assert!(c.locked());
        let before = c.screen().cursor;
        let f = c.screen().fields[1];
        let (row, col) = (
            u16::try_from(f.row - 1).unwrap(),
            u16::try_from(f.col).unwrap(),
        );
        c.handle(Command::Virtual(Key::Pointer { row, col }));
        assert_eq!(c.screen().cursor, before);
    }

    /// 1024 by 768: scale 1, the grid from (152, 192).
    fn with_display(c: &mut Console) -> Layout {
        let l = Layout::compute(1024, 768, ROWS, COLS, Scale::Auto);
        c.geometry = Some(((1024, 768), l));
        l
    }

    fn pointer(c: &mut Console, ev: PointerEvent) {
        c.handle(Command::Pointer(ev));
    }

    #[test]
    fn the_pointer_is_off_until_the_caller_turns_it_on() {
        let mut c = console();
        let rx = fields_screen(&mut c);
        with_display(&mut c);
        pointer(&mut c, PointerEvent::Move { dx: 5, dy: 5 });
        pointer(&mut c, PointerEvent::Button { pressed: true });
        pointer(&mut c, PointerEvent::Wheel { up: true });
        assert_eq!(c.pointer.at, None);
        assert_eq!(c.pointer_mark(), None);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_mouse_moves_shows_and_clicks_through_the_layout() {
        let mut c = console();
        let rx = fields_screen(&mut c);
        let l = with_display(&mut c);
        c.set_pointer(true);
        // To the top left corner, then onto row 6 (0-based 5), column 13.
        pointer(
            &mut c,
            PointerEvent::Move {
                dx: -5000,
                dy: -5000,
            },
        );
        assert_eq!(c.pointer_mark(), None, "outside the grid");
        let (x, y) = (l.x0 + 12 * 9 + 4, l.y0 + 5 * 16 + 8);
        pointer(
            &mut c,
            PointerEvent::Move {
                dx: i32::try_from(x).unwrap(),
                dy: i32::try_from(y).unwrap(),
            },
        );
        assert_eq!(c.pointer_mark(), Some((5, 12)));
        assert!(marked(c.screen(), 5, 12).cell(5, 12).attr.reverse);
        pointer(&mut c, PointerEvent::Button { pressed: true });
        assert_eq!(c.screen().cursor, (5, 12));
        pointer(&mut c, PointerEvent::Button { pressed: false });
        assert_eq!(c.screen().cursor, (5, 12));
        // A tablet places it absolutely, and the host shows its own.
        pointer(
            &mut c,
            PointerEvent::Absolute {
                axis: crate::pointer::Axis::Y,
                at: crate::pointer::ABS_FULL,
            },
        );
        assert_eq!(c.pointer_mark(), None);
        pointer(&mut c, PointerEvent::Button { pressed: true });
        assert_eq!(c.screen().cursor, (5, 12), "below the grid: nothing");
        // Onto F3=Exit, absolutely.
        let share = |px: usize, n: usize| {
            u16::try_from(px * usize::from(crate::pointer::ABS_FULL) / (n - 1) + 1).unwrap()
        };
        let (x, y) = (l.x0 + 3 * 9 + 2, l.y0 + 22 * 16 + 2);
        for (axis, at) in [
            (crate::pointer::Axis::X, share(x, 1024)),
            (crate::pointer::Axis::Y, share(y, 768)),
        ] {
            pointer(&mut c, PointerEvent::Absolute { axis, at });
        }
        pointer(&mut c, PointerEvent::Button { pressed: true });
        assert_eq!(
            rx.try_recv(),
            Ok(Input::Function(3, vec![String::new(), String::new()]))
        );
        // A mode change keeps the pointer on the display.
        c.geometry = Some((
            (640, 480),
            Layout::compute(640, 480, ROWS, COLS, Scale::Auto),
        ));
        pointer(&mut c, PointerEvent::Move { dx: 9999, dy: 9999 });
        assert_eq!(c.pointer.at.unwrap().xy(), (639, 479));
        assert_eq!(c.pointer.at.unwrap().size(), (640, 480));
        // Off again: the mark goes.
        c.set_pointer(false);
        assert_eq!(c.pointer_mark(), None);
    }

    #[test]
    #[cfg(feature = "session")]
    fn the_wheel_pages() {
        use tn5250::ds::attr;
        use tn5250::ds::field::Ffw;
        use tn5250::record::Record;
        use tn5250::screen::{Read, ScreenModel};
        let mut c = console();
        complete(&mut c);
        let (link, records) = std::sync::mpsc::channel();
        c.handle(Command::Attach(link));
        let mut m = ScreenModel::new(false);
        m.field(6, 53, 10, attr::GREEN_UL, "", Ffw::default(), &[]);
        let out = m.render(Read::MdtFields);
        c.handle(Command::Record(Record::new(out.opcode, out.data).encode()));
        // Off: the wheel is nothing.
        pointer(&mut c, PointerEvent::Wheel { up: false });
        assert!(records.try_recv().is_err());
        // On: Page Down (Roll Up) goes to the host.
        c.set_pointer(true);
        pointer(&mut c, PointerEvent::Wheel { up: false });
        assert!(records.try_recv().is_ok());
    }

    #[test]
    fn a_serial_display_is_asked_for_the_mouse_with_the_pointer() {
        let serial = Shared::default();
        let mut c = console();
        c.add_display(Box::new(crate::ansi::AnsiDisplay::new(serial.clone())));
        c.handle(Command::Redraw);
        assert!(!String::from_utf8_lossy(&serial.0.lock().unwrap()).contains("\x1b[?1000h"));
        c.set_pointer(true);
        c.handle(Command::Redraw);
        assert!(
            String::from_utf8_lossy(&serial.0.lock().unwrap()).contains(crate::ansi::POINTER_ON)
        );
        // A display added later is asked too.
        let later = Shared::default();
        c.add_display(Box::new(crate::ansi::AnsiDisplay::new(later.clone())));
        c.handle(Command::Redraw);
        assert!(
            String::from_utf8_lossy(&later.0.lock().unwrap()).starts_with(crate::ansi::POINTER_ON)
        );
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

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console service: owns the screen and the keyboard state, redraws
//! on every command, answers `Dump`, and reports Enter on screens whose
//! meaning belongs to slic. Runs on its own thread.

use std::sync::mpsc::{Receiver, Sender};

use crate::editor::{self, Outcome};
use crate::fb::{FbError, Framebuffer};
use crate::keyboard::KeyEvent;
use crate::keyboard::map::{Modifiers, translate, update_modifiers};
use crate::render::Renderer;
use crate::screen::{Attr, Screen};
use crate::screens;

/// Message shown when Enter is pressed on the sign-on display.
pub const SIGN_ON_UNAVAILABLE: &str = "Sign-on not available in this release";
/// Input-inhibited indicator, bottom left.
pub const INHIBITED: &str = "X II";

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
    /// Show the manual-mode IPL menu.
    ShowMenu {
        /// System name.
        system: String,
    },
    /// Show the sign-on display.
    ShowSignOn {
        /// System name.
        system: String,
        /// Subsystem name.
        subsystem: String,
        /// Display name.
        display: String,
    },
    /// Show a dialog: title, text lines, and a prompt with an input field
    /// of the given length when one is wanted.
    ShowDialog {
        /// System name.
        system: String,
        /// Title, centered on row 1.
        title: String,
        /// Text lines from row 4.
        lines: Vec<String>,
        /// Prompt label and field length; `None` for a notice.
        prompt: Option<(String, usize)>,
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
    /// Reply with the 24 lines of the grid.
    Dump(Sender<String>),
}

/// What the console reports back to slic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Enter on the IPL menu or a dialog, with the input field.
    MenuSelection(String),
    /// A function key on any screen.
    Function(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Src,
    Menu,
    Dialog,
    SignOn,
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
}

impl Console {
    /// Open the framebuffer. Without one the console still keeps a screen,
    /// so `Dump` works over serial.
    pub fn open() -> (Self, Result<(), FbError>) {
        let (renderer, status) =
            match Framebuffer::open().and_then(|fb| Renderer::new(fb).map_err(FbError::from)) {
                Ok(r) => (Some(r), Ok(())),
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
            },
            status,
        )
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
            }
            Command::ShowMenu { system } => {
                self.system = system;
                self.screen = screens::ipl_menu(&self.system);
                self.kind = Kind::Menu;
                self.locked = false;
                editor::home(&mut self.screen);
            }
            Command::ShowSignOn {
                system,
                subsystem,
                display,
            } => {
                self.system = system;
                self.screen = screens::sign_on(&self.system, &subsystem, &display);
                self.kind = Kind::SignOn;
                self.locked = false;
                editor::home(&mut self.screen);
            }
            Command::ShowDialog {
                system,
                title,
                lines,
                prompt,
            } => {
                self.system = system;
                let prompt = prompt.as_ref().map(|(label, len)| (label.as_str(), *len));
                self.screen = screens::dialog(&self.system, &title, &lines, prompt);
                self.kind = Kind::Dialog;
                self.locked = false;
                editor::home(&mut self.screen);
            }
            Command::Message { text, error } => {
                screens::set_message(&mut self.screen, &text, error);
            }
            Command::Key(ev) => {
                if !self.key(ev) {
                    return;
                }
            }
            Command::Dump(reply) => {
                let _ = reply.send(self.screen.dump());
                return;
            }
        }
        self.redraw();
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
                self.send(Input::Function(n));
                false
            }
            Outcome::Reset | Outcome::Attention | Outcome::Ignored => false,
        }
    }

    /// Enter: the menu or dialog field goes to slic; sign-on locks the
    /// keyboard; a dialog without a field ignores Enter.
    fn enter(&mut self) -> bool {
        match self.kind {
            Kind::Menu | Kind::Dialog => {
                if let Some(f) = self.screen.fields.first() {
                    let text = self.screen.field_text(f);
                    self.send(Input::MenuSelection(text));
                }
                false
            }
            Kind::SignOn => {
                self.lock(SIGN_ON_UNAVAILABLE);
                true
            }
            Kind::Src => false,
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

    fn redraw(&self) {
        if let Some(r) = &self.renderer {
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

    #[test]
    fn sign_on_enter_locks_until_reset() {
        let mut c = console();
        c.handle(Command::ShowSignOn {
            system: "QS400".into(),
            subsystem: "QINTER".into(),
            display: "QSCONSOLE".into(),
        });
        press(&mut c, 30); // a
        press(&mut c, 28); // Enter
        assert!(c.locked());
        let dump = c.screen().dump();
        assert!(
            dump.lines()
                .nth(22)
                .unwrap()
                .starts_with(SIGN_ON_UNAVAILABLE)
        );
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
    fn dialog_enter_reports_the_field_or_nothing() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut c = console();
        c.out = Some(tx);
        c.handle(Command::ShowDialog {
            system: "QS400".into(),
            title: "Install the Operating System".into(),
            lines: vec!["Type YES to continue".into()],
            prompt: Some(("Answer".into(), 3)),
        });
        press(&mut c, 21); // y
        press(&mut c, 28);
        assert_eq!(rx.try_recv(), Ok(Input::MenuSelection("y".into())));
        c.handle(Command::ShowDialog {
            system: "QS400".into(),
            title: "Working".into(),
            lines: Vec::new(),
            prompt: None,
        });
        press(&mut c, 28);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn menu_enter_reports_the_selection() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut c = console();
        c.out = Some(tx);
        c.handle(Command::ShowMenu {
            system: "QS400".into(),
        });
        press(&mut c, 2); // 1
        press(&mut c, 28);
        assert_eq!(rx.try_recv(), Ok(Input::MenuSelection("1".into())));
    }
}

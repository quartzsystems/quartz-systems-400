// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console service: owns the screen, redraws on every command, and
//! answers `Dump` with the text grid. Runs on its own thread; slic talks to
//! it over a channel.

use std::sync::mpsc::{Receiver, Sender};

use crate::fb::{FbError, Framebuffer};
use crate::render::Renderer;
use crate::screen::Screen;
use crate::screens;

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
    /// Replace the message line.
    Message {
        /// Text.
        text: String,
        /// Shown in red.
        error: bool,
    },
    /// Reply with the 24 lines of the grid.
    Dump(Sender<String>),
}

/// The console state.
pub struct Console {
    renderer: Option<Renderer>,
    screen: Screen,
    recent: Vec<(String, String)>,
    system: String,
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
                recent: Vec::new(),
                system: String::new(),
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

    /// Serve commands until the channel closes.
    pub fn run(mut self, rx: &Receiver<Command>) {
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
            }
            Command::ShowMenu { system } => {
                self.system = system;
                self.screen = screens::ipl_menu(&self.system);
            }
            Command::ShowSignOn {
                system,
                subsystem,
                display,
            } => {
                self.system = system;
                self.screen = screens::sign_on(&self.system, &subsystem, &display);
            }
            Command::Message { text, error } => {
                screens::set_message(&mut self.screen, &text, error);
            }
            Command::Dump(reply) => {
                let _ = reply.send(self.screen.dump());
                return;
            }
        }
        if let Some(r) = &self.renderer {
            let _ = r.draw(&self.screen);
        }
    }

    /// The current screen.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }
}

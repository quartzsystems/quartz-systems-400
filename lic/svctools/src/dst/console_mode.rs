// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Select DST console mode`: the local console (the display and its
//! keyboard) or the serial console (`ttyS0`), kept in the LIC record
//! and honoured at the next IPL. The tag and its byte are `licpart`'s
//! (`licpart::console`); the screens' words for a mode are here.

use console::Screen;

use crate::dst::{INVALID_SELECTION, selection};
use crate::env::DstEnv;
use crate::panel::{Flow, Panel};
use crate::ui;

pub use licpart::console::{ConsoleMode, TAG_CONSOLE_MODE};

/// How the screens name a mode (the tag and the byte are `licpart`'s).
pub trait ModeText {
    /// The mode as the screens name it.
    fn text(self) -> &'static str;
}

impl ModeText for ConsoleMode {
    fn text(self) -> &'static str {
        match self {
            Self::Local => "Local console (display and keyboard)",
            Self::Serial => "Serial console (ttyS0)",
        }
    }
}

/// The options.
const OPTIONS: [(u8, &str); 2] = [
    (1, "Local console (display and keyboard)"),
    (2, "Serial console (ttyS0)"),
];

/// The `Select DST Console Mode` panel.
pub struct SelectConsoleMode;

impl Panel for SelectConsoleMode {
    fn name(&self) -> &'static str {
        "dst-console-mode"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::menu(
            "Select DST Console Mode",
            &env.system_name(),
            &OPTIONS,
            "F3=Exit   F12=Cancel",
        );
        s.put(
            ui::BODY_ROW + 6,
            ui::BODY_COL,
            &format!(
                "Console mode now . . . . . :  {}",
                env.console_mode().text()
            ),
            console::Attr::GREEN,
        );
        s.put(
            ui::BODY_ROW + 8,
            ui::BODY_COL,
            "The choice is kept in the Licensed Internal Code and takes effect",
            console::Attr::GREEN,
        );
        s.put(
            ui::BODY_ROW + 9,
            ui::BODY_COL,
            "at the next IPL. The serial console echoes every screen either way.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let Some(mode) = selection(fields).and_then(ConsoleMode::from_u8) else {
            return Flow::Error(INVALID_SELECTION.into());
        };
        match env.set_console_mode(mode) {
            Ok(()) => Flow::Unwind(
                1,
                Some((
                    format!("Console mode {} takes effect at the next IPL", mode.text()),
                    false,
                )),
            ),
            Err(e) => Flow::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;
    use crate::panel::Runner;

    #[test]
    fn selecting_a_mode_records_it() {
        let mut env = MemEnv::new("QS400");
        let mut r = Runner::new(Box::new(SelectConsoleMode));
        assert!(r.screen(&mut env).dump().contains("Local console"));
        assert_eq!(r.enter(&["x".to_string()], &mut env), None);
        assert!(r.screen(&mut env).dump().contains(INVALID_SELECTION));
        r.enter(&["2".to_string()], &mut env);
        assert_eq!(env.console_mode, ConsoleMode::Serial);
        assert_eq!(env.actions, vec!["console Serial"]);
    }
}

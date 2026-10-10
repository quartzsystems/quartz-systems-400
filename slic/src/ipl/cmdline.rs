// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Boot parameters from `/proc/cmdline`: `qs400.slot=A|B`,
//! `qs400.mode=normal|manual`, `qs400.svc=1`.

use std::fs;

use licpart::{IplMode, Slot};

/// Parsed boot parameters.
#[derive(Debug)]
pub struct Boot {
    /// Boot slot. Defaults to A when absent.
    pub slot: Slot,
    /// Whether the slot was actually on the command line.
    pub slot_from_cmdline: bool,
    /// Keylock from the command line, if given. Wins over the LIC override.
    pub mode: Option<IplMode>,
    /// Serial service console enabled.
    pub svc: bool,
}

impl Boot {
    /// Read and parse `/proc/cmdline`. Missing file means defaults.
    pub fn read() -> Self {
        let text = fs::read_to_string("/proc/cmdline").unwrap_or_default();
        Self::parse(&text)
    }

    /// Parse a command line string.
    pub fn parse(text: &str) -> Self {
        let mut boot = Self {
            slot: Slot::A,
            slot_from_cmdline: false,
            mode: None,
            svc: false,
        };
        for word in text.split_whitespace() {
            let Some((key, value)) = word.split_once('=') else {
                continue;
            };
            match key {
                "qs400.slot" => match value {
                    "A" | "a" => {
                        boot.slot = Slot::A;
                        boot.slot_from_cmdline = true;
                    }
                    "B" | "b" => {
                        boot.slot = Slot::B;
                        boot.slot_from_cmdline = true;
                    }
                    _ => {}
                },
                "qs400.mode" => match value {
                    "normal" => boot.mode = Some(IplMode::Normal),
                    "manual" => boot.mode = Some(IplMode::Manual),
                    _ => {}
                },
                "qs400.svc" => boot.svc = value == "1",
                _ => {}
            }
        }
        boot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_keys_and_ignores_the_rest() {
        let b = Boot::parse("console=ttyS0 qs400.slot=B qs400.mode=manual qs400.svc=1 quiet");
        assert_eq!(b.slot, Slot::B);
        assert!(b.slot_from_cmdline);
        assert_eq!(b.mode, Some(IplMode::Manual));
        assert!(b.svc);
        let d = Boot::parse("console=ttyS0");
        assert_eq!(d.slot, Slot::A);
        assert!(!d.slot_from_cmdline);
        assert_eq!(d.mode, None);
        assert!(!d.svc);
    }
}

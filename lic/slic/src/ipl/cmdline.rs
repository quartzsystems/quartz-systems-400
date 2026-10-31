// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Boot parameters from `/proc/cmdline`: `qs400.slot=A|B`,
//! `qs400.mode=normal|manual`, `qs400.svc=1`, `qs400.ipl=D`, `qs400.media=iso`,
//! `qs400.pace=MS` (how long each IPL code and install step stays up),
//! `qs400.console.scale=auto|N` (the console scale, a development aid).

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
    /// IPL type D: booted from installation media; no LIC partition required.
    pub ipl_d: bool,
    /// The media kind named on the command line (`iso`).
    pub media: Option<String>,
    /// Milliseconds each IPL code and install step stays on the display
    /// (`qs400.pace`); the default when absent.
    pub pace: Option<u64>,
    /// The first Ethernet link's IPv4 address and prefix
    /// (`qs400.ip=10.0.2.15/24`); QEMU's user network when absent.
    pub ip: Option<String>,
    /// The console scale (`qs400.console.scale=auto|N`); auto when absent.
    pub console_scale: console::Scale,
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
            ipl_d: false,
            media: None,
            pace: None,
            ip: None,
            console_scale: console::Scale::Auto,
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
                "qs400.ipl" => boot.ipl_d = value.eq_ignore_ascii_case("d"),
                "qs400.media" => boot.media = Some(value.to_string()),
                "qs400.pace" => boot.pace = value.parse().ok(),
                "qs400.ip" => boot.ip = Some(value.to_string()),
                "qs400.console.scale" => boot.console_scale = console::Scale::parse(value),
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
        assert!(!d.ipl_d);
        let m = Boot::parse("qs400.ipl=D qs400.media=iso");
        assert!(m.ipl_d);
        assert_eq!(m.media.as_deref(), Some("iso"));
        assert_eq!(d.console_scale, console::Scale::Auto);
    }

    #[test]
    fn parses_the_console_scale() {
        let fixed = Boot::parse("qs400.console.scale=3");
        assert_eq!(fixed.console_scale, console::Scale::Fixed(3));
        let auto = Boot::parse("qs400.console.scale=auto");
        assert_eq!(auto.console_scale, console::Scale::Auto);
    }
}

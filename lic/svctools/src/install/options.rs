// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `IPL Options` and `Set Major System Options`, shown once the system
//! values exist, and their records.

use console::Screen;

use crate::env::DstEnv;
use crate::panel::{Flow, Outcome, Panel, Runner};
use crate::ui::{self, FormLine};

/// The TLV tag the IPL options are kept under in the LIC record.
pub const TAG_IPL_OPTIONS: u16 = 0x0104;

/// The `IPL Options` choices. Only the print writers one has an effect
/// today (`QSTRPRTWTR`); the rest are kept for Phase 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one per line of the display
pub struct IplOptions {
    /// Show `Set Major System Options` next.
    pub set_major_options: bool,
    /// Define or change the system at IPL.
    pub define_system: bool,
    /// Clear job queues.
    pub clear_job_queues: bool,
    /// Clear output queues.
    pub clear_output_queues: bool,
    /// Clear incomplete job logs.
    pub clear_job_logs: bool,
    /// Start print writers.
    pub start_print_writers: bool,
    /// Start the system in restricted state.
    pub restricted_state: bool,
}

impl Default for IplOptions {
    fn default() -> Self {
        Self {
            set_major_options: true,
            define_system: false,
            clear_job_queues: false,
            clear_output_queues: false,
            clear_job_logs: false,
            start_print_writers: true,
            restricted_state: false,
        }
    }
}

impl IplOptions {
    /// One byte per option.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        self.bits().iter().map(|&b| u8::from(b)).collect()
    }

    /// From the record bytes.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 7 {
            return None;
        }
        let b = |i: usize| bytes[i] != 0;
        Some(Self {
            set_major_options: b(0),
            define_system: b(1),
            clear_job_queues: b(2),
            clear_output_queues: b(3),
            clear_job_logs: b(4),
            start_print_writers: b(5),
            restricted_state: b(6),
        })
    }

    fn bits(self) -> [bool; 7] {
        [
            self.set_major_options,
            self.define_system,
            self.clear_job_queues,
            self.clear_output_queues,
            self.clear_job_logs,
            self.start_print_writers,
            self.restricted_state,
        ]
    }
}

const IPL_LABELS: [&str; 7] = [
    "Set major system options",
    "Define or change the system at IPL",
    "Clear job queues",
    "Clear output queues",
    "Clear incomplete job logs",
    "Start print writers",
    "Start system to restricted state",
];

/// The `Set Major System Options` choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MajorOptions {
    /// `QAUTOCFG`.
    pub auto_configure: bool,
    /// `QDEVNAMING`: `*NORMAL`, `*S36` or `*DEVADR`.
    pub device_naming: String,
    /// `QSPCENV`: `*NONE` or `*S36`.
    pub special_environment: String,
}

impl Default for MajorOptions {
    fn default() -> Self {
        Self {
            auto_configure: true,
            device_naming: "*NORMAL".into(),
            special_environment: "*NONE".into(),
        }
    }
}

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

fn parse_yn(s: &str) -> Result<bool, String> {
    match s.trim().to_ascii_uppercase().as_str() {
        "Y" => Ok(true),
        "N" => Ok(false),
        other => Err(format!("'{other}' is not Y or N")),
    }
}

/// The options sequence, starting at `IPL Options`.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(IplOptionsPanel))
}

/// `IPL Options`: date, time and the seven choices.
pub struct IplOptionsPanel;

impl Panel for IplOptionsPanel {
    fn name(&self) -> &'static str {
        "ipl-options"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let (date, time) = env.clock();
        let o = env.ipl_options();
        let mut lines = vec![
            FormLine::new("System date", 10, date),
            FormLine::new("System time", 8, time),
        ];
        for (label, bit) in IPL_LABELS.iter().zip(o.bits()) {
            lines.push(FormLine::new(label, 1, yn(bit)));
        }
        let mut s = ui::form(
            "IPL Options",
            &env.system_name(),
            &lines,
            "F3=Exit   F12=Cancel",
        );
        s.put(
            ui::BODY_ROW + 10,
            ui::BODY_COL,
            "Type choices, press Enter.  Date YYYY-MM-DD, time HH:MM:SS, options Y or N.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let date = fields.first().map_or("", String::as_str).trim();
        let time = fields.get(1).map_or("", String::as_str).trim();
        let mut bits = [false; 7];
        for (i, b) in bits.iter_mut().enumerate() {
            match parse_yn(fields.get(i + 2).map_or("", String::as_str)) {
                Ok(v) => *b = v,
                Err(e) => return Flow::Error(e),
            }
        }
        let options = IplOptions {
            set_major_options: bits[0],
            define_system: bits[1],
            clear_job_queues: bits[2],
            clear_output_queues: bits[3],
            clear_job_logs: bits[4],
            start_print_writers: bits[5],
            restricted_state: bits[6],
        };
        if let Err(e) = env.set_clock(date, time) {
            return Flow::Error(e);
        }
        if let Err(e) = env.set_ipl_options(&options) {
            return Flow::Error(e);
        }
        if options.set_major_options {
            Flow::Replace(Box::new(MajorOptionsPanel {
                define: options.define_system,
            }))
        } else if options.define_system {
            Flow::Replace(Box::new(super::define::DefineSystem::new()))
        } else {
            Flow::Exit(Outcome::Continue)
        }
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 | 12 => Flow::Exit(Outcome::Continue),
            _ => Flow::Stay,
        }
    }
}

/// `Set Major System Options`; `define` goes on to `Define or Change
/// the System at IPL`.
pub struct MajorOptionsPanel {
    /// Show the define menu next.
    pub define: bool,
}

impl Panel for MajorOptionsPanel {
    fn name(&self) -> &'static str {
        "major-options"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let o = env.major_options();
        let mut s = ui::form(
            "Set Major System Options",
            &env.system_name(),
            &[
                FormLine::new("Enable automatic configuration", 1, yn(o.auto_configure)),
                FormLine::new("Device configuration naming", 8, o.device_naming),
                FormLine::new("Default special environment", 8, o.special_environment),
            ],
            "F3=Exit   F12=Cancel",
        );
        s.put(
            ui::BODY_ROW + 5,
            ui::BODY_COL,
            "Type choices, press Enter.  Y or N; *NORMAL, *S36 or *DEVADR; *NONE or *S36.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let auto = match parse_yn(fields.first().map_or("", String::as_str)) {
            Ok(v) => v,
            Err(e) => return Flow::Error(e),
        };
        let naming = fields
            .get(1)
            .map_or("", String::as_str)
            .trim()
            .to_ascii_uppercase();
        let special = fields
            .get(2)
            .map_or("", String::as_str)
            .trim()
            .to_ascii_uppercase();
        if !["*NORMAL", "*S36", "*DEVADR"].contains(&naming.as_str()) {
            return Flow::Error("Device naming must be *NORMAL, *S36 or *DEVADR".into());
        }
        if !["*NONE", "*S36"].contains(&special.as_str()) {
            return Flow::Error("Special environment must be *NONE or *S36".into());
        }
        let o = MajorOptions {
            auto_configure: auto,
            device_naming: naming,
            special_environment: special,
        };
        match env.set_major_options(&o) {
            Ok(()) if self.define => Flow::Replace(Box::new(super::define::DefineSystem::new())),
            Ok(()) => Flow::Exit(Outcome::Continue),
            Err(e) => Flow::Error(e),
        }
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 | 12 => Flow::Exit(Outcome::Continue),
            _ => Flow::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn options_round_trip_and_the_panels_store_them() {
        let o = IplOptions {
            clear_job_queues: true,
            ..IplOptions::default()
        };
        assert_eq!(IplOptions::decode(&o.encode()), Some(o));
        assert!(IplOptions::decode(&[1, 0]).is_none());
        let mut e = MemEnv::new("QS400");
        let mut r = start();
        let d = r.screen(&mut e).dump();
        assert!(d.contains("Start print writers"));
        assert_eq!(
            r.enter(
                &f(&["2026-10-10", "12:00:00", "Y", "N", "N", "N", "N", "x", "N"]),
                &mut e
            ),
            None
        );
        assert!(r.screen(&mut e).dump().contains("is not Y or N"));
        assert_eq!(
            r.enter(
                &f(&["2026-10-10", "12:00:00", "Y", "N", "N", "N", "N", "Y", "N"]),
                &mut e
            ),
            None
        );
        assert_eq!(r.top(), "major-options");
        assert_eq!(
            r.enter(&f(&["N", "*s36", "*NONE"]), &mut e),
            Some(Outcome::Continue)
        );
        assert!(e.actions.contains(&"clock 2026-10-10 12:00:00".to_string()));
        assert!(e.actions.iter().any(|a| a.starts_with("ipl-options")));
        assert!(e.actions.contains(&"major N *S36 *NONE".to_string()));
        // Without major options the sequence ends at once.
        let mut r = start();
        assert_eq!(
            r.enter(
                &f(&["2026-10-10", "12:00:00", "N", "N", "N", "N", "N", "Y", "N"]),
                &mut e
            ),
            Some(Outcome::Continue)
        );
    }
}

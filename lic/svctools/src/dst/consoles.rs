// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with Management Consoles` (DST environment option 6,
//! QS/400's: `docs/QSID.md` section 12): the system's QSI fingerprint,
//! the consoles paired with it, option 4 to remove one, and F6 to make
//! a one-time pairing password, shown once. The same as the commands
//! `WRKQSICSL`, `RMVQSICSL` and `STRQSIPAIR`; the service tools user
//! is named in the audit journal's `QI` entries.

use console::{Attr, Screen};

use crate::dst::common::{Confirm, parse_options};
use crate::env::{DstEnv, QsiConsoleRow};
use crate::panel::{Flow, Panel};
use crate::ui;

/// The list of consoles, read again every time it is shown.
pub struct Consoles;

fn rows(env: &mut dyn DstEnv) -> Result<Vec<QsiConsoleRow>, String> {
    env.qsi_consoles()
}

impl Panel for Consoles {
    fn name(&self) -> &'static str {
        "dst-consoles"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let (rows, note) = match rows(env) {
            Ok(r) => (r, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let lines: Vec<String> = rows
            .iter()
            .map(|c| {
                format!(
                    "{:<20}  {:<14}  {:<14}  {}",
                    c.name.chars().take(20).collect::<String>(),
                    c.paired,
                    c.last,
                    c.fingerprint.get(..16).unwrap_or(&c.fingerprint)
                )
            })
            .collect();
        let mut s = ui::list(
            "Work with Management Consoles",
            &env.system_name(),
            "Type options, press Enter.  4=Remove",
            "Opt  Console               Paired          Last session    Fingerprint",
            &lines,
            "F3=Exit   F6=Make a pairing password   F12=Cancel",
        );
        let at = ui::KEYS_ROW - 5;
        if let Some(e) = note {
            s.put(at, ui::BODY_COL, &e, Attr::GREEN);
            return s;
        }
        if rows.is_empty() {
            s.put(
                ui::BODY_ROW + 3,
                ui::BODY_COL + 4,
                "(No consoles are paired)",
                Attr::GREEN,
            );
        }
        let fp = env.qsi_fingerprint().ok().flatten();
        s.put(at, ui::BODY_COL, "System's QSI fingerprint:", Attr::GREEN);
        match fp {
            Some(hex) => s.put(at + 1, ui::BODY_COL + 2, &hex, Attr::WHITE),
            None => s.put(
                at + 1,
                ui::BODY_COL + 2,
                "None yet: the QSI server makes it when it first starts.",
                Attr::GREEN,
            ),
        }
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let chosen = match parse_options(fields, &[4]) {
            Ok(c) => c,
            Err(e) => return Flow::Error(e),
        };
        let Some(&(row, _)) = chosen.first() else {
            return Flow::Stay;
        };
        let list = match rows(env) {
            Ok(r) => r,
            Err(e) => return Flow::Error(e),
        };
        let Some(c) = list.get(row).cloned() else {
            return Flow::Error("Entry not found".into());
        };
        let lines = vec![
            format!("Management console {} will be removed:", c.name),
            format!("  {}", c.fingerprint),
            String::new(),
            "A session it has ends, and it cannot connect again until".into(),
            "it is paired again.".into(),
            String::new(),
            "Press F10 to remove it, F12 to keep it.".into(),
        ];
        let fp = c.fingerprint.clone();
        Flow::Push(Box::new(Confirm::new(
            "dst-console-remove",
            "Confirm Remove of Management Console",
            lines,
            Box::new(move |env: &mut dyn DstEnv| env.qsi_remove(&fp)),
        )))
    }

    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        match key {
            6 => match env.qsi_start_pairing() {
                Ok(p) => Flow::Push(Box::new(Pairing {
                    password: p.0,
                    until: p.1,
                    attempts: p.2,
                    fingerprint: env.qsi_fingerprint().ok().flatten().unwrap_or_default(),
                })),
                Err(e) => Flow::Error(e),
            },
            3 => Flow::PopAll,
            12 => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

/// The pairing password, shown once.
pub struct Pairing {
    password: String,
    until: String,
    attempts: u32,
    fingerprint: String,
}

impl Panel for Pairing {
    fn name(&self) -> &'static str {
        "dst-console-pairing"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = Screen::new();
        ui::header(&mut s, "Make a Pairing Password", &env.system_name());
        let r = ui::BODY_ROW;
        let c = ui::BODY_COL;
        s.put(r, c, "System's QSI fingerprint:", Attr::GREEN);
        s.put(r + 1, c + 2, &self.fingerprint, Attr::WHITE);
        s.put(
            r + 3,
            c,
            &format!("Pairing password . . . :  {}", self.password),
            Attr::GREEN,
        );
        s.put(r + 3, c + 26, &self.password, Attr::WHITE);
        s.put(
            r + 4,
            c,
            &format!("Good until . . . . . . :  {}", self.until),
            Attr::GREEN,
        );
        s.put(
            r + 5,
            c,
            &format!("Attempts . . . . . . . :  {}", self.attempts),
            Attr::GREEN,
        );
        s.put(
            r + 7,
            c,
            "Type the password at the management console. It is good for",
            Attr::GREEN,
        );
        s.put(
            r + 8,
            c,
            "one pairing and is shown on this display and nowhere else.",
            Attr::GREEN,
        );
        s.put(
            r + 9,
            c,
            "Press F3 to leave it out until it is used, F12 to end it now.",
            Attr::GREEN,
        );
        ui::keys(&mut s, "F3=Exit   F12=End the password");
        s
    }

    // A display without fields: its keys are all it takes.
    fn enter(&mut self, _fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        Flow::Stay
    }

    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        match key {
            3 => Flow::PopAll,
            12 => match env.qsi_end_pairing() {
                Ok(()) => Flow::Unwind(1, Some(("The pairing password was ended.".into(), false))),
                Err(e) => Flow::Error(e),
            },
            _ => Flow::Stay,
        }
    }
}

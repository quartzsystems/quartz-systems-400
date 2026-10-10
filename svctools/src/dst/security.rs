// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with system security`: four flags, Y or N each.

use console::Screen;

use crate::env::{DstEnv, SecurityFlags};
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};

/// The TLV tag the flags are kept under in the LIC record.
pub const TAG_SECURITY: u16 = 0x0103;

/// The flags as one byte.
#[must_use]
pub fn encode(f: SecurityFlags) -> u8 {
    u8::from(f.allow_default_password_change)
        | (u8::from(f.allow_new_certificates) << 1)
        | (u8::from(f.allow_sysval_security_changes) << 2)
        | (u8::from(f.allow_add_certificates) << 3)
}

/// The flags from one byte.
#[must_use]
pub fn decode(b: u8) -> SecurityFlags {
    SecurityFlags {
        allow_default_password_change: b & 1 != 0,
        allow_new_certificates: b & 2 != 0,
        allow_sysval_security_changes: b & 4 != 0,
        allow_add_certificates: b & 8 != 0,
    }
}

/// The security panel.
pub struct Security;

const LABELS: [&str; 4] = [
    "Allow a service tools user ID with a default and expired password to change its own password",
    "Allow new digital certificates",
    "Allow system value security changes",
    "Allow add of digital certificates",
];

fn yn(b: bool) -> &'static str {
    if b { "Y" } else { "N" }
}

impl Panel for Security {
    fn name(&self) -> &'static str {
        "dst-security"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let f = env.security_flags();
        let values = [
            f.allow_default_password_change,
            f.allow_new_certificates,
            f.allow_sysval_security_changes,
            f.allow_add_certificates,
        ];
        let mut s = Screen::new();
        ui::header(&mut s, "Work with System Security", &env.system_name());
        s.put(
            ui::BODY_ROW,
            ui::BODY_COL,
            "Type choices, press Enter.",
            console::Attr::GREEN,
        );
        for (i, (label, value)) in LABELS.iter().zip(values).enumerate() {
            let row = ui::BODY_ROW + 2 + i * 3;
            let (first, second) = split_label(label);
            s.put(
                row,
                ui::BODY_COL + 2,
                &format!("{}. {first}", i + 1),
                console::Attr::GREEN,
            );
            if !second.is_empty() {
                s.put(row + 1, ui::BODY_COL + 5, second, console::Attr::GREEN);
            }
            s.put(
                row + 1,
                ui::BODY_COL + 62,
                "Y=Yes, N=No",
                console::Attr::GREEN,
            );
            s.field(row + 1, ui::BODY_COL + 75, 1, false);
            s.put(row + 1, ui::BODY_COL + 75, yn(value), console::Attr::INPUT);
        }
        s.put(
            ui::BODY_ROW + 15,
            ui::BODY_COL,
            "Only options 1 and 3 have an effect in this release; the others are kept.",
            console::Attr::GREEN,
        );
        ui::keys(&mut s, "F3=Exit   F12=Cancel");
        s.cursor = (ui::BODY_ROW + 2, ui::BODY_COL + 74);
        let _ = FormLine::new("", 1, "");
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let mut bits = [false; 4];
        for (i, b) in bits.iter_mut().enumerate() {
            match fields
                .get(i)
                .map(|f| f.trim().to_ascii_uppercase())
                .as_deref()
            {
                Some("Y") => *b = true,
                Some("N" | "") | None => *b = false,
                _ => return Flow::Error("Type Y or N".into()),
            }
        }
        let flags = SecurityFlags {
            allow_default_password_change: bits[0],
            allow_new_certificates: bits[1],
            allow_sysval_security_changes: bits[2],
            allow_add_certificates: bits[3],
        };
        match env.set_security_flags(flags) {
            Ok(()) => Flow::Unwind(1, Some(("System security options changed".into(), false))),
            Err(e) => Flow::Error(e),
        }
    }
}

/// Break a long label at a word boundary so it fits two lines.
fn split_label(label: &str) -> (&str, &str) {
    if label.len() <= 60 {
        return (label, "");
    }
    let cut = label[..60].rfind(' ').unwrap_or(60);
    (&label[..cut], label[cut..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_round_trip() {
        let f = SecurityFlags {
            allow_default_password_change: true,
            allow_new_certificates: false,
            allow_sysval_security_changes: true,
            allow_add_certificates: false,
        };
        assert_eq!(encode(f), 0b0101);
        assert_eq!(decode(0b0101), f);
        assert_eq!(
            split_label(LABELS[0]).1,
            "password to change its own password"
        );
    }
}

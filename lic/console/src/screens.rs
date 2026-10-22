// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 1 screens. Positions are 1-based row and column. Every
//! string here is Quartz's own.

use crate::screen::{Attr, Screen};

/// Bottom line of the SRC and sign-on displays, white, at the right.
pub const COPYRIGHT: &str = "(C) COPYRIGHT QUARTZ SYSTEMS 2026.";
/// Column of the copyright line: it ends at column 76.
pub const COPYRIGHT_COL: usize = 43;
/// Row of the message line.
pub const MESSAGE_ROW: usize = 23;

/// The SRC display shown during IPL: the current code double width, the
/// last eight codes below it.
pub fn src_screen(system: &str, current: &str, text: &str, recent: &[(String, String)]) -> Screen {
    let mut s = Screen::new();
    s.put(1, 1, &release::banner(), Attr::WHITE);
    s.put(1, 70, system, Attr::GREEN);
    s.put(3, 1, "System reference code", Attr::GREEN);
    s.put_double(5, 24, current, Attr::WHITE);
    s.put(7, 24, text, Attr::GREEN);
    s.put(10, 1, "Recent codes", Attr::GREEN);
    let start = recent.len().saturating_sub(8);
    for (i, (code, t)) in recent[start..].iter().enumerate() {
        s.put(11 + i, 3, &format!("{code}  {t}"), Attr::GREEN);
    }
    s.put(24, COPYRIGHT_COL, COPYRIGHT, Attr::WHITE);
    s
}

/// The sign-on display. Geometry follows the classic layout; the words are
/// Quartz's.
pub fn sign_on(system: &str, subsystem: &str, display: &str, long_password: bool) -> Screen {
    let mut s = Screen::new();
    s.put(1, 36, "Sign On", Attr::WHITE);
    s.put(2, 48, "System  . . . . . :", Attr::GREEN);
    s.put(2, 68, system, Attr::GREEN);
    s.put(3, 48, "Subsystem . . . . :", Attr::GREEN);
    s.put(3, 68, subsystem, Attr::GREEN);
    s.put(4, 48, "Display . . . . . :", Attr::GREEN);
    s.put(4, 68, display, Attr::GREEN);
    s.put(6, 17, "User  . . . . . . . . . . . . . .", Attr::GREEN);
    s.field(6, 53, 10, false);
    s.put(7, 17, "Password  . . . . . . . . . . . .", Attr::GREEN);
    if long_password {
        // QDSIGNON2: 128 characters, nothing shown, no underline.
        s.field_secret(7, 53, 28, 128);
    } else {
        s.field(7, 53, 10, true);
    }
    s.put(8, 17, "Program/procedure . . . . . . . .", Attr::GREEN);
    s.field(8, 53, 10, false);
    s.put(9, 17, "Menu  . . . . . . . . . . . . . .", Attr::GREEN);
    s.field(9, 53, 10, false);
    s.put(10, 17, "Current library . . . . . . . . .", Attr::GREEN);
    s.field(10, 53, 10, false);
    s.put(24, COPYRIGHT_COL, COPYRIGHT, Attr::WHITE);
    s.cursor = (5, 52);
    s
}

/// Replace the message line. Red for errors.
pub fn set_message(s: &mut Screen, text: &str, error: bool) {
    s.put(
        MESSAGE_ROW,
        1,
        &" ".repeat(crate::screen::COLS),
        Attr::GREEN,
    );
    s.put(
        MESSAGE_ROW,
        1,
        text,
        if error { Attr::RED } else { Attr::GREEN },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_on_layout_matches_the_specification() {
        let s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        let d = s.dump();
        let lines: Vec<&str> = d.lines().collect();
        let at = |row: usize, col: usize, text: &str| {
            let line = lines[row - 1];
            assert_eq!(
                &line[col - 1..col - 1 + text.len()],
                text,
                "row {row} col {col}"
            );
        };
        at(1, 36, "Sign On");
        at(2, 48, "System  . . . . . :");
        at(2, 68, "QS400");
        at(3, 68, "QINTER");
        at(4, 68, "QSCONSOLE");
        at(6, 17, "User  . . . . . . . . . . . . . .");
        at(10, 17, "Current library . . . . . . . . .");
        at(24, 43, COPYRIGHT);
        assert_eq!(s.fields.len(), 5);
        assert!(s.fields[1].nondisplay);
        assert!(s.fields.iter().all(|f| f.col == 53 && f.len == 10));
    }

    #[test]
    fn src_screen_shows_the_last_eight() {
        let recent: Vec<(String, String)> = (0..10)
            .map(|i| (format!("C100{i:04}"), format!("step {i}")))
            .collect();
        let s = src_screen("QS400", "C6001090", "IPL complete", &recent);
        let d = s.dump();
        assert!(d.contains("C1000002  step 2"));
        assert!(!d.contains("C1000001  step 1"));
        assert_eq!(s.cell(4, 23).ch, 'C');
        assert!(s.cell(4, 23).attr.double_width);
    }
}

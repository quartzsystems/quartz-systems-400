// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statement-level displays of `ADDBKP`, `RMVBKP` and `DSPBKP`:
//! Display Breakpoints (the list `DSPBKP` shows) and Display Breakpoint
//! (a stop at a breakpoint `ADDBKP` set). pub400 has no program of this
//! kind for the account (`DSPBKP` answers "no default program"), so the
//! layout follows IBM's data displays: labels with leaders to column
//! 40, values at 43.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title};

fn label(s: &mut Screen, row: usize, col: usize, text: &str, value: &str) {
    put(s, row, col, Attr::Normal, &leader(text, col, 40));
    put(s, row, 41, Attr::Normal, ":");
    put(s, row, 43, Attr::Normal, value);
}

/// Display Breakpoints: every statement breakpoint with its program.
#[must_use]
pub fn dspbkp(rows: &[(String, String, u32)], first: usize, msg: Option<&str>) -> Screen {
    const ROOM: usize = 14;
    let mut s = blank();
    title(&mut s, "Display Breakpoints");
    put(&mut s, 3, 2, Attr::HighIntensity, "Statement/Instruction");
    put(&mut s, 3, 30, Attr::HighIntensity, "Program");
    put(&mut s, 3, 43, Attr::HighIntensity, "Library");
    for (i, (pgm, lib, stmt)) in rows.iter().enumerate().skip(first).take(ROOM) {
        let row = 4 + i - first;
        put(&mut s, row, 4, Attr::Normal, &stmt.to_string());
        put(&mut s, row, 30, Attr::Normal, pgm);
        put(&mut s, row, 43, Attr::Normal, lib);
    }
    if rows.is_empty() {
        put(&mut s, 4, 4, Attr::Normal, "(No breakpoints)");
    }
    more(&mut s, 19, first + ROOM >= rows.len());
    put(&mut s, 21, 2, Attr::Blue, "Press Enter to continue.");
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s
}

/// Display Breakpoint: where the program stopped.
#[must_use]
pub fn display_breakpoint(
    stmt: u32,
    program: &str,
    library: &str,
    procedure: &str,
    depth: u32,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, "Display Breakpoint");
    label(&mut s, 3, 2, "Statement/Instruction", &stmt.to_string());
    label(&mut s, 4, 2, "Program", program);
    label(&mut s, 5, 4, "Library", library);
    label(&mut s, 6, 2, "Procedure", procedure);
    label(&mut s, 7, 2, "Recursion level", &depth.to_string());
    put(&mut s, 21, 2, Attr::Blue, "Press Enter to continue.");
    keys(
        &mut s,
        &[
            (23, 2, "F3=Exit program"),
            (23, 20, "F10=Command entry"),
            (23, 40, "F12=Resume"),
        ],
    );
    message(&mut s, msg);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opm_displays() {
        let t = dspbkp(&[("P".into(), "QGPL".into(), 7)], 0, None).text_rows();
        assert!(t[0].contains("Display Breakpoints"));
        assert!(t[3].contains("QGPL"));
        let t = display_breakpoint(7, "P", "QGPL", "P", 1, None).text_rows();
        assert!(t[2].contains("Statement/Instruction"));
        assert_eq!(&t[2][40..41], ":");
    }
}

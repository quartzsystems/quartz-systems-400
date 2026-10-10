// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Screen builders with the geometry of the attended IPL menu: title
//! centered on row 1, `System:` right-aligned on row 2, body from row
//! 4, a selection field on row 21 under its label on row 20, function
//! keys on row 23, the message line on row 24. All text is Quartz's.

use console::{Attr, COLS, Screen};

/// Row of the `Selection` label.
pub const SELECTION_ROW: usize = 20;
/// Row of the function key line.
pub const KEYS_ROW: usize = 23;
/// Row of the message line.
pub const MESSAGE_ROW: usize = 24;
/// Column the body starts at.
pub const BODY_COL: usize = 2;
/// First row of the body.
pub const BODY_ROW: usize = 4;

/// Title and system name.
pub fn header(s: &mut Screen, title: &str, system: &str) {
    let col = COLS.saturating_sub(title.len()) / 2 + 1;
    s.put(1, col, title, Attr::WHITE);
    let label = format!("System:  {system}");
    s.put(2, COLS + 1 - label.len() - 1, &label, Attr::GREEN);
}

/// The function key line.
pub fn keys(s: &mut Screen, keys: &str) {
    s.put(KEYS_ROW, BODY_COL, keys, Attr::GREEN);
}

/// Replace the message line. Red for errors.
pub fn message(s: &mut Screen, text: &str, error: bool) {
    s.put(MESSAGE_ROW, 1, &" ".repeat(COLS), Attr::GREEN);
    s.put(
        MESSAGE_ROW,
        1,
        text,
        if error { Attr::RED } else { Attr::GREEN },
    );
}

/// A numbered menu: the options from row 4, `Selection` and its field
/// at the bottom.
pub fn menu(title: &str, system: &str, options: &[(u8, &str)], fkeys: &str) -> Screen {
    let mut s = Screen::new();
    header(&mut s, title, system);
    s.put(
        BODY_ROW,
        BODY_COL,
        "Select one of the following:",
        Attr::GREEN,
    );
    for (i, (number, text)) in options.iter().enumerate() {
        s.put(
            BODY_ROW + 2 + i,
            BODY_COL + 4,
            &format!("{number:>2}. {text}"),
            Attr::GREEN,
        );
    }
    s.put(SELECTION_ROW, BODY_COL, "Selection", Attr::GREEN);
    s.field(SELECTION_ROW + 1, BODY_COL + 2, 2, false);
    keys(&mut s, fkeys);
    s.cursor = (SELECTION_ROW, BODY_COL + 1);
    s
}

/// A list with an option column: column headings on row 4, one input
/// field of one cell per row from row 5, text lines after the list.
pub fn list(
    title: &str,
    system: &str,
    prompt: &str,
    headings: &str,
    rows: &[String],
    fkeys: &str,
) -> Screen {
    let mut s = Screen::new();
    header(&mut s, title, system);
    s.put(BODY_ROW, BODY_COL, prompt, Attr::GREEN);
    s.put(BODY_ROW + 2, BODY_COL, headings, Attr::GREEN);
    for (i, row) in rows.iter().take(KEYS_ROW - BODY_ROW - 4).enumerate() {
        let r = BODY_ROW + 3 + i;
        s.field(r, BODY_COL + 1, 1, false);
        s.put(r, BODY_COL + 4, row, Attr::GREEN);
    }
    keys(&mut s, fkeys);
    s.cursor = (BODY_ROW + 2, BODY_COL);
    s
}

/// A form: label and input field per line from row 4, labels padded to
/// one column.
pub fn form(title: &str, system: &str, lines: &[FormLine], fkeys: &str) -> Screen {
    let mut s = Screen::new();
    header(&mut s, title, system);
    let width = lines
        .iter()
        .map(|l| l.label.len())
        .max()
        .unwrap_or(0)
        .max(20);
    let labels: Vec<String> = lines.iter().map(|l| dotted(l.label, width)).collect();
    let col = BODY_COL + labels.iter().map(String::len).max().unwrap_or(0) + 1;
    for (i, (line, label)) in lines.iter().zip(&labels).enumerate() {
        let r = BODY_ROW + i;
        s.put(r, BODY_COL, label, Attr::GREEN);
        s.field(r, col, line.len, line.hidden);
        if !line.value.is_empty() {
            s.put(r, col, &line.value, Attr::INPUT);
        }
    }
    keys(&mut s, fkeys);
    s.cursor = (BODY_ROW - 1, col - 1);
    s
}

/// One line of a form.
#[derive(Debug, Clone)]
pub struct FormLine {
    /// Label at the left.
    pub label: &'static str,
    /// Field length.
    pub len: usize,
    /// Initial contents.
    pub value: String,
    /// Typed text shows nothing.
    pub hidden: bool,
}

impl FormLine {
    /// A visible field.
    #[must_use]
    pub fn new(label: &'static str, len: usize, value: impl Into<String>) -> Self {
        Self {
            label,
            len,
            value: value.into(),
            hidden: false,
        }
    }

    /// A password field.
    #[must_use]
    pub fn hidden(label: &'static str, len: usize) -> Self {
        Self {
            label,
            len,
            value: String::new(),
            hidden: true,
        }
    }
}

/// A label padded with ` . ` to `width` and ended with `:`.
#[must_use]
pub fn dotted(label: &str, width: usize) -> String {
    let mut out = String::from(label);
    out.push(' ');
    while out.len() < width {
        out.push_str(". ");
    }
    out.push(':');
    out
}

/// Text lines from row 4, optionally a percent bar, and no field: a
/// notice or a status display.
pub fn text(
    title: &str,
    system: &str,
    lines: &[String],
    percent: Option<u8>,
    fkeys: &str,
) -> Screen {
    let mut s = Screen::new();
    header(&mut s, title, system);
    let mut row = BODY_ROW;
    for line in lines.iter().take(KEYS_ROW - BODY_ROW - 3) {
        s.put(row, BODY_COL, line, Attr::GREEN);
        row += 1;
    }
    if let Some(p) = percent {
        row += 1;
        let filled = usize::from(p.min(100)) * 50 / 100;
        let bar = format!(
            "{:>3}%  {}{}",
            p.min(100),
            "#".repeat(filled),
            "-".repeat(50 - filled)
        );
        s.put(row, BODY_COL, &bar, Attr::WHITE);
    }
    keys(&mut s, fkeys);
    s
}

/// A confirmation: text lines, then `F10=Continue   F12=Cancel`.
pub fn confirm(title: &str, system: &str, lines: &[String]) -> Screen {
    text(title, system, lines, None, "F10=Continue   F12=Cancel")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_geometry_matches_the_attended_ipl_display() {
        let s = menu(
            "IPL or Install the System",
            "QS400",
            &[(1, "Perform an IPL"), (2, "Install the operating system")],
            "F3=Exit   F12=Cancel",
        );
        let d = s.dump();
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines[0].trim(), "IPL or Install the System");
        assert!(lines[1].ends_with("System:  QS400 "));
        assert!(lines[3].starts_with(" Select one of the following:"));
        assert!(lines[5].starts_with("      1. Perform an IPL"));
        assert!(lines[19].starts_with(" Selection"));
        assert_eq!(
            (s.fields[0].row, s.fields[0].col, s.fields[0].len),
            (21, 4, 2)
        );
        assert!(lines[22].starts_with(" F3=Exit"));
    }

    #[test]
    fn dotted_labels_line_up() {
        assert_eq!(dotted("User", 10), "User . . . :");
        assert_eq!(dotted("Password", 10), "Password . :");
        assert_eq!(dotted("A long label", 10), "A long label :");
    }

    #[test]
    fn list_and_form_place_fields() {
        let l = list(
            "Select Load Source Device",
            "QS400",
            "Type 1 to select, press Enter.",
            "Opt  Unit",
            &["DD001".into(), "DD002".into()],
            "F3=Exit",
        );
        assert_eq!(l.fields.len(), 2);
        assert_eq!((l.fields[1].row, l.fields[1].col), (8, 3));
        let f = form(
            "Sign On",
            "QS400",
            &[
                FormLine::new("User", 10, ""),
                FormLine::hidden("Password", 10),
            ],
            "F3=Exit",
        );
        assert_eq!(f.fields.len(), 2);
        assert!(f.fields[1].nondisplay);
        let t = text("Status", "QS400", &["Copying".into()], Some(40), "");
        assert!(
            t.dump()
                .lines()
                .nth(5)
                .unwrap()
                .contains("40%  ####################------------------------------")
        );
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debugger's displays, laid out as IBM's display files place them:
//! Display Module Source ([`source`], measured on the pub400 goldens),
//! the lists it opens ([`lists`]: module breakpoints, the module list,
//! watches, threads), Select View and Evaluate Expression ([`eval`]),
//! the OPM breakpoint displays ([`opm`]) and help ([`help`]). Each
//! builds a `uim::Screen`; input fields name what they hold with their
//! `Target`.

pub mod eval;
pub mod help;
pub mod lists;
pub mod opm;
pub mod source;

pub use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target, Text};

/// Rows of the displays.
pub const ROWS: usize = 24;
/// Columns.
pub const COLS: usize = 80;

/// A blank 24 by 80 screen.
#[must_use]
pub fn blank() -> Screen {
    Screen::new(ROWS, COLS)
}

/// Text at a position, up to column 80.
pub fn put(s: &mut Screen, row: usize, col: usize, attr: Attr, text: &str) {
    if text.trim().is_empty() || col == 0 || row == 0 || row > s.rows {
        return;
    }
    let room = s.cols + 1 - col;
    s.texts.push(Text {
        row,
        col,
        attr,
        text: text.chars().take(room).collect(),
    });
}

/// An input field.
pub fn input(s: &mut Screen, row: usize, col: usize, len: usize, target: Target, value: &str) {
    s.field(Field {
        row,
        col,
        len,
        attr: Attr::Underline,
        value: value.chars().take(len).collect(),
        target,
        usage: Usage::Inout,
        shift: Shift::None,
        required: false,
        autoskip: false,
    });
}

/// Function keys as `(row, column, text)`.
pub fn keys(s: &mut Screen, keys: &[(usize, usize, &str)]) {
    for (r, c, t) in keys {
        put(s, *r, *c, Attr::Blue, t);
    }
}

/// The title, centered as IBM centers it (in the 80 columns, the odd
/// column to the right).
pub fn title(s: &mut Screen, text: &str) {
    let col = 1 + (COLS - text.len()) / 2;
    put(s, 1, col, Attr::HighIntensity, text);
}

/// `System:` and its name on row 2.
pub fn system(s: &mut Screen, name: &str) {
    put(s, 2, 62, Attr::Normal, "System:");
    put(s, 2, 72, Attr::Normal, name);
}

/// The message line.
pub fn message(s: &mut Screen, message: Option<&str>) {
    if let Some(m) = message {
        put(s, ROWS, 2, Attr::HighIntensity, m);
    }
}

/// `More...` or `Bottom` on a row.
pub fn more(s: &mut Screen, row: usize, bottom: bool) {
    if bottom {
        put(s, row, 74, Attr::HighIntensity, "Bottom");
    } else {
        put(s, row, 73, Attr::HighIntensity, "More...");
    }
    s.more = Some(uim::render::More {
        row,
        col: if bottom { 74 } else { 73 },
        bottom,
    });
}

/// A list display's command line: `Command` on `row`, the `===>` line
/// under it.
pub fn cmdline(s: &mut Screen, row: usize, value: &str) {
    put(s, row, 2, Attr::Normal, "Command");
    put(s, row + 1, 2, Attr::Normal, "===>");
    input(s, row + 1, 7, 73, Target::CmdLine, value);
}

/// `label` followed by dot leaders ending at `end`, IBM's way: a blank,
/// then dots on every other column.
#[must_use]
pub fn leader(label: &str, col: usize, end: usize) -> String {
    let mut out = label.to_string();
    let mut c = col + label.chars().count();
    let mut first = c + 1;
    if first % 2 != end % 2 {
        first += 1;
    }
    while c < first {
        out.push(' ');
        c += 1;
    }
    while c <= end {
        out.push(if (c % 2) == (end % 2) { '.' } else { ' ' });
        c += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_center() {
        let mut s = blank();
        title(&mut s, "Display Module Source");
        assert_eq!(s.texts[0].col, 30);
    }

    #[test]
    fn leaders() {
        assert_eq!(leader("Program", 2, 17), "Program  . . . .");
    }
}

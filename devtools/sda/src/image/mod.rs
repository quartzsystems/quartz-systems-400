// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The design image of a record: constants as their text, fields as
//! the characters SDA draws for them (`I`, `O`, `B` for character
//! fields, `3`, `6`, `9` for numeric ones, through the edit word when
//! there is one), the ruler (F14) and what the operator types on it
//! ([`place`]).

pub mod place;

use crate::source::{Element, Kind, Record, Usage};

/// Rows of the design image.
pub const ROWS: usize = 24;
/// Columns.
pub const COLS: usize = 80;

/// What the image shows besides the record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The ruler at a cursor position (F14).
    pub ruler: Option<(usize, usize)>,
    /// Show hidden, message and program-to-system fields on row 24's
    /// left (F20 lists them by name on the message line).
    pub hidden: bool,
}

/// The characters an element shows on the image.
#[must_use]
pub fn element_text(e: &Element) -> String {
    match &e.kind {
        Kind::Constant { text, .. } => text.clone(),
        Kind::Field {
            usage, dec, ty, ..
        } => {
            let numeric = dec.is_some() || matches!(ty, 'S' | 'Y' | 'P' | 'B' | 'F');
            let ch = match (usage, numeric) {
                (Usage::Input, false) => 'I',
                (Usage::Both, false) => 'B',
                (_, false) => 'O',
                (Usage::Input, true) => '3',
                (Usage::Both, true) => '9',
                (_, true) => '6',
            };
            let width = e.width as usize;
            if let Some(w) = e
                .keywords
                .iter()
                .find(|k| k.name == "EDTWRD")
                .and_then(|k| k.quoted(0))
            {
                // Digit places of the word show the field's character.
                return w
                    .chars()
                    .map(|c| if c == ' ' || c == '0' { ch } else { c })
                    .collect();
            }
            std::iter::repeat_n(ch, width).collect()
        }
    }
}

/// The image of `record` with the records shown with it underneath
/// (F9), as 24 rows of 80 characters.
#[must_use]
pub fn render(record: &Record, with: &[&Record], view: &View) -> Vec<String> {
    let mut grid = vec![vec![' '; COLS]; ROWS];
    for r in with.iter().copied().chain(std::iter::once(record)) {
        for e in r.elements.iter().filter(|e| e.displayed()) {
            let Some((row, col)) = e.pos else { continue };
            put(&mut grid, row as usize, col as usize, &element_text(e));
        }
    }
    if let Some((row, col)) = view.ruler {
        ruler(&mut grid, row, col);
    }
    grid.into_iter().map(|r| r.into_iter().collect()).collect()
}

/// Text at a 1-based position, wrapping onto the next row.
fn put(grid: &mut [Vec<char>], row: usize, col: usize, text: &str) {
    if row == 0 || col == 0 {
        return;
    }
    for (at, ch) in ((row - 1) * COLS + (col - 1)..).zip(text.chars()) {
        let (r, c) = (at / COLS, at % COLS);
        if r >= ROWS {
            return;
        }
        grid[r][c] = ch;
    }
}

/// The ruler character of a column: the tens digit on each tenth,
/// dots on the others but the units 1, 5 and 9.
fn ruler_char(col: usize) -> char {
    match col % 10 {
        0 => char::from_digit(u32::try_from((col / 10) % 10).unwrap_or(0), 10).unwrap_or('0'),
        1 | 5 | 9 => ' ',
        _ => '.',
    }
}

/// The ruler: a scale along the cursor's row and the row numbers down
/// its column, each drawn only on blank places that do not touch what
/// the image shows.
fn ruler(grid: &mut [Vec<char>], row: usize, col: usize) {
    let free = |g: &[Vec<char>], r: usize, c: usize| -> bool {
        let at = |c: usize| g[r][c] == ' ';
        at(c) && (c == 0 || at(c - 1)) && (c + 1 >= COLS || at(c + 1))
    };
    if (1..=ROWS).contains(&row) {
        let r = row - 1;
        let shown: Vec<bool> = (0..COLS).map(|c| free(grid, r, c)).collect();
        for c in 0..COLS {
            if shown[c] {
                grid[r][c] = ruler_char(c + 1);
            }
        }
    }
    if (1..=COLS).contains(&col) {
        for r in 0..ROWS {
            if r + 1 == row {
                continue;
            }
            let label = (r + 1).to_string();
            // Right aligned on the column after the cursor's.
            let end = col.min(COLS - 1);
            let start = end + 1 - label.len();
            if (start..=end).all(|c| free(grid, r, c)) {
                for (i, ch) in label.chars().enumerate() {
                    grid[r][start + i] = ch;
                }
            }
        }
    }
}

/// The names of the fields the image does not show (hidden, message,
/// program-to-system), for F20.
#[must_use]
pub fn hidden_names(record: &Record) -> Vec<String> {
    record
        .elements
        .iter()
        .filter(|e| !e.displayed())
        .filter_map(|e| e.name().map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{RecordType, Element};

    fn record() -> Record {
        let mut r = Record::new("SCREEN1", RecordType::Record);
        r.elements.push(Element::new(
            Kind::Constant {
                text: "Customer".into(),
                keyword: None,
            },
            Some((3, 2)),
        ));
        r.elements
            .push(Element::field("FLD001", Usage::Input, false, 10, 0, Some((3, 21))));
        r.elements.push(Element::new(
            Kind::Constant {
                text: "Amount".into(),
                keyword: None,
            },
            Some((5, 2)),
        ));
        r.elements.push(Element::new(
            Kind::Constant {
                text: "due".into(),
                keyword: None,
            },
            Some((5, 9)),
        ));
        r.elements
            .push(Element::field("FLD002", Usage::Output, true, 7, 2, Some((5, 21))));
        r
    }

    #[test]
    fn image() {
        let rows = render(&record(), &[], &View::default());
        assert_eq!(rows[2].trim_end(), format!(" Customer{}IIIIIIIIII", " ".repeat(11)));
        assert_eq!(rows[4].trim_end(), format!(" Amount due{}66666.66", " ".repeat(9)));
    }

    #[test]
    fn ruler_as_sda_draws_it() {
        let view = View {
            ruler: Some((5, 20)),
            hidden: false,
        };
        let rows = render(&record(), &[], &view);
        // The scale skips places next to the image's text.
        assert!(rows[4].starts_with(" Amount due .. ...  66666.66 3 ... ... 4"));
        assert!(rows[4].ends_with("7 ... ... 8"));
        assert_eq!(&rows[0][20..21], "1");
        assert_eq!(&rows[9][19..21], "10");
        // Row 3 holds the field where the numbers would go.
        assert_eq!(&rows[2][19..21], " I");
    }
}

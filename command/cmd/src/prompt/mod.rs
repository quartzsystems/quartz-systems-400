// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prompter: the display every command gets from its definition
//! (`F4`), with IBM's geometry (`CMD.md` §6), and the session that
//! turns keys and typed fields into a command string the analyzer
//! accepts or a cancel. The renderer produces a [`Screen`] the job
//! shows through its display; it knows nothing of the display itself.

pub mod layout;
pub mod session;

use std::fmt::Write as _;

pub use layout::{Line, Lines, lines};
pub use session::{Prompter, Step, Window};

/// Rows on the display.
pub const ROWS: usize = 24;
/// Columns on the display.
pub const COLS: usize = 80;

/// Where a field's value belongs: the parameter, then the item path
/// inside it (an element's index, a list entry's index, a qualifier's
/// index, outermost first).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Path {
    /// Parameter index in the definition.
    pub parm: usize,
    /// Steps into the value.
    pub steps: Vec<usize>,
}

impl Path {
    /// A parameter's own path.
    #[must_use]
    pub fn parm(parm: usize) -> Self {
        Self {
            parm,
            steps: Vec::new(),
        }
    }

    /// One step deeper.
    #[must_use]
    pub fn step(&self, i: usize) -> Self {
        let mut steps = self.steps.clone();
        steps.push(i);
        Self {
            parm: self.parm,
            steps,
        }
    }
}

/// An input field on the prompt display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Row, 1-based.
    pub row: u8,
    /// Column, 1-based.
    pub col: u8,
    /// Length.
    pub len: u16,
    /// Its value.
    pub value: String,
    /// What it belongs to; `None` for the `+ for more values` field.
    pub path: Option<Path>,
    /// Shown in reverse image (the field in error).
    pub reverse: bool,
}

/// A rendered display: 24 rows of 80, the fields, the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// The text, each row padded to 80.
    pub rows: Vec<String>,
    /// The fields.
    pub fields: Vec<Field>,
    /// Where the cursor goes, 1-based.
    pub cursor: Option<(u8, u8)>,
    /// The message line (row 24), when there is one.
    pub message: String,
}

impl Default for Screen {
    fn default() -> Self {
        Self {
            rows: vec![" ".repeat(COLS); ROWS],
            fields: Vec::new(),
            cursor: None,
            message: String::new(),
        }
    }
}

impl Screen {
    /// Put text at a row and column (1-based), clipped at the right.
    pub fn put(&mut self, row: usize, col: usize, text: &str) {
        if row == 0 || row > ROWS || col == 0 || col > COLS {
            return;
        }
        let mut chars: Vec<char> = self.rows[row - 1].chars().collect();
        for (i, ch) in text.chars().enumerate() {
            let c = col - 1 + i;
            if c >= COLS {
                break;
            }
            chars[c] = ch;
        }
        self.rows[row - 1] = chars.into_iter().collect();
    }

    /// Put text centered on a row.
    pub fn center(&mut self, row: usize, text: &str) {
        let n = text.chars().count().min(COLS);
        let col = (COLS - n) / 2 + 1;
        self.put(row, col, text);
    }

    /// Put text ending at column 79.
    pub fn right(&mut self, row: usize, text: &str) {
        let n = text.chars().count();
        let col = COLS.saturating_sub(n);
        self.put(row, col.max(1), text);
    }

    /// Add a field; its value is drawn into the row.
    pub fn field(&mut self, row: usize, col: usize, len: usize, value: &str, path: Option<Path>) {
        let shown: String = value.chars().take(len).collect();
        self.put(row, col, &shown);
        self.fields.push(Field {
            row: u8::try_from(row).unwrap_or(1),
            col: u8::try_from(col).unwrap_or(1),
            len: u16::try_from(len).unwrap_or(1),
            value: value.to_string(),
            path,
            reverse: false,
        });
    }

    /// The display as text, for goldens and tests.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        for (i, r) in self.rows.iter().enumerate() {
            if i == ROWS - 1 && !self.message.is_empty() {
                let mut m = self.message.clone();
                m.truncate(COLS);
                let _ = write!(s, "{m:<80}");
            } else {
                s.push_str(r);
            }
            s.push('\n');
        }
        s
    }
}

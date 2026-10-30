// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rendered screen: texts and fields at 1-based positions, the
//! cursor and the paging indicator. Column 1 and the last column are
//! never written to; the data stream crate puts attribute bytes there.

use crate::model::{Shift, Usage};

/// A display attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attr {
    /// Green.
    Normal,
    /// White: titles, headings, `More...`, the message line.
    HighIntensity,
    /// Blue: instructions, option legends, function keys.
    Blue,
    /// Reverse image: an entry in error.
    Reverse,
    /// Underlined: an input field.
    Underline,
    /// Not shown.
    Nondisplay,
}

/// Text at a position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// Row, from 1.
    pub row: usize,
    /// Column, from 1.
    pub col: usize,
    /// How it shows.
    pub attr: Attr,
    /// The characters.
    pub text: String,
}

/// What an input field belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A dialog variable.
    Var(String),
    /// The option column of a list entry.
    ListOption {
        /// The list.
        list: String,
        /// The entry's index in the whole list.
        row_index: usize,
    },
    /// An input column of a list entry.
    ListCell {
        /// The list.
        list: String,
        /// The entry's index in the whole list.
        row_index: usize,
        /// The column's variable.
        var: String,
    },
    /// The command line.
    CmdLine,
    /// A menu's selection field (menus without a command line).
    MenuSelection,
    /// A list's position-to field.
    PositionTo(String),
}

/// An input field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Row, from 1.
    pub row: usize,
    /// Column, from 1.
    pub col: usize,
    /// Length; a field longer than its row goes on at column 1 of the next.
    pub len: usize,
    /// How it shows.
    pub attr: Attr,
    /// The current value.
    pub value: String,
    /// What it belongs to.
    pub target: Target,
    /// `USAGE`.
    pub usage: Usage,
    /// `SHIFT`.
    pub shift: Shift,
    /// `REQUIRED`.
    pub required: bool,
    /// `AUTOSKIP`.
    pub autoskip: bool,
}

/// The paging indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct More {
    /// Row.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// `Bottom` rather than `More...`.
    pub bottom: bool,
}

/// A stretch of a row and the help module for it (F1 there), or the
/// help module a link shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpArea {
    /// Row.
    pub row: usize,
    /// First column.
    pub col: usize,
    /// Last column.
    pub end: usize,
    /// The help module.
    pub module: String,
}

/// A rendered screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Screen {
    /// Rows: 24 or 27.
    pub rows: usize,
    /// Columns: 80 or 132.
    pub cols: usize,
    /// Texts in placement order.
    pub texts: Vec<Text>,
    /// Input fields in placement order.
    pub fields: Vec<Field>,
    /// Where the cursor starts.
    pub cursor: Option<(usize, usize)>,
    /// The paging indicator.
    pub more: Option<More>,
    /// How many entries of each list the page shows, by list name.
    pub list_shown: std::collections::BTreeMap<String, usize>,
    /// Help modules by region, later regions over earlier ones.
    pub helps: Vec<HelpArea>,
    /// The links of a help screen (`:LINK.`).
    pub links: Vec<HelpArea>,
}

impl Screen {
    /// Blank.
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            ..Self::default()
        }
    }

    /// The last column text may use.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.cols - 1
    }

    /// Text at a position, cut at the last usable column; nothing
    /// outside the screen.
    pub fn put(&mut self, row: usize, col: usize, attr: Attr, text: &str) {
        if row == 0 || row > self.rows || col == 0 || col > self.limit() || text.is_empty() {
            return;
        }
        let room = self.limit() + 1 - col;
        // A non-breaking blank (it keeps words together, R13) shows as
        // a blank.
        let text: String = text
            .chars()
            .take(room)
            .map(|c| if c == '\u{a0}' { ' ' } else { c })
            .collect();
        if text.trim().is_empty() && attr != Attr::Underline {
            return;
        }
        self.texts.push(Text {
            row,
            col,
            attr,
            text,
        });
    }

    /// Text right-aligned so that it ends at `end`.
    pub fn put_right(&mut self, row: usize, end: usize, attr: Attr, text: &str) {
        let n = text.chars().count();
        if n <= end {
            self.put(row, end + 1 - n, attr, text);
        }
    }

    /// A help region, when there is a module for it.
    pub fn help(&mut self, row: usize, col: usize, end: usize, module: Option<&str>) {
        if let Some(m) = module
            && row >= 1
            && row <= self.rows
            && col <= end
        {
            self.helps.push(HelpArea {
                row,
                col,
                end,
                module: m.to_string(),
            });
        }
    }

    /// A field.
    pub fn field(&mut self, f: Field) {
        if f.row == 0 || f.row > self.rows || f.col == 0 || f.col > self.limit() || f.len == 0 {
            return;
        }
        self.fields.push(f);
    }

    /// The character grid: one string of `cols` characters per row,
    /// fields shown as their values padded to their length.
    #[must_use]
    pub fn text_rows(&self) -> Vec<String> {
        let mut grid = vec![vec![' '; self.cols]; self.rows];
        for t in &self.texts {
            if t.attr == Attr::Nondisplay {
                continue;
            }
            for (i, ch) in t.text.chars().enumerate() {
                let c = t.col - 1 + i;
                if c < self.cols {
                    grid[t.row - 1][c] = ch;
                }
            }
        }
        for f in &self.fields {
            let shown: Vec<char> = if f.attr == Attr::Nondisplay {
                Vec::new()
            } else {
                f.value.chars().collect()
            };
            let mut row = f.row - 1;
            let mut col = f.col - 1;
            for i in 0..f.len {
                if col >= self.limit() {
                    row += 1;
                    col = 0;
                }
                if row >= self.rows {
                    break;
                }
                grid[row][col] = shown.get(i).copied().unwrap_or(' ');
                col += 1;
            }
        }
        grid.into_iter().map(String::from_iter).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_clips_and_wraps_fields() {
        let mut s = Screen::new(3, 10);
        s.put(1, 8, Attr::Normal, "abcdef");
        s.put(4, 1, Attr::Normal, "no");
        s.field(Field {
            row: 2,
            col: 8,
            len: 5,
            attr: Attr::Underline,
            value: "xyz".into(),
            target: Target::CmdLine,
            usage: Usage::Input,
            shift: Shift::None,
            required: false,
            autoskip: false,
        });
        let rows = s.text_rows();
        assert_eq!(rows[0], "       ab ");
        assert_eq!(rows[1], "       xy ");
        assert_eq!(rows[2], "z         ");
        assert_eq!(s.texts.len(), 1);
    }
}

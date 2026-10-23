// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The terminal as text: the character grid the goldens hold, and the
//! attribute grid beside it.

use super::Terminal;
use crate::ds::{DUP, FIELD_MARK, NULL, attr};

impl Terminal {
    /// Each row as characters: attribute bytes and nulls show as
    /// blanks, nondisplay runs show as blanks, dup as `*`.
    pub fn text_rows(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.rows);
        let mut a = attr::GREEN;
        for r in 0..self.rows {
            let mut line = String::with_capacity(self.cols);
            for c in 0..self.cols {
                let i = r * self.cols + c;
                let b = self.cells[i];
                if attr::is_attr(b) {
                    a = b;
                    line.push(' ');
                    continue;
                }
                if c == 0 {
                    a = self.attr_at(i);
                }
                let ch = if attr::is_nondisplay(a) {
                    ' '
                } else {
                    match b {
                        NULL => ' ',
                        DUP => '*',
                        FIELD_MARK => ';',
                        _ => ccsid::char_of(b),
                    }
                };
                line.push(if ch.is_control() { ' ' } else { ch });
            }
            out.push(line);
        }
        out
    }

    /// Each row's attributes, one character per cell: `#` at an
    /// attribute byte, else the attribute in effect as `0x20..=0x3F`
    /// mapped onto `0`..`?` and `@`..`O`.
    pub fn attr_rows(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.rows);
        let mut a = attr::GREEN;
        for r in 0..self.rows {
            let mut line = String::with_capacity(self.cols);
            for c in 0..self.cols {
                let i = r * self.cols + c;
                let b = self.cells[i];
                if attr::is_attr(b) {
                    a = b;
                    line.push('#');
                    continue;
                }
                if c == 0 {
                    a = self.attr_at(i);
                }
                line.push(char::from(b'0' + (a - 0x20)));
            }
            out.push(line);
        }
        out
    }

    /// The raw cells, for tests.
    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    /// The text of a field, nulls as blanks.
    pub fn field_text(&self, index: usize) -> Option<String> {
        let f = self.fields.get(index)?;
        Some(
            self.cells[f.start..f.end()]
                .iter()
                .map(|&b| if b == NULL { ' ' } else { ccsid::char_of(b) })
                .collect(),
        )
    }

    /// The whole screen as one string, rows joined by newlines.
    pub fn text(&self) -> String {
        self.text_rows().join("\n")
    }
}

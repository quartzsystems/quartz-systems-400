// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source view of Display Module Source: a module's lines, the
//! first one shown, the shift, the line the program stopped at and
//! `FIND`.

use crate::command::Amount;

/// A module's view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceView {
    /// `LIB/NAME` of the program.
    pub program: String,
    /// The module.
    pub module: String,
    /// The view's name (`*SOURCE`, `*LISTING`, `*STATEMENT`).
    pub view: String,
    /// The lines.
    pub lines: Vec<String>,
    /// The first line shown, from 1.
    pub top: usize,
    /// Columns shifted out on the left.
    pub left: usize,
    /// Lines on a page.
    pub page: usize,
    /// Columns of text a row shows.
    pub width: usize,
    /// Where the program stopped.
    pub current: Option<usize>,
    /// The last `FIND` string.
    pub last_find: String,
    /// `FIND` matches case.
    pub match_case: bool,
    /// The last line `FIND` stopped on.
    pub found: Option<usize>,
}

/// How far `LEFT` and `RIGHT` shift by default.
pub const SHIFT: usize = 40;

impl SourceView {
    /// A view of `lines`, `page` lines a page, `width` columns a row.
    #[must_use]
    pub fn new(program: &str, module: &str, lines: Vec<String>, page: usize, width: usize) -> Self {
        Self {
            program: program.to_string(),
            module: module.to_string(),
            view: "*SOURCE".into(),
            lines,
            top: 1,
            left: 0,
            page: page.max(1),
            width,
            current: None,
            last_find: String::new(),
            match_case: false,
            found: None,
        }
    }

    /// Number of lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the view has no lines.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn last_top(&self) -> usize {
        self.len().saturating_sub(self.page - 1).max(1)
    }

    /// The longest line.
    fn widest(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// Back toward the start (Page Up, `UP`).
    pub fn up(&mut self, a: Amount) {
        self.top = match a {
            Amount::Default => self.top.saturating_sub(self.page),
            Amount::By(n) => self.top.saturating_sub(n),
            Amount::Max => 1,
        }
        .max(1);
    }

    /// Toward the end (Page Down, `DOWN`). The last page still shows a
    /// full page when it can.
    pub fn down(&mut self, a: Amount) {
        self.top = match a {
            Amount::Default => self.top + self.page,
            Amount::By(n) => self.top + n,
            Amount::Max => self.last_top(),
        }
        .min(self.len().max(1));
    }

    /// Shift right: see further columns (`RIGHT`, F20).
    pub fn right(&mut self, a: Amount) {
        let max = self.widest().saturating_sub(self.width);
        self.left = match a {
            Amount::Default => self.left + SHIFT,
            Amount::By(n) => self.left + n,
            Amount::Max => max,
        }
        .min(max);
    }

    /// Shift left (`LEFT`, F19).
    pub fn left(&mut self, a: Amount) {
        self.left = match a {
            Amount::Default => self.left.saturating_sub(SHIFT),
            Amount::By(n) => self.left.saturating_sub(n),
            Amount::Max => 0,
        };
    }

    /// `TOP`.
    pub fn to_top(&mut self) {
        self.top = 1;
    }

    /// `BOTTOM`: the last page.
    pub fn to_bottom(&mut self) {
        self.top = self.last_top();
    }

    /// Bring `line` onto the display, as the first line shown unless it
    /// already is on the page.
    pub fn show_line(&mut self, line: usize) {
        if line < self.top || line >= self.top + self.page {
            self.top = line.clamp(1, self.len().max(1));
        }
    }

    /// The line a program stopped at, shown.
    pub fn stop_at(&mut self, line: usize) {
        self.current = Some(line);
        self.show_line(line);
    }

    /// `FIND`: a line number goes there; a string is searched for from
    /// the line after the last find (or the top shown), wrapping. The
    /// line found, or `None`.
    pub fn find(&mut self, what: &str, back: bool) -> Option<usize> {
        let what = if what.is_empty() {
            self.last_find.clone()
        } else {
            what.to_string()
        };
        if what.is_empty() {
            return None;
        }
        self.last_find.clone_from(&what);
        if let Ok(n) = what.trim().parse::<usize>() {
            if n == 0 || n > self.len() {
                return None;
            }
            self.found = Some(n);
            self.show_line(n);
            return Some(n);
        }
        let needle = if self.match_case {
            what.clone()
        } else {
            what.to_ascii_uppercase()
        };
        let n = self.len();
        if n == 0 {
            return None;
        }
        let from = self
            .found
            .unwrap_or(self.top.saturating_sub(usize::from(!back)));
        for k in 1..=n {
            let idx = if back {
                (from + 2 * n - 1 - k) % n + 1
            } else {
                (from + k - 1) % n + 1
            };
            let line = &self.lines[idx - 1];
            let hay = if self.match_case {
                line.clone()
            } else {
                line.to_ascii_uppercase()
            };
            if hay.contains(&needle) {
                self.found = Some(idx);
                self.show_line(idx);
                return Some(idx);
            }
        }
        None
    }

    /// The lines on the page: `(line number, text shifted)`.
    #[must_use]
    pub fn page_lines(&self) -> Vec<(usize, String)> {
        (self.top..self.top + self.page)
            .filter(|n| *n <= self.len())
            .map(|n| {
                let t: String = self.lines[n - 1]
                    .chars()
                    .skip(self.left)
                    .take(self.width)
                    .collect();
                (n, t)
            })
            .collect()
    }

    /// Whether the last line is on the page.
    #[must_use]
    pub fn at_bottom(&self) -> bool {
        self.top + self.page > self.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(n: usize) -> SourceView {
        let lines = (1..=n).map(|i| format!("LINE {i} text")).collect();
        SourceView::new("QGPL/P", "P", lines, 10, 60)
    }

    #[test]
    fn paging() {
        let mut v = view(25);
        v.down(Amount::Default);
        assert_eq!(v.top, 11);
        v.down(Amount::Max);
        assert_eq!(v.top, 16);
        assert!(v.at_bottom());
        v.up(Amount::By(3));
        assert_eq!(v.top, 13);
        v.up(Amount::Default);
        assert_eq!(v.top, 3);
        v.to_top();
        assert_eq!(v.top, 1);
        v.to_bottom();
        assert_eq!(v.page_lines().len(), 10);
        assert_eq!(v.page_lines()[9].0, 25);
    }

    #[test]
    fn shifting() {
        let mut v = view(3);
        v.lines[0] = "x".repeat(130);
        v.right(Amount::Default);
        assert_eq!(v.left, 40);
        v.right(Amount::Max);
        assert_eq!(v.left, 70);
        v.right(Amount::Default);
        assert_eq!(v.left, 70);
        v.left(Amount::By(5));
        assert_eq!(v.left, 65);
        v.left(Amount::Max);
        assert_eq!(v.left, 0);
    }

    #[test]
    fn finding() {
        let mut v = view(30);
        assert_eq!(v.find("line 2 ", false), Some(2));
        assert_eq!(v.find("LINE 2", false), Some(20));
        assert_eq!(v.find("", false), Some(21));
        assert_eq!(v.find("", true), Some(20));
        assert_eq!(v.find("25", false), Some(25));
        assert_eq!(v.find("nothing", false), None);
        v.match_case = true;
        assert_eq!(v.find("line", false), None);
    }

    #[test]
    fn stops() {
        let mut v = view(40);
        v.stop_at(33);
        assert_eq!(v.current, Some(33));
        assert_eq!(v.top, 33);
        v.stop_at(35);
        assert_eq!(v.top, 33);
    }
}

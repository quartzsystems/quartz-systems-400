// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A query's rows as Display Data shows them: each column as wide as
//! its type or heading, numbers on the right, nulls as `-`; the lines
//! paged by the display's height and shifted by its width; the same
//! lines as a printed report.

use std::fmt::Write as _;

/// A result column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// The heading: the column's name.
    pub name: String,
    /// Its SQL type, e.g. `DECIMAL(7,0)`.
    pub data_type: String,
}

impl Column {
    /// The width its values take, from the type.
    #[must_use]
    pub fn type_width(&self) -> usize {
        let t = self.data_type.to_ascii_uppercase();
        let (base, args) = match t.split_once('(') {
            Some((b, a)) => (b.trim().to_string(), a.trim_end_matches(')').to_string()),
            None => (t.trim().to_string(), String::new()),
        };
        let nums: Vec<usize> = args
            .split(',')
            .filter_map(|a| a.trim().parse().ok())
            .collect();
        let first = nums.first().copied();
        match base.as_str() {
            "SMALLINT" => 6,
            "INTEGER" | "INT" => 11,
            "BIGINT" => 20,
            "DECIMAL" | "NUMERIC" | "DEC" => {
                let p = first.unwrap_or(5);
                let s = nums.get(1).copied().unwrap_or(0);
                p + 1 + usize::from(s > 0)
            }
            "REAL" | "FLOAT" | "DOUBLE" => 22,
            "DATE" => 10,
            "TIME" => 8,
            "TIMESTAMP" => 26,
            _ => first.unwrap_or(1).min(MAX_WIDTH),
        }
    }

    /// Whether values sit on the right.
    #[must_use]
    pub fn numeric(&self) -> bool {
        let t = self.data_type.to_ascii_uppercase();
        [
            "SMALLINT", "INTEGER", "INT", "BIGINT", "DECIMAL", "NUMERIC", "DEC", "REAL", "FLOAT",
            "DOUBLE",
        ]
        .iter()
        .any(|b| t == *b || t.starts_with(&format!("{b}(")))
    }

    /// The column's width on the line.
    #[must_use]
    pub fn width(&self) -> usize {
        self.type_width().max(self.name.chars().count())
    }
}

/// The widest a single column is shown.
pub const MAX_WIDTH: usize = 32766;

/// Blanks between columns.
pub const GAP: usize = 2;

/// The rows of a query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Data {
    /// Columns.
    pub columns: Vec<Column>,
    /// Rows, `None` for a null.
    pub rows: Vec<Vec<Option<String>>>,
}

impl Data {
    /// The width of a line: every column and the gaps between them.
    #[must_use]
    pub fn width(&self) -> usize {
        let cols: usize = self.columns.iter().map(Column::width).sum();
        cols + GAP * self.columns.len().saturating_sub(1)
    }

    fn cells(&self, values: impl Iterator<Item = (usize, String)>) -> String {
        let mut out = String::new();
        for (i, v) in values {
            let c = &self.columns[i];
            let w = c.width();
            if i > 0 {
                out.push_str(&" ".repeat(GAP));
            }
            // A value narrower than its heading sits in the middle of it.
            let tw = c.type_width().min(w);
            let lead = (w - tw) / 2;
            let v: String = v.chars().take(tw).collect();
            let cell = if c.numeric() {
                format!("{:lead$}{v:>tw$}", "")
            } else {
                format!("{:lead$}{v:<tw$}", "")
            };
            let _ = write!(out, "{cell:<w$}");
        }
        out
    }

    /// The heading line.
    #[must_use]
    pub fn heading(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                out.push_str(&" ".repeat(GAP));
            }
            let w = c.width();
            let _ = write!(out, "{:<w$}", c.name);
        }
        out
    }

    /// Row `i` as a line.
    #[must_use]
    pub fn line(&self, i: usize) -> String {
        let Some(r) = self.rows.get(i) else {
            return String::new();
        };
        self.cells(
            r.iter()
                .enumerate()
                .map(|(j, v)| (j, v.clone().unwrap_or_else(|| "-".to_string()))),
        )
    }
}

/// The part of `line` from column `shift` (1-based), `width` wide.
#[must_use]
pub fn window(line: &str, shift: usize, width: usize) -> String {
    line.chars()
        .skip(shift.saturating_sub(1))
        .take(width)
        .collect()
}

/// The scale above the heading: `....+....1....+....2` from `shift`,
/// each tens number ending on its column (`...10`, `..100`).
#[must_use]
pub fn scale(shift: usize, width: usize) -> String {
    let end = shift + width;
    let mut out: Vec<char> = (shift..end)
        .map(|c| if c % 5 == 0 { '+' } else { '.' })
        .collect();
    let mut tens = shift.div_ceil(10) * 10;
    while tens < end + 10 {
        let digits = (tens / 10).to_string();
        for (k, ch) in digits.chars().rev().enumerate() {
            if let Some(c) = tens.checked_sub(k)
                && c >= shift
                && c < end
            {
                out[c - shift] = ch;
            }
        }
        tens += 10;
    }
    out.into_iter().collect()
}

/// Where Display Data stands: the first row shown and the first
/// column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pager {
    /// The first row shown, from 0.
    pub top: usize,
    /// The first column shown, from 1.
    pub shift: usize,
    /// Rows a page shows.
    pub page: usize,
    /// Columns a page shows.
    pub cols: usize,
}

impl Pager {
    /// At the start.
    #[must_use]
    pub fn new(page: usize, cols: usize) -> Self {
        Self {
            top: 0,
            shift: 1,
            page,
            cols,
        }
    }

    /// Page Down over `rows` rows (the end-of-data line counts as one).
    pub fn down(&mut self, rows: usize) {
        if self.top + self.page <= rows {
            self.top += self.page;
        }
    }

    /// Page Up.
    pub fn up(&mut self) {
        self.top = self.top.saturating_sub(self.page);
    }

    /// F20: right by a page width, not past the data's last column.
    pub fn right(&mut self, width: usize) {
        if self.shift + self.cols <= width {
            self.shift += self.cols;
        }
    }

    /// F19: left by a page width.
    pub fn left(&mut self) {
        self.shift = self.shift.saturating_sub(self.cols).max(1);
    }

    /// Position to a line: a number from 1, `T` for the top, `B` for
    /// the bottom page.
    pub fn position(&mut self, to: &str, rows: usize) -> bool {
        let t = to.trim().to_ascii_uppercase();
        match t.as_str() {
            "" => return true,
            "T" | "*TOP" => self.top = 0,
            "B" | "*BOT" => self.top = (rows + 1).saturating_sub(self.page),
            n => match n.parse::<usize>() {
                Ok(n) if n >= 1 => self.top = (n - 1).min(rows),
                _ => return false,
            },
        }
        true
    }

    /// Shift to a column, from 1.
    pub fn shift_to(&mut self, to: &str, width: usize) -> bool {
        match to.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= width.max(1) => {
                self.shift = n;
                true
            }
            _ => to.trim().is_empty(),
        }
    }

    /// Whether the end-of-data line is on this page.
    #[must_use]
    pub fn at_bottom(&self, rows: usize) -> bool {
        self.top + self.page > rows
    }
}

/// The end-of-data line.
pub const END_OF_DATA: &str = "********  End of data  ********";

/// The printed report of a query: the statement, the heading and the
/// rows, the end line.
#[must_use]
pub fn report(statement: &str, data: &Data, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in crate::history::wrap(statement, width) {
        out.push(chunk);
    }
    out.push(String::new());
    let w = data.width();
    let mut shift = 1;
    while shift <= w.max(1) {
        out.push(window(&data.heading(), shift, width));
        for i in 0..data.rows.len() {
            out.push(window(&data.line(i), shift, width));
        }
        shift += width;
        if shift <= w {
            out.push(String::new());
        }
    }
    out.push(END_OF_DATA.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Data {
        Data {
            columns: vec![
                Column {
                    name: "NAME".into(),
                    data_type: "CHAR(6)".into(),
                },
                Column {
                    name: "QTY".into(),
                    data_type: "DECIMAL(5,0)".into(),
                },
            ],
            rows: vec![
                vec![Some("BOLT".into()), Some("12".into())],
                vec![Some("NUT".into()), None],
            ],
        }
    }

    #[test]
    fn widths_and_lines() {
        let d = data();
        assert_eq!(d.columns[1].width(), 6);
        assert_eq!(d.width(), 6 + 2 + 6);
        assert_eq!(d.heading(), "NAME    QTY   ");
        assert_eq!(d.line(0), "BOLT        12");
        assert_eq!(d.line(1), "NUT          -");
        let one = Data {
            columns: vec![Column {
                name: "IBMREQD".into(),
                data_type: "CHAR(1)".into(),
            }],
            rows: vec![vec![Some("Y".into())]],
        };
        assert_eq!(one.width(), 7);
        assert_eq!(one.line(0), "   Y   ");
    }

    #[test]
    fn scales() {
        assert_eq!(scale(1, 12), "....+....1..");
        assert_eq!(scale(75, 6), "+....8");
    }

    #[test]
    fn paging() {
        let mut p = Pager::new(10, 80);
        p.down(25);
        assert_eq!(p.top, 10);
        p.down(25);
        assert_eq!(p.top, 20);
        assert!(p.at_bottom(25));
        p.down(25);
        assert_eq!(p.top, 20);
        p.up();
        assert_eq!(p.top, 10);
        assert!(p.position("B", 25));
        assert_eq!(p.top, 16);
        assert!(p.position("3", 25));
        assert_eq!(p.top, 2);
        assert!(!p.position("x", 25));
        p.right(200);
        assert_eq!(p.shift, 81);
        p.right(200);
        assert_eq!(p.shift, 161);
        p.right(200);
        assert_eq!(p.shift, 161);
        p.left();
        assert_eq!(p.shift, 81);
    }

    #[test]
    fn printed() {
        let r = report("SELECT * FROM T", &data(), 132);
        assert_eq!(r[0], "SELECT * FROM T");
        assert_eq!(r.last().map(String::as_str), Some(END_OF_DATA));
    }
}

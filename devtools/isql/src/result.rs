// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A query's rows as Display Data shows them: each column as wide as
//! its edited values or its heading, numbers on the right with their
//! heading over the last digit, nulls as `-`; the lines paged by the
//! display's height and shifted by its width; the same lines as the
//! printed report (`QPQUPRFIL`). The widths are pub400's
//! (`tools/conformance/golden/sql/strsql/strsql-fmt*`).

use std::fmt::Write as _;

use crate::format::Format;

/// A result column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// The heading: the column's name.
    pub name: String,
    /// Its SQL type, e.g. `DECIMAL(7, 0)`.
    pub data_type: String,
}

/// What a column holds, for its width and its edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Text of this many characters.
    Text(usize),
    /// A decimal of precision and scale.
    Decimal(usize, usize),
    /// A binary integer of this many digits.
    Integer(usize),
    /// A float with this many digits after the point.
    Float(usize),
    /// A date.
    Date,
    /// A time.
    Time,
    /// A timestamp of this many characters.
    Timestamp(usize),
}

impl Column {
    /// What the column holds, from its type.
    #[must_use]
    pub fn kind(&self) -> Kind {
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
            "SMALLINT" => Kind::Integer(5),
            "INTEGER" | "INT" => Kind::Integer(10),
            "BIGINT" => Kind::Integer(19),
            "DECIMAL" | "NUMERIC" | "DEC" => {
                Kind::Decimal(first.unwrap_or(5), nums.get(1).copied().unwrap_or(0))
            }
            "DOUBLE" | "FLOAT" => Kind::Float(16),
            "REAL" => Kind::Float(7),
            "DATE" => Kind::Date,
            "TIME" => Kind::Time,
            "TIMESTAMP" => Kind::Timestamp(match first {
                Some(0) => 19,
                Some(p) => 20 + p,
                None => 26,
            }),
            _ => Kind::Text(first.unwrap_or(1).min(MAX_WIDTH)),
        }
    }

    /// The width its values take under `f`.
    #[must_use]
    pub fn type_width(&self, f: &Format) -> usize {
        match self.kind() {
            Kind::Text(n) | Kind::Timestamp(n) => n,
            Kind::Decimal(p, s) => {
                let int = p.saturating_sub(s);
                p + int.saturating_sub(1) / 3 + usize::from(s > 0) + 1
            }
            Kind::Integer(d) => d + (d - 1) / 3 + 1,
            Kind::Float(d) => d + 8,
            Kind::Date => f.date.width(),
            Kind::Time => 8,
        }
    }

    /// Whether values sit on the right.
    #[must_use]
    pub fn numeric(&self) -> bool {
        matches!(self.kind(), Kind::Decimal(..) | Kind::Integer(_) | Kind::Float(_))
    }

    /// The column's width on the line.
    #[must_use]
    pub fn width(&self, f: &Format) -> usize {
        self.type_width(f).max(self.name.chars().count())
    }

    /// A value as shown.
    #[must_use]
    pub fn show(&self, v: &str, f: &Format) -> String {
        match self.kind() {
            // A positive number leaves the sign's column blank.
            Kind::Decimal(..) | Kind::Integer(_) => {
                let n = f.number(v);
                if n.ends_with('-') { n } else { format!("{n} ") }
            }
            Kind::Float(d) => f.float(v, d),
            Kind::Date => f.date(v),
            Kind::Time => f.time(v),
            Kind::Text(_) | Kind::Timestamp(_) => v.to_string(),
        }
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
    /// How the values are shown.
    pub fmt: Format,
}

impl Data {
    /// The width of a line: every column and the gaps between them.
    #[must_use]
    pub fn width(&self) -> usize {
        let cols: usize = self.columns.iter().map(|c| c.width(&self.fmt)).sum();
        cols + GAP * self.columns.len().saturating_sub(1)
    }

    /// The heading line: text and dates on the left, numbers over
    /// their last digit (a float's over its last character).
    #[must_use]
    pub fn heading(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.columns.iter().enumerate() {
            if i > 0 {
                out.push_str(&" ".repeat(GAP));
            }
            let w = c.width(&self.fmt);
            let n = c.name.chars().count();
            let end = match c.kind() {
                Kind::Decimal(..) | Kind::Integer(_) if n < w => w - 1,
                Kind::Float(_) => w,
                _ => n,
            };
            let _ = write!(out, "{:>end$}{:rest$}", c.name, "", rest = w - end);
        }
        out
    }

    /// Row `i` as a line.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // row, column, value
    pub fn line(&self, i: usize) -> String {
        let Some(r) = self.rows.get(i) else {
            return String::new();
        };
        let mut out = String::new();
        for (j, v) in r.iter().enumerate() {
            let Some(c) = self.columns.get(j) else {
                break;
            };
            if j > 0 {
                out.push_str(&" ".repeat(GAP));
            }
            let w = c.width(&self.fmt);
            let tw = c.type_width(&self.fmt).min(w);
            // A value narrower than its heading sits in the middle of it.
            let lead = (w - tw) / 2;
            let cell = match v {
                None if c.numeric() => format!("{:>w$}", "-"),
                None => format!("{:lead$}{:<tw$}", "", "-"),
                Some(v) => {
                    let v: String = c.show(v, &self.fmt).chars().take(tw).collect();
                    if c.numeric() {
                        format!("{:lead$}{v:>tw$}", "")
                    } else {
                        format!("{:lead$}{v:<tw$}", "")
                    }
                }
            };
            let _ = write!(out, "{cell:<w$}");
        }
        out
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

    /// F20: right by a page width. The last page ends on the data's last column; false when that
    /// page is already shown.
    pub fn right(&mut self, width: usize) -> bool {
        let last = width.saturating_sub(self.cols) + 1;
        if self.shift >= last {
            return false;
        }
        self.shift = (self.shift + self.cols).min(last);
        true
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
            fmt: Format::default(),
        }
    }

    #[test]
    fn widths_and_lines() {
        let d = data();
        assert_eq!(d.columns[1].width(&d.fmt), 7);
        assert_eq!(d.width(), 6 + 2 + 7);
        assert_eq!(d.heading(), "NAME       QTY ");
        assert_eq!(d.line(0), "BOLT        12 ");
        assert_eq!(d.line(1), "NUT           -");
        let one = Data {
            columns: vec![Column {
                name: "IBMREQD".into(),
                data_type: "CHAR(1)".into(),
            }],
            rows: vec![vec![Some("Y".into())]],
            fmt: Format::default(),
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
        assert!(p.right(200));
        assert_eq!(p.shift, 81);
        assert!(p.right(200));
        assert_eq!(p.shift, 121);
        assert!(!p.right(200));
        assert_eq!(p.shift, 121);
        p.left();
        assert_eq!(p.shift, 41);
    }

}

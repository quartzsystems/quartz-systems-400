// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Working with the data in a file: the record display (the key line
//! on row 4, the fields from row 5, a long field going on at column 1
//! of the next row), Select Record Format and End Data Entry.

use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target};

use super::{blank, bottom_line, input, keys, leader, out, put};
use crate::session::{Mode, Session};

/// One line of the record display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The field's name; empty for an output.
    pub name: String,
    /// The label (the field name, or the record number's heading).
    pub label: String,
    /// Positions on the display.
    pub width: usize,
    /// The value.
    pub value: String,
    /// Typed into.
    pub input: bool,
}

impl Row {
    /// An input.
    #[must_use]
    pub fn input(name: &str, label: &str, width: usize, value: &str) -> Self {
        Self {
            name: name.to_string(),
            label: label.to_string(),
            width,
            value: value.to_string(),
            input: true,
        }
    }

    /// An output.
    #[must_use]
    pub fn output(label: &str, width: usize, value: &str) -> Self {
        Self {
            name: String::new(),
            label: label.to_string(),
            width,
            value: value.to_string(),
            input: false,
        }
    }

    /// The value's column: after the label and a blank, at 11 at the
    /// least.
    #[must_use]
    pub fn value_col(&self) -> usize {
        (2 + self.label.chars().count() + 2).max(11)
    }

    /// Rows the line takes: a value past column 80 goes on at column 1.
    #[must_use]
    pub fn height(&self) -> usize {
        let end = self.value_col() - 1 + self.width.max(1);
        end.div_ceil(80).max(1)
    }
}

/// What the record display shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The title: `WORK WITH DATA IN A FILE` or the program's job title.
    pub title: String,
    /// The mode.
    pub mode: Mode,
    /// The record format.
    pub format: String,
    /// The file.
    pub file: String,
    /// The key line (row 4): the record number or the key fields.
    pub keys: Vec<Row>,
    /// The fields.
    pub rows: Vec<Row>,
    /// The page of fields, from 0.
    pub page: usize,
    /// The message line.
    pub message: Option<String>,
    /// Show the copyright line (the first display).
    pub copyright: bool,
}

/// The first row for fields.
const FIRST: usize = 5;
/// The last row for fields.
const LAST: usize = 21;

/// The fields of each page, as index ranges.
fn paged(rows: &[Row], first: usize) -> Vec<std::ops::Range<usize>> {
    let mut pages = Vec::new();
    let mut start = 0;
    let mut row = first;
    for (i, r) in rows.iter().enumerate() {
        let h = r.height();
        if row + h - 1 > LAST && i > start {
            pages.push(start..i);
            start = i;
            row = FIRST;
        }
        row += h;
    }
    pages.push(start..rows.len());
    pages
}

/// Pages of fields the view has.
#[must_use]
pub fn pages(v: &View) -> usize {
    paged(&v.rows, FIRST + v.keys.len().saturating_sub(1)).len()
}

fn line(s: &mut Screen, row: usize, r: &Row) {
    put(s, row, 2, Attr::Normal, &format!("{}:", r.label));
    let col = r.value_col();
    if r.input {
        s.field(Field {
            row,
            col,
            len: r.width,
            attr: Attr::Underline,
            value: r.value.chars().take(r.width).collect(),
            target: Target::Var(r.name.clone()),
            usage: Usage::Inout,
            shift: Shift::None,
            required: false,
            autoskip: false,
        });
    } else {
        out(s, row, col, &r.value);
    }
}

/// The record display.
#[must_use]
pub fn screen(v: &View) -> Screen {
    let mut s = blank();
    // The data reaches column 80, which the panels keep free.
    s.cols = 81;
    put(&mut s, 1, 2, Attr::Normal, &v.title);
    out(&mut s, 1, 49, "Mode . . . . :");
    out(&mut s, 1, 66, v.mode.word());
    out(&mut s, 2, 2, "Format . . . . :");
    out(&mut s, 2, 21, &v.format);
    out(&mut s, 2, 49, "File . . . . :");
    out(&mut s, 2, 66, &v.file);
    let mut row = 4;
    for k in &v.keys {
        line(&mut s, row, k);
        row += k.height();
    }
    let first = FIRST + v.keys.len().saturating_sub(1);
    let ranges = paged(&v.rows, first);
    let page = v.page.min(ranges.len() - 1);
    let mut row = if page == 0 { first } else { FIRST };
    for r in &v.rows[ranges[page].clone()] {
        line(&mut s, row, r);
        row += r.height();
    }
    if ranges.len() > 1 {
        let last = page + 1 == ranges.len();
        put(
            &mut s,
            21,
            if last { 74 } else { 73 },
            Attr::HighIntensity,
            if last { "Bottom" } else { "More..." },
        );
    }
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit"),
            (22, 26, "F5=Refresh"),
            (22, 51, "F6=Select format"),
            (23, 2, "F9=Insert"),
            (23, 26, "F10=Entry"),
            (23, 51, "F11=Change"),
        ],
    );
    bottom_line(&mut s, v.message.as_deref(), v.copyright);
    s.cursor = s.fields.first().map(|f| (f.row, f.col));
    s
}

/// Select Record Format: each format's name and text.
#[must_use]
pub fn formats(list: &[(String, String)], message: Option<&str>) -> Screen {
    let mut s = blank();
    put(&mut s, 1, 29, Attr::HighIntensity, "Select Record Format");
    put(&mut s, 3, 2, Attr::Blue, "Type option, press Enter.");
    put(&mut s, 4, 4, Attr::Blue, "1=Select");
    put(&mut s, 7, 2, Attr::HighIntensity, "Opt  Format         Description");
    for (i, (name, text)) in list.iter().take(13).enumerate() {
        input(
            &mut s,
            8 + i,
            3,
            1,
            Target::ListOption {
                list: "FORMATS".into(),
                row_index: i,
            },
            "",
        );
        out(&mut s, 8 + i, 7, name);
        out(&mut s, 8 + i, 22, text);
    }
    put(&mut s, 22, 74, Attr::HighIntensity, "Bottom");
    keys(
        &mut s,
        &[(23, 2, "F3=Exit"), (23, 19, "F5=Refresh"), (23, 39, "F12=Cancel")],
    );
    bottom_line(&mut s, message, false);
    s.cursor = Some((8, 3));
    s
}

/// End Data Entry with the session's counts.
#[must_use]
pub fn end(sess: &Session, answer: &str, message: Option<&str>) -> Screen {
    let mut s = blank();
    put(&mut s, 1, 33, Attr::HighIntensity, "End Data Entry");
    out(&mut s, 3, 4, "Number of records processed");
    for (row, label, n) in [
        (5, "Added  . . . . . :", sess.added),
        (6, "Changed  . . . . :", sess.changed),
        (7, "Deleted  . . . . :", sess.deleted),
    ] {
        out(&mut s, row, 6, label);
        out(&mut s, row, 28, &format!("{n:>8}"));
    }
    put(&mut s, 12, 2, Attr::Blue, "Type choice, press Enter.");
    out(&mut s, 14, 4, &leader("End data entry", 4, 32));
    pdm::screen::var(&mut s, 14, 37, 1, "END", answer);
    out(&mut s, 14, 49, "Y=Yes,  N=No");
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 16, "F12=Cancel")]);
    bottom_line(&mut s, message, false);
    s.cursor = Some((14, 37));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights() {
        let r = Row::input("A", "A", 70, "");
        assert_eq!(r.value_col(), 11);
        assert_eq!(r.height(), 1);
        assert_eq!(Row::input("I", "INTER", 80, "").height(), 2);
        assert_eq!(Row::input("X", "ABCDEFGHIJ", 10, "").value_col(), 14);
    }

    #[test]
    fn pages_of_long_records() {
        let rows: Vec<Row> = (0..30)
            .map(|i| Row::input(&format!("F{i}"), &format!("F{i}"), 5, ""))
            .collect();
        let v = View {
            title: "T".into(),
            mode: Mode::Entry,
            format: "R".into(),
            file: "F".into(),
            keys: Vec::new(),
            rows,
            page: 1,
            message: None,
            copyright: false,
        };
        assert_eq!(pages(&v), 2);
        let s = screen(&v);
        assert_eq!(s.fields.len(), 13);
        assert!(s.text_rows()[20].contains("Bottom"));
    }
}

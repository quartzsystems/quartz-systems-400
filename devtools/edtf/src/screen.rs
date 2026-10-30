// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The edit and browse displays on a 5250 screen model, in the geometry
//! the pub400 goldens show (`tools/conformance/golden/phase10/ifs/`):
//! the title on row 1, the record and column fields on row 2, the
//! control field on row 3, the ruler on row 5 (after `CMD` when
//! editing), the data from row 6 (15 rows editing with a 3-column `CMD`
//! area, 16 browsing from column 1), the keys on rows 22 and 23, the
//! message on row 24.

use tn5250::ds::attr;
use tn5250::ds::field::Ffw;
use tn5250::screen::ScreenModel;

use crate::session::{Input, Item, Mode, RowInput, Session};

/// The first data row.
pub const FIRST_ROW: usize = 6;

const BEGIN: &str = "************Beginning of data**************";
const END: &str = "************End of Data********************";

/// Where the fields are: (row, column, length).
const RECORD: (usize, usize, usize) = (2, 11, 9);
const REC_BY: (usize, usize, usize) = (2, 35, 14);
const COLUMN: (usize, usize, usize) = (2, 59, 6);
const COL_BY: (usize, usize, usize) = (2, 74, 7);
const CONTROL: (usize, usize, usize) = (3, 12, 69);

/// The keys of each display, two rows.
#[must_use]
pub fn keys(s: &Session) -> [&'static str; 2] {
    match (s.mode, s.hex) {
        (Mode::Edit, _) => [
            "F2=Save   F3=Save/Exit   F12=Exit   F15=Services   F16=Repeat find",
            "F17=Repeat change  F19=Left  F20=Right",
        ],
        (Mode::Browse, false) => [
            "F3=Exit   F10=Display Hex   F12=Exit  F15=Services  F16=Repeat find",
            "F19=Left   F20=Right",
        ],
        (Mode::Browse, true) => [
            "F3=Exit   F10=Display Text  F12=Exit  F15=Services  F16=Repeat find",
            "F19=Left   F20=Right",
        ],
    }
}

/// The ruler from column `left`, `width` long.
#[must_use]
pub fn ruler(left: usize, width: usize) -> String {
    (left..left + width)
        .map(|p| {
            if p % 10 == 0 {
                char::from_digit(u32::try_from(p / 10 % 10).unwrap_or(0), 10).unwrap_or('0')
            } else if p % 5 == 0 {
                '+'
            } else {
                '.'
            }
        })
        .collect()
}

fn input(m: &mut ScreenModel, at: (usize, usize, usize), value: &str) {
    m.field(at.0, at.1, at.2, attr::GREEN_UL, value, Ffw::default(), &[]);
}

/// Text in CCSID 37 for cells written without an attribute.
fn bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x6F))
        .collect()
}

/// Draw the session.
pub fn draw(s: &Session, m: &mut ScreenModel) {
    m.clear();
    let edit = s.mode == Mode::Edit;
    if edit {
        m.put(1, 2, "Edit File:", attr::WHITE);
        m.put(1, 13, &s.path, attr::GREEN);
    } else {
        m.put(1, 2, "Browse :", attr::WHITE);
        m.put(1, 11, &s.path, attr::GREEN);
    }
    m.put(2, 2, "Record :", attr::GREEN);
    input(m, RECORD, &format!("{:>7}", s.top));
    m.put(2, 21, "of", attr::GREEN);
    m.put(2, 24, &format!("{:>7}", s.buf.len()), attr::GREEN);
    m.put(2, 32, "by", attr::GREEN);
    input(m, REC_BY, &format!("{:>3}", s.rec_by));
    m.put(2, 50, "Column :", attr::GREEN);
    input(m, COLUMN, &format!("{:>4}", s.left));
    m.put(2, 66, &format!("{:>4}", s.rcdlen), attr::GREEN);
    m.put(2, 71, "by", attr::GREEN);
    input(m, COL_BY, &format!("{:>3}", s.col_by));
    m.put(3, 2, "Control :", attr::GREEN);
    input(m, CONTROL, "");
    let width = s.width();
    if edit {
        m.put(5, 1, "CMD", attr::WHITE);
        m.put(5, 5, &ruler(s.left, width), attr::WHITE);
    } else {
        m.put(5, 1, &ruler(s.left, width), attr::WHITE);
    }
    for (r, item) in s.layout().into_iter().enumerate() {
        let row = FIRST_ROW + r;
        if edit {
            let cmd_attr = if matches!(item, Item::Line(..)) {
                attr::GREEN_UL
            } else {
                attr::NONDISPLAY
            };
            let ffw = Ffw {
                monocase: true,
                ..Ffw::default()
            };
            m.field(row, 1, 3, cmd_attr, "", ffw, &[]);
            match item {
                Item::Begin => m.put(row, 5, &format!(" {BEGIN}"), attr::WHITE),
                Item::End => m.put(row, 5, &format!(" {END}"), attr::WHITE),
                Item::Line(i, _) => {
                    m.field(row, 5, width, attr::GREEN, &s.window(i), Ffw::default(), &[]);
                }
                Item::Empty => m.put(row, 5, "", attr::WHITE),
            }
        } else {
            let text = match item {
                Item::Begin => format!(" {BEGIN}"),
                Item::End => format!(" {END}"),
                Item::Line(i, 0) => s.window(i),
                Item::Line(i, sub) => {
                    let (hi, lo) = s.hex_rows(i);
                    if sub == 1 { hi } else { lo }
                }
                Item::Empty => String::new(),
            };
            m.put_bytes(row, 1, &bytes(&text));
        }
    }
    let [k1, k2] = keys(s);
    m.put(22, 2, k1, attr::BLUE);
    m.put(23, 2, k2, attr::BLUE);
    if let Some(msg) = &s.message {
        m.error_line(&msg.text, attr::WHITE);
    }
    let cursor = s
        .cursor
        .and_then(|(id, col)| s.row_of(id).map(|r| (r, col)))
        .map(|(r, col)| {
            let col = col.saturating_sub(s.left - 1).min(width - 1);
            if edit { (FIRST_ROW + r, 5 + col) } else { (FIRST_ROW + r, 1 + col) }
        });
    match cursor {
        Some((r, c)) => m.set_cursor(r, c),
        None => m.set_cursor(CONTROL.0, CONTROL.1),
    }
}

/// The text of the field at a position when the operator changed it.
fn changed(m: &ScreenModel, at: (usize, usize)) -> Option<String> {
    let i = m.field_index_at(at.0, at.1)?;
    let f = m.fields().get(i)?;
    f.modified.then(|| f.text())
}

fn value(m: &ScreenModel, at: (usize, usize)) -> String {
    m.field_index_at(at.0, at.1)
        .and_then(|i| m.fields().get(i))
        .map(tn5250::screen::Field::text)
        .unwrap_or_default()
}

/// Read what was typed on the display `draw` drew for `s`.
#[must_use]
pub fn read(s: &Session, m: &ScreenModel) -> Input {
    let mut rows = Vec::new();
    if s.mode == Mode::Edit {
        for r in 0..s.rows() {
            let row = FIRST_ROW + r;
            let cmd = changed(m, (row, 1));
            let data = changed(m, (row, 5));
            if cmd.is_some() || data.is_some() {
                rows.push(RowInput { row: r, cmd, data });
            }
        }
    }
    Input {
        rows,
        control: value(m, (CONTROL.0, CONTROL.1)),
        record: changed(m, (RECORD.0, RECORD.1)),
        rec_by: changed(m, (REC_BY.0, REC_BY.1)),
        column: changed(m, (COLUMN.0, COLUMN.1)),
        col_by: changed(m, (COL_BY.0, COL_BY.1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rulers() {
        assert_eq!(&ruler(1, 12), "....+....1..");
        assert_eq!(&ruler(8, 5), "..1..");
    }
}

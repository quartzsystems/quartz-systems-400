// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work with Display Records: the member's records with their order,
//! type, related subfile record, date and DDS error flag (or their text
//! with F11), the add line on top, two key sets.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, member_head, message, more, put, title, var};

/// The list's name in the screen's targets.
pub const LIST: &str = "RECORDS";
/// Entries on a page.
pub const PAGE: usize = 9;
/// The first entry row.
pub const FIRST_ROW: usize = 12;

/// One record on the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Its order number (10, 20, ...).
    pub order: u32,
    /// The record name.
    pub name: String,
    /// `RECORD`, `SFL`, `SFLCTL`, ...
    pub ty: String,
    /// The related subfile record.
    pub related: String,
    /// The date it was last changed, as shown.
    pub date: String,
    /// The DDS has errors.
    pub error: bool,
    /// Its `TEXT`.
    pub text: String,
}

/// What the display shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The source file.
    pub file: String,
    /// Its library.
    pub lib: String,
    /// The member.
    pub member: String,
    /// The member's source type.
    pub srctype: String,
    /// The text column instead of the date (F11).
    pub text: bool,
    /// The second key set (F24).
    pub keys2: bool,
    /// The first entry shown.
    pub top: usize,
}

/// The display.
#[must_use]
#[allow(clippy::too_many_lines)] // one display, laid out in order
pub fn records(v: &View, rows: &[Row], add: (&str, &str), msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 23, "Work with Display Records");
    member_head(&mut s, &v.file, &v.lib, &v.member, &v.srctype);
    put(&mut s, 6, 2, Attr::Blue, "Type options, press Enter.");
    for (row, col, t) in [
        (7, 4, "1=Add"),
        (7, 22, "2=Edit comments"),
        (7, 47, "3=Copy"),
        (7, 63, "4=Delete"),
        (8, 4, "7=Rename"),
        (8, 22, "8=Select keywords"),
        (8, 46, "12=Design image"),
    ] {
        put(&mut s, row, col, Attr::Blue, t);
    }
    let heads: &[(usize, &str)] = if v.text {
        &[(2, "Opt"), (7, "Order"), (16, "Record"), (30, "Type"), (39, "Text")]
    } else {
        &[
            (2, "Opt"),
            (7, "Order"),
            (16, "Record"),
            (30, "Type"),
            (40, "Related Subfile"),
            (58, "Date"),
            (70, "DDS Error"),
        ]
    };
    for (col, t) in heads {
        put(&mut s, 10, *col, Attr::HighIntensity, t);
    }
    // The add line: an option and a new record's name.
    var(&mut s, 11, 2, 4, "ADDOPT", add.0);
    var(&mut s, 11, 16, 10, "ADDNAME", add.1);
    if rows.is_empty() {
        put(&mut s, 13, 4, Attr::Normal, "(No records in file)");
    }
    for (i, r) in rows.iter().enumerate().skip(v.top).take(PAGE) {
        let row = FIRST_ROW + i - v.top;
        input(
            &mut s,
            row,
            2,
            4,
            Target::ListOption {
                list: LIST.into(),
                row_index: i,
            },
            "",
        );
        input(
            &mut s,
            row,
            7,
            8,
            Target::ListCell {
                list: LIST.into(),
                row_index: i,
                var: "ORDER".into(),
            },
            &format!("{:>5}", r.order),
        );
        put(&mut s, row, 16, Attr::Normal, &r.name);
        put(&mut s, row, 30, Attr::Normal, &r.ty);
        if v.text {
            put(&mut s, row, 39, Attr::Normal, &r.text);
        } else {
            put(&mut s, row, 40, Attr::Normal, &r.related);
            put(&mut s, row, 58, Attr::Normal, &r.date);
            if r.error {
                put(&mut s, row, 73, Attr::HighIntensity, "Y");
            }
        }
    }
    more(&mut s, 21, v.top + PAGE >= rows.len());
    if v.keys2 {
        keys(
            &mut s,
            &[
                (22, 2, "F5=Refresh"),
                (
                    22,
                    17,
                    if v.text {
                        "F11=Display date"
                    } else {
                        "F11=Display text"
                    },
                ),
                (22, 38, "F13=Change defaults"),
                (22, 62, "F24=More keys"),
            ],
        );
    } else {
        keys(
            &mut s,
            &[
                (22, 2, "F3=Exit"),
                (22, 30, "F12=Cancel"),
                (22, 46, "F14=File-level keywords"),
                (23, 2, "F15=File-level comments"),
                (23, 30, "F17=Subset"),
                (23, 46, "F24=More keys"),
            ],
        );
    }
    message(&mut s, msg);
    s.cursor = Some((11, 2));
    s
}

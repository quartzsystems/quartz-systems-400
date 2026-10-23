// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display Data: a query's rows under the data width, the position-to
//! and shift-to fields, the scale and the heading, paged and shifted.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, more, put, title, var};
use crate::result::{Data, END_OF_DATA, Pager, scale, window};

/// The first row of the heading.
pub const HEAD_ROW: usize = 5;
/// The last row of data.
pub const LAST_ROW: usize = 21;
/// The columns the data shows.
pub const DATA_COLS: usize = 79;

/// Rows of data a page shows under a heading of `head` lines.
#[must_use]
pub fn page_rows(head: usize) -> usize {
    LAST_ROW + 1 - HEAD_ROW - head
}

/// A pager for Display Data.
#[must_use]
pub fn pager() -> Pager {
    Pager::new(page_rows(1), DATA_COLS)
}

/// Display Data over `d` where `p` stands.
#[allow(clippy::many_single_char_names)] // d, p, s, w, i
#[must_use]
pub fn display(d: &Data, p: &Pager, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 35, "Display Data");
    let width = d.width();
    put(&mut s, 2, 47, Attr::Normal, "Data width . . . . . . :");
    let w = width.to_string();
    put(&mut s, 2, 79 - w.len(), Attr::Normal, &w);
    put(
        &mut s,
        3,
        2,
        Attr::Normal,
        &leader("Position to line", 2, 28),
    );
    var(&mut s, 3, 32, 10, "POSITION", "");
    put(
        &mut s,
        3,
        43,
        Attr::Normal,
        &leader("Shift to column", 43, 70),
    );
    var(&mut s, 3, 74, 7, "SHIFT", "");
    let cols = DATA_COLS.min(width.saturating_sub(p.shift - 1)).max(1);
    put(&mut s, 4, 2, Attr::HighIntensity, &scale(p.shift, cols));
    put(
        &mut s,
        HEAD_ROW,
        2,
        Attr::HighIntensity,
        &window(&d.heading(), p.shift, DATA_COLS),
    );
    let first = HEAD_ROW + 1;
    let rows = d.rows.len();
    for k in 0..p.page {
        let i = p.top + k;
        let row = first + k;
        if i < rows {
            put(
                &mut s,
                row,
                2,
                Attr::Normal,
                &window(&d.line(i), p.shift, DATA_COLS),
            );
        } else if i == rows {
            put(&mut s, row, 2, Attr::HighIntensity, END_OF_DATA);
        }
    }
    more(&mut s, 22, 77, p.at_bottom(rows));
    keys(
        &mut s,
        &[
            (23, 2, "F3=Exit"),
            (23, 15, "F12=Cancel"),
            (23, 31, "F19=Left"),
            (23, 45, "F20=Right"),
            (23, 60, "F21=Split"),
        ],
    );
    message(&mut s, msg);
    s.cursor = Some((3, 32));
    s
}

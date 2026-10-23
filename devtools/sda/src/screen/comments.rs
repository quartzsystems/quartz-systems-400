// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The comments editor (option 2, F15, option 2 on Work with Fields):
//! the comment lines in front of a record, field or the file, shown the
//! way SEU shows a member, each line's source columns 1-71 editable.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, message, put};

/// Lines on the display.
pub const LINES: usize = 16;

/// The list's name in the screen's targets.
pub const LIST: &str = "COMMENTS";

/// The editor over `lines` (source text, columns 1-80), with blank
/// lines after them up to the page; `what` names them on the right of
/// row 1 (`Record comments`, `File comments`, `Field comments`).
#[must_use]
pub fn editor(what: &str, name: &str, lines: &[String], msg: Option<&str>) -> Screen {
    let mut s = blank();
    put(&mut s, 1, 2, Attr::Normal, "Columns . . . :    1  71");
    put(&mut s, 1, 38, Attr::HighIntensity, "Edit");
    let wc = 80 - what.chars().count();
    put(&mut s, 1, wc, Attr::HighIntensity, what);
    put(&mut s, 2, 2, Attr::Normal, "SEU==>");
    let nc = 80 - name.chars().count();
    put(&mut s, 2, nc, Attr::HighIntensity, name);
    put(
        &mut s,
        3,
        2,
        Attr::HighIntensity,
        "FMT A* .....A*. 1 ...+... 2 ...+... 3 ...+... 4 ...+... 5 ...+... 6 ...+... 7",
    );
    put(
        &mut s,
        4,
        9,
        Attr::HighIntensity,
        "*************** Beginning of data *************************************",
    );
    let shown = lines.len().clamp(1, LINES);
    for i in 0..shown {
        let row = 5 + i;
        let seq = (i + 1) * 10;
        put(&mut s, row, 1, Attr::Normal, &format!("{:04}.{:02}", seq / 100, seq % 100));
        let text = lines.get(i).map_or_else(
            || format!("{:5}A*", ""),
            |l| l.chars().take(71).collect::<String>(),
        );
        input(
            &mut s,
            row,
            9,
            71,
            Target::ListCell {
                list: LIST.into(),
                row_index: i,
                var: "TEXT".into(),
            },
            &text,
        );
    }
    put(
        &mut s,
        5 + shown,
        9,
        Attr::HighIntensity,
        "****************** End of data ****************************************",
    );
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit"),
            (22, 12, "F4=Prompt"),
            (22, 24, "F5=Refresh"),
            (22, 37, "F9=Retrieve"),
            (22, 51, "F10=Cursor"),
            (22, 64, "F11=Toggle"),
            (23, 2, "F16=Repeat find"),
            (23, 24, "F17=Repeat change"),
            (23, 51, "F24=More keys"),
        ],
    );
    message(&mut s, msg);
    s.cursor = Some((5, 16));
    s
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The menu design displays after Design Menus: Specify Menu Functions,
//! Define Menu Commands and Save Menu. pub400 creates the member before
//! these show, so they have no golden: laid out by the rules of the
//! displays that have one.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{Values, blank, input, keys, leader, message, more, put, title, title_centered, var};

/// Specify Menu Functions: `IMAGE` (work with the image and commands)
/// and `HELP` (the help member).
#[must_use]
pub fn functions(menu: &str, v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title_centered(&mut s, "Specify Menu Functions");
    put(&mut s, 3, 2, Attr::Normal, "Menu . . . . :");
    put(&mut s, 3, 19, Attr::HighIntensity, menu);
    put(&mut s, 5, 2, Attr::Blue, "Type choices, press Enter.");
    put(&mut s, 7, 4, Attr::Normal, &leader("Work with menu image and commands", 4, 43));
    var(&mut s, 7, 47, 1, "IMAGE", &v("IMAGE"));
    put(&mut s, 7, 60, Attr::Normal, "Y=Yes");
    put(&mut s, 9, 4, Attr::Normal, &leader("Work with menu help", 4, 43));
    var(&mut s, 9, 47, 1, "HELP", &v("HELP"));
    put(&mut s, 9, 60, Attr::Normal, "Y=Yes");
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((7, 47));
    s
}

/// The list's name in the screen's targets.
pub const LIST: &str = "COMMANDS";
/// Options on a page.
pub const PAGE: usize = 12;

/// Define Menu Commands: each option with its command.
#[must_use]
pub fn commands(menu: &str, rows: &[(u8, String)], top: usize, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title_centered(&mut s, "Define Menu Commands");
    put(&mut s, 3, 2, Attr::Normal, "Menu . . . . :");
    put(&mut s, 3, 19, Attr::HighIntensity, menu);
    put(&mut s, 5, 2, Attr::Blue, "Type commands, press Enter.");
    put(&mut s, 7, 2, Attr::HighIntensity, "Option");
    put(&mut s, 7, 10, Attr::HighIntensity, "Command");
    if rows.is_empty() {
        put(&mut s, 9, 4, Attr::Normal, "(No options on the menu image)");
    }
    for (i, (o, c)) in rows.iter().enumerate().skip(top).take(PAGE) {
        let row = 8 + i - top;
        put(&mut s, row, 4, Attr::Normal, &format!("{o:>2}"));
        input(
            &mut s,
            row,
            10,
            70,
            Target::ListCell {
                list: LIST.into(),
                row_index: i,
                var: "CMD".into(),
            },
            c,
        );
    }
    more(&mut s, 21, top + PAGE >= rows.len());
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F4=Prompt"), (23, 24, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((8, 10));
    s
}

/// Save Menu - Create Menu: `SAVE`, `SRCFILE`, `SRCLIB`, `CREATE`,
/// `LIB`, `REPLACE`.
#[must_use]
pub fn save(v: Values<'_>, msg: Option<&str>) -> Screen {
    let mut s = blank();
    title(&mut s, 28, "Save Menu - Create Menu");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    let rows: &[(usize, usize, &str, usize, &str, &str)] = &[
        (5, 4, "Save menu source", 1, "SAVE", "Y=Yes"),
        (6, 6, "Source file", 10, "SRCFILE", "F4 for list"),
        (7, 8, "Library", 10, "SRCLIB", "Name, *LIBL ..."),
        (9, 4, "Create menu objects", 1, "CREATE", "Y=Yes"),
        (10, 6, "Library", 10, "LIB", "Name, *CURLIB"),
        (11, 6, "Replace existing objects", 1, "REPLACE", "Y=Yes"),
    ];
    for (row, col, label, len, name, hint) in rows {
        put(&mut s, *row, *col, Attr::Normal, &leader(label, *col, 43));
        var(&mut s, *row, 47, *len, name, &v(name));
        put(&mut s, *row, 60, Attr::Normal, hint);
    }
    keys(&mut s, &[(23, 2, "F3=Exit"), (23, 12, "F12=Cancel")]);
    message(&mut s, msg);
    s.cursor = Some((5, 47));
    s
}

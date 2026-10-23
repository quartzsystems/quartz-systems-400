// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRDFU`'s menu and the Run a DFU Program menu.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, bottom_line, input, keys, put};

/// The DFU menu's options.
pub const ITEMS: &[&str] = &[
    "Run a DFU program",
    "Create a DFU program",
    "Change a DFU program",
    "Delete a DFU program",
    "Update data using temporary program",
];

/// The run menu's options.
pub const RUN_ITEMS: &[&str] = &["Change a data file", "Display a data file"];

fn menu(title_col: usize, title: &str, items: &[&str], cmdline: &str, message: Option<&str>, copyright: bool) -> Screen {
    let mut s = blank();
    put(&mut s, 1, title_col, Attr::HighIntensity, title);
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    for (i, text) in items.iter().enumerate() {
        put(&mut s, 5 + i, 7, Attr::Normal, &format!("{}. {text}", i + 1));
    }
    put(&mut s, 20, 2, Attr::Normal, "Selection or command");
    put(&mut s, 21, 2, Attr::Normal, "===>");
    input(&mut s, 21, 7, 153, Target::CmdLine, cmdline);
    keys(
        &mut s,
        &[
            (23, 2, "F3=Exit"),
            (23, 12, "F4=Prompt"),
            (23, 24, "F9=Retrieve"),
            (23, 38, "F12=Cancel"),
        ],
    );
    bottom_line(&mut s, message, copyright);
    s.cursor = Some((21, 7));
    s
}

/// The DFU menu.
#[must_use]
pub fn strdfu(cmdline: &str, message: Option<&str>, copyright: bool) -> Screen {
    menu(29, "Data File Utility (DFU)", ITEMS, cmdline, message, copyright)
}

/// Run a DFU Program.
#[must_use]
pub fn run(cmdline: &str, message: Option<&str>) -> Screen {
    menu(32, "Run a DFU Program", RUN_ITEMS, cmdline, message, false)
}

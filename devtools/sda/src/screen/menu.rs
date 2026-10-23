// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRSDA`'s menu.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{COPYRIGHT, COPYRIGHT_COL, blank, input, keys, message, put, title};

/// The menu's options: number, row, text.
pub const ITEMS: &[(u8, usize, &str)] = &[
    (1, 5, "Design screens"),
    (2, 6, "Design menus"),
    (3, 7, "Test display files"),
];

/// The SDA menu with the command line's value; the copyright line on
/// the session's first display, else the message.
#[must_use]
pub fn strsda(cmdline: &str, msg: Option<&str>, copyright: bool) -> Screen {
    let mut s = blank();
    title(&mut s, 29, "Screen Design Aid (SDA)");
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    for (n, row, text) in ITEMS {
        put(&mut s, *row, 7, Attr::Normal, &format!("{n}. {text}"));
    }
    put(&mut s, 20, 2, Attr::Normal, "Selection or command");
    put(&mut s, 21, 2, Attr::Normal, "===>");
    input(&mut s, 21, 7, 73, Target::CmdLine, cmdline);
    keys(
        &mut s,
        &[
            (23, 2, "F1=Help"),
            (23, 12, "F3=Exit"),
            (23, 22, "F4=Prompt"),
            (23, 34, "F9=Retrieve"),
            (23, 48, "F12=Cancel"),
        ],
    );
    match msg {
        Some(_) => message(&mut s, msg),
        None if copyright => put(&mut s, 24, COPYRIGHT_COL, Attr::HighIntensity, COPYRIGHT),
        None => {}
    }
    s.cursor = Some((21, 7));
    s
}

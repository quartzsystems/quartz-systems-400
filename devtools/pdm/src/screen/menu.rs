// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRPDM`'s menu and its help.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, bottom_line, input, keys, more, put, title};

/// The menu's options: number, row, text.
pub const ITEMS: &[(u8, usize, &str)] = &[
    (1, 5, "Work with libraries"),
    (2, 6, "Work with objects"),
    (3, 7, "Work with members"),
    (9, 9, "Work with user-defined options"),
];

/// The PDM menu with the command line's value.
#[must_use]
pub fn strpdm(cmdline: &str, message: Option<&str>, copyright: bool) -> Screen {
    let mut s = blank();
    title(&mut s, 26, "Programming Development Manager (PDM)", None);
    put(&mut s, 3, 2, Attr::Blue, "Select one of the following:");
    for (n, row, text) in ITEMS {
        put(&mut s, *row, 7, Attr::Normal, &format!("{n}. {text}"));
    }
    put(&mut s, 19, 2, Attr::Normal, "Selection or command");
    put(&mut s, 20, 2, Attr::Normal, "===>");
    input(&mut s, 20, 7, 73, Target::CmdLine, cmdline);
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit"),
            (22, 16, "F4=Prompt"),
            (22, 32, "F9=Retrieve"),
            (22, 51, "F10=Command entry"),
            (23, 2, "F12=Cancel"),
            (23, 16, "F18=Change defaults"),
        ],
    );
    bottom_line(&mut s, message, copyright);
    s.cursor = Some((20, 7));
    s
}

/// A help display: `Help` and the title, the page's text from row 3,
/// the indicator on row 21 and the help keys.
#[must_use]
pub fn help(title_text: &str, lines: &[String], page: usize) -> Screen {
    const PER: usize = 17;
    let mut s = blank();
    put(&mut s, 1, 2, Attr::Blue, "Help");
    let col = 1 + (80usize.saturating_sub(title_text.len())) / 2;
    put(&mut s, 1, col, Attr::HighIntensity, title_text);
    let start = page * PER;
    for (i, l) in lines.iter().skip(start).take(PER).enumerate() {
        let (c, a, t) = match l.strip_prefix('#') {
            Some(h) => (2, Attr::HighIntensity, h),
            None => (6, Attr::Normal, l.as_str()),
        };
        put(&mut s, 3 + i, c, a, t);
    }
    more(&mut s, 21, start + PER >= lines.len());
    keys(
        &mut s,
        &[
            (22, 2, "F3=Exit help"),
            (22, 17, "F10=Move to top"),
            (22, 35, "F11=Search Index"),
            (22, 54, "F12=Cancel"),
            (23, 2, "F13=Information Assistant"),
            (23, 35, "F14=Print help"),
        ],
    );
    s
}

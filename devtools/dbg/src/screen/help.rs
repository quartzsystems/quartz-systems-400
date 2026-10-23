// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help for the debugger: the `HELP` command and F1 on Display Module
//! Source. The text is Quartz's: one line a command.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, more, put};

/// Lines a help page shows.
pub const PAGE: usize = 17;

/// What each debug command does.
pub const COMMANDS: &[(&str, &str)] = &[
    ("ATTR", "Show the type and length of a variable."),
    ("BREAK", "Stop at a line; WHEN adds a condition."),
    ("CLEAR", "Remove a breakpoint, every one (PGM) or watches."),
    ("DISPLAY", "Show breakpoints, equates, watches or a module."),
    ("EQUATE", "Name a command or expression for reuse."),
    (
        "EVAL",
        "Show or assign a variable; :X and :C pick a format.",
    ),
    ("FIND", "Look for text or go to a line number."),
    ("HELP", "This list."),
    ("QUAL", "Look up variables as seen from a line."),
    ("SET", "Session options: UPDPROD, CASE, OPMSRC."),
    ("STEP", "Run N statements, OVER or INTO calls."),
    ("TBREAK", "A breakpoint for the current thread only."),
    ("THREAD", "Work with the threads being debugged."),
    ("WATCH", "Stop when a variable changes."),
    ("TOP, BOTTOM", "Go to the first or last page."),
    ("UP, DOWN", "Move by a number of lines, or MAX."),
    ("LEFT, RIGHT", "Shift by a number of columns, or MAX."),
    ("NEXT, PREVIOUS", "Go to the next or previous breakpoint."),
];

/// The help lines: a heading, then each command.
#[must_use]
pub fn lines() -> Vec<String> {
    let mut v = vec!["#Debug commands".to_string(), String::new()];
    v.extend(COMMANDS.iter().map(|(c, t)| format!("{c:<16}{t}")));
    v
}

/// The keys of help reached by F1 (`extended`) or by `HELP`.
fn help_keys(s: &mut Screen, extended: bool) {
    if extended {
        keys(
            s,
            &[
                (22, 2, "F2=Extended help"),
                (22, 21, "F3=Exit help"),
                (22, 36, "F10=Move to top"),
                (22, 54, "F11=Search Index"),
                (23, 2, "F12=Cancel"),
                (23, 21, "F13=Information Assistant"),
                (23, 54, "F14=Print help"),
            ],
        );
    } else {
        keys(
            s,
            &[
                (22, 2, "F3=Exit help"),
                (22, 17, "F10=Move to top"),
                (22, 35, "F12=Cancel"),
                (22, 48, "F13=Information Assistant"),
                (23, 2, "F14=Print help"),
            ],
        );
    }
}

/// One page of a help display: F1's (`extended`, with F2 and F11) or
/// the `HELP` command's.
#[must_use]
pub fn help(title_text: &str, lines: &[String], page: usize, extended: bool) -> Screen {
    let mut s = blank();
    put(&mut s, 1, 2, Attr::Blue, "Help");
    let col = 1 + (80usize.saturating_sub(title_text.len())) / 2;
    put(&mut s, 1, col, Attr::HighIntensity, title_text);
    let start = page * PAGE;
    for (i, l) in lines.iter().skip(start).take(PAGE).enumerate() {
        let (c, a, t) = match l.strip_prefix('#') {
            Some(h) => (2, Attr::HighIntensity, h),
            None => (6, Attr::Normal, l.as_str()),
        };
        put(&mut s, 3 + i, c, a, t);
    }
    more(&mut s, 21, start + PAGE >= lines.len());
    help_keys(&mut s, extended);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages() {
        let l = lines();
        let s = help("Display Module Source - Help", &l, 0, true);
        let t = s.text_rows();
        assert!(t[0].contains("Display Module Source - Help"));
        assert!(t[20].contains("More..."));
        let t = help("Display Module Source - Help", &l, 1, false).text_rows();
        assert!(t[20].contains("Bottom"));
    }
}

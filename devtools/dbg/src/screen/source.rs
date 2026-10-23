// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display Module Source: the program, library and module on row 3, the
//! source from row 4 (line numbers, breakpoint lines bright, the line
//! the program stopped at in reverse image), `Bottom`/`More...` on row
//! 19, the `Debug` command line on rows 20 and 21, three pages of
//! function keys on rows 22 and 23 and the message line. Positions are
//! the pub400 goldens'; the source rows are laid out by the same rules
//! (pub400 shows no program with a source view to this account).

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, message, more, put, title};
use crate::view::SourceView;

/// The first source row.
pub const FIRST_ROW: usize = 4;
/// Source rows on the display.
pub const PAGE: usize = 15;
/// The column the source text starts in.
pub const TEXT_COL: usize = 10;
/// Columns of source a row shows.
pub const WIDTH: usize = 70;
/// The debug command line: row, column and length (two rows).
pub const CMD: (usize, usize, usize) = (20, 16, 144);

/// The function keys, three pages.
pub const KEYS: [&[(usize, usize, &str)]; 3] = [
    &[
        (22, 2, "F3=End program"),
        (22, 19, "F6=Add/Clear breakpoint"),
        (22, 45, "F10=Step"),
        (22, 56, "F11=Display variable"),
        (23, 2, "F12=Resume"),
        (23, 19, "F17=Watch variable"),
        (23, 40, "F18=Work with watch"),
        (23, 62, "F24=More keys"),
    ],
    &[
        (22, 2, "F5=Refresh"),
        (22, 20, "F9=Retrieve"),
        (22, 34, "F14=Work with module list"),
        (22, 62, "F15=Select view"),
        (23, 2, "F16=Repeat find"),
        (23, 20, "F21=Command entry"),
        (23, 40, "F22=Step into"),
        (23, 62, "F24=More keys"),
    ],
    &[
        (22, 2, "F13=Work with module breakpoints"),
        (22, 37, "F19=Left"),
        (22, 48, "F20=Right"),
        (22, 60, "F23=Display output"),
        (23, 2, "F24=More keys"),
    ],
];

/// What the display shows.
#[derive(Clone, Debug, Default)]
pub struct SourceScreen<'a> {
    /// Program name.
    pub program: &'a str,
    /// Its library.
    pub library: &'a str,
    /// The module.
    pub module: &'a str,
    /// The view; `None` or an empty one shows "source not available".
    pub view: Option<&'a SourceView>,
    /// Breakpoint lines of the module.
    pub breakpoints: &'a [usize],
    /// The key page, 0 to 2.
    pub keys: usize,
    /// The debug command line's value.
    pub cmdline: &'a str,
    /// The message line.
    pub message: Option<&'a str>,
}

/// The display.
#[must_use]
pub fn dspmodsrc(d: &SourceScreen<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, "Display Module Source");
    put(&mut s, 3, 2, Attr::Normal, "Program:");
    put(&mut s, 3, 13, Attr::Normal, d.program);
    put(&mut s, 3, 28, Attr::Normal, "Library:");
    put(&mut s, 3, 39, Attr::Normal, d.library);
    put(&mut s, 3, 54, Attr::Normal, "Module:");
    put(&mut s, 3, 64, Attr::Normal, d.module);
    match d.view.filter(|v| !v.is_empty()) {
        None => {
            put(&mut s, 5, 4, Attr::Normal, "(Source not available.)");
            more(&mut s, 19, true);
        }
        Some(v) => {
            for (i, (n, text)) in v.page_lines().into_iter().enumerate() {
                let row = FIRST_ROW + i;
                let current = v.current == Some(n);
                let bp = d.breakpoints.contains(&n);
                let num_attr = if current {
                    Attr::Reverse
                } else if bp {
                    Attr::HighIntensity
                } else {
                    Attr::Normal
                };
                put(&mut s, row, 2, num_attr, &format!("{n:>6}"));
                let text_attr = if current { Attr::Reverse } else { Attr::Normal };
                put(&mut s, row, TEXT_COL, text_attr, &text);
            }
            more(&mut s, 19, v.at_bottom());
        }
    }
    put(&mut s, CMD.0, 2, Attr::Normal, "Debug . . .");
    input(&mut s, CMD.0, CMD.1, CMD.2, Target::CmdLine, d.cmdline);
    keys(&mut s, KEYS[d.keys % KEYS.len()]);
    message(&mut s, d.message);
    s.cursor = Some((CMD.0, CMD.1));
    s
}

/// The source line on a display row, if the row shows one.
#[must_use]
pub fn line_at(view: &SourceView, row: usize) -> Option<usize> {
    if !(FIRST_ROW..FIRST_ROW + PAGE).contains(&row) {
        return None;
    }
    let n = view.top + (row - FIRST_ROW);
    (n <= view.len()).then_some(n)
}

/// The word under the cursor on a source row (F11, F17): letters,
/// digits and `& _ # @ $ .` around column `col`.
#[must_use]
pub fn word_at(view: &SourceView, row: usize, col: usize) -> Option<String> {
    let n = line_at(view, row)?;
    if col < TEXT_COL {
        return None;
    }
    let chars: Vec<char> = view.lines[n - 1].chars().collect();
    let at = view.left + (col - TEXT_COL);
    let part = |c: char| c.is_ascii_alphanumeric() || "&_#@$.".contains(c);
    if !chars.get(at).is_some_and(|c| part(*c)) {
        return None;
    }
    let mut a = at;
    while a > 0 && part(chars[a - 1]) {
        a -= 1;
    }
    let mut b = at;
    while b + 1 < chars.len() && part(chars[b + 1]) {
        b += 1;
    }
    Some(
        chars[a..=b]
            .iter()
            .collect::<String>()
            .trim_end_matches('.')
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_rows() {
        let mut v = SourceView::new(
            "QGPL/P",
            "P",
            vec![
                "PGM".into(),
                "DCL VAR(&X) TYPE(*DEC)".into(),
                "CHGVAR &X (&X + 1)".into(),
            ],
            PAGE,
            WIDTH,
        );
        v.stop_at(3);
        let s = dspmodsrc(&SourceScreen {
            program: "P",
            library: "QGPL",
            module: "P",
            view: Some(&v),
            breakpoints: &[2],
            ..SourceScreen::default()
        });
        let rows = s.text_rows();
        assert!(rows[3].starts_with("      1  PGM"));
        assert!(rows[5].contains("CHGVAR"));
        assert!(rows[18].contains("Bottom"));
        assert_eq!(line_at(&v, 5), Some(2));
        assert_eq!(line_at(&v, 9), None);
        assert_eq!(word_at(&v, 5, 18).as_deref(), Some("&X"));
        assert_eq!(word_at(&v, 4, 2), None);
    }
}

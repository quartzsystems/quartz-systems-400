// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RLU's displays: Design Report (the page image under a ruler, a
//! prefix area for line commands, the `RLU==>` command line) and Exit
//! RLU.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, leader, out, put, var};

/// Rows of the image on the display.
pub const IMAGE_ROWS: usize = 17;
/// Image columns on the display.
pub const IMAGE_COLS: usize = 71;
/// The display column of image column 1.
pub const DATA_COL: usize = 9;

/// One line of the image as the display shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLine {
    /// The prefix area: the record format's name.
    pub prefix: String,
    /// The characters from the first column shown.
    pub text: String,
    /// The operator may type on it (else a banner).
    pub editable: bool,
}

/// What Design Report shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Design {
    /// The source file's library.
    pub lib: String,
    /// The source file.
    pub file: String,
    /// The member.
    pub member: String,
    /// The first image column shown.
    pub from: usize,
    /// The command line.
    pub cmdline: String,
    /// The lines shown, top first.
    pub lines: Vec<ImageLine>,
    /// Which set of function keys (`F24`).
    pub key_set: usize,
    /// The message line.
    pub message: Option<String>,
}

/// The ruler from an image column: digits at tens, `+` at fives.
#[must_use]
pub fn ruler(from: usize, n: usize) -> String {
    (from..from + n)
        .map(|c| match c % 10 {
            0 => char::from_digit(u32::try_from((c / 10) % 10).unwrap_or(0), 10).unwrap_or('0'),
            5 => '+',
            1 | 9 => ' ',
            _ => '.',
        })
        .collect()
}

/// A banner line: dashes around the text, as wide as the image shown.
#[must_use]
pub fn banner(text: &str) -> String {
    let dashes = IMAGE_COLS - text.chars().count() - 2;
    let left = (dashes / 2).saturating_sub(1);
    format!(
        "{} {text} {}",
        "-".repeat(left),
        "-".repeat(dashes - left)
    )
}

/// The key sets `F24` steps through.
const KEYS: [[(usize, usize, &str); 5]; 3] = [
    [
        (22, 2, "F3=Exit"),
        (22, 12, "F11=Define field"),
        (22, 31, "F16=Delete field"),
        (23, 2, "F22=Alternative keys"),
        (23, 31, "F24=More keys"),
    ],
    [
        (22, 2, "F13=Mark/Unmark data"),
        (22, 25, "F14=Copy marked data"),
        (23, 2, "F15=Move marked data"),
        (23, 25, "F24=More keys"),
        (0, 0, ""),
    ],
    [
        (22, 2, "F17=File keywords"),
        (22, 23, "F18=Record keywords"),
        (23, 2, "F23=Field keywords"),
        (23, 23, "F24=More keys"),
        (0, 0, ""),
    ],
];

/// Design Report.
#[must_use]
pub fn design(d: &Design) -> Screen {
    let mut s = blank();
    out(&mut s, 1, 2, "Columns . . . :");
    out(
        &mut s,
        1,
        17,
        &format!("{:>5} {:>3}", d.from, d.from + IMAGE_COLS - 1),
    );
    put(&mut s, 1, 34, Attr::HighIntensity, "Design Report");
    let qual = format!("{}/{}", d.lib, d.file);
    out(&mut s, 1, 80 - qual.len().min(40), &qual);
    out(&mut s, 2, 2, "RLU==>");
    input(&mut s, 2, 9, 60, Target::CmdLine, &d.cmdline);
    out(&mut s, 2, 80 - d.member.len().min(10), &d.member);
    put(&mut s, 3, 2, Attr::HighIntensity, "BASE");
    out(&mut s, 3, DATA_COL, &ruler(d.from, IMAGE_COLS));
    for (i, l) in d.lines.iter().take(IMAGE_ROWS).enumerate() {
        let row = 4 + i;
        let prefix: String = l.prefix.chars().take(7).collect();
        if l.editable {
            var(&mut s, row, 1, 7, &format!("P{i}"), &prefix);
            var(&mut s, row, DATA_COL, IMAGE_COLS, &format!("L{i}"), &l.text);
        } else {
            out(&mut s, row, 1, &prefix);
            out(&mut s, row, DATA_COL, &l.text);
        }
    }
    let set = d.key_set % KEYS.len();
    let ks: Vec<(usize, usize, &str)> = KEYS[set].iter().copied().filter(|k| k.0 > 0).collect();
    keys(&mut s, &ks);
    if let Some(m) = &d.message {
        put(&mut s, 24, 2, Attr::HighIntensity, m);
    }
    s.cursor = Some((2, 9));
    s
}

/// Exit RLU's values, by field: `OPTION`, `MBR`, `FILE`, `LIB`,
/// `TEXT`, `CRTPRTF`, `CRTDFT`, `PROTO`, `PROTODFT`, `BATCH`, `JOBD`,
/// `JOBDLIB`.
#[must_use]
pub fn exit(value: &dyn Fn(&str) -> String, message: Option<&str>) -> Screen {
    let mut s = blank();
    put(&mut s, 1, 37, Attr::HighIntensity, "Exit RLU");
    put(&mut s, 3, 2, Attr::Blue, "Type choices, press Enter.");
    let rows: [(usize, bool, &str, &str, usize, &str); 12] = [
        (5, false, "Option", "OPTION", 1, "1=Save and exit"),
        (8, true, "Member", "MBR", 10, "Name"),
        (9, true, "File", "FILE", 10, "Name"),
        (10, true, "Library", "LIB", 10, "Name"),
        (11, true, "Text", "TEXT", 50, ""),
        (13, false, "Create printer file", "CRTPRTF", 1, "Y=Yes, N=No"),
        (14, true, "Change defaults", "CRTDFT", 1, "Y=Yes, N=No"),
        (15, false, "Prototype report", "PROTO", 1, "Y=Yes, N=No"),
        (16, true, "Change defaults", "PROTODFT", 1, "Y=Yes, N=No"),
        (17, false, "Submit to batch", "BATCH", 1, "Y=Yes, N=No"),
        (18, true, "Job description", "JOBD", 10, "Name, *USRPRF, *RLU"),
        (19, true, "Library", "JOBDLIB", 10, "Name, *LIBL, *CURLIB"),
    ];
    for (row, sub, label, key, len, legend) in rows {
        let col = if sub { 6 } else { 4 };
        out(&mut s, row, col, &leader(label, col, 38));
        var(&mut s, row, if sub { 44 } else { 42 }, len, key, &value(key));
        out(&mut s, row, 57, legend);
    }
    out(&mut s, 6, 57, "2=Exit without saving");
    out(&mut s, 7, 57, "3=Resume");
    keys(&mut s, &[(23, 2, "F5=Refresh"), (23, 15, "F12=Cancel")]);
    if let Some(m) = message {
        put(&mut s, 24, 2, Attr::HighIntensity, m);
    }
    s.cursor = Some((5, 42));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rulers_and_banners() {
        assert_eq!(&ruler(1, 20), " ...+... 1 ...+... 2");
        assert_eq!(banner("Start of Page 001").len(), IMAGE_COLS);
        assert!(banner("Start of Page 001").starts_with(&format!("{} S", "-".repeat(25))));
        assert!(banner("End of Report").starts_with(&format!("{} E", "-".repeat(27))));
    }
}

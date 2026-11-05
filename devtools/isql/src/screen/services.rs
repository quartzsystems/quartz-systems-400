// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays behind Interactive SQL Session Services' options 2 to
//! 4: Change Printer File (print the session), Confirm Remove Entries
//! Request and Change Source File (save the session in a source
//! member), placed as pub400 places them
//! (`tools/conformance/golden/sql/strsql/`). Every word is a
//! `QSTXTMSG` text.

use uim::Screen;
use uim::render::Attr;

use super::{blank, keys, leader, message, put, title, var};
use crate::print::Words;

/// The `QSTXTMSG` identifiers of these displays.
pub mod txt {
    /// Change Printer File's title.
    pub const PRINTER_TITLE: &str = "TXT0910";
    /// `Type choice, press Enter.`
    pub const TYPE_CHOICE: &str = "TXT0911";
    /// `Printer file`.
    pub const PRINTER_FILE: &str = "TXT0912";
    /// `F3=Exit`.
    pub const F3: &str = "TXT0913";
    /// `F5=Refresh`.
    pub const F5: &str = "TXT0914";
    /// `F12=Cancel`.
    pub const F12: &str = "TXT0915";
    /// Change Source File's title.
    pub const SOURCE_TITLE: &str = "TXT0916";
    /// `Type choices, press Enter.`
    pub const TYPE_CHOICES: &str = "TXT0917";
    /// `File`.
    pub const FILE: &str = "TXT0918";
    /// `Library`.
    pub const LIBRARY: &str = "TXT0919";
    /// `Member`.
    pub const MEMBER: &str = "TXT091A";
    /// The member's choices.
    pub const MEMBER_CHOICES: &str = "TXT091B";
    /// `Option`.
    pub const OPTION: &str = "TXT091C";
    /// The options, 1 to 5.
    pub const OPTIONS: [&str; 5] = ["TXT091D", "TXT091E", "TXT091F", "TXT0920", "TXT0921"];
    /// `For a new file:`.
    pub const NEW_FILE: &str = "TXT0922";
    /// `Authority`.
    pub const AUTHORITY: &str = "TXT0923";
    /// The authority's choices, two lines.
    pub const AUTHORITY_CHOICES: [&str; 2] = ["TXT0924", "TXT0925"];
    /// `Text`.
    pub const TEXT: &str = "TXT0926";
    /// Confirm Remove Entries Request's title.
    pub const CONFIRM_TITLE: &str = "TXT0927";
    /// What removing does, three lines.
    pub const CONFIRM_LINES: [&str; 3] = ["TXT0928", "TXT0929", "TXT092C"];
    /// Enter confirms.
    pub const CONFIRM_ENTER: &str = "TXT092A";
    /// F12 goes back.
    pub const CONFIRM_F12: &str = "TXT092B";
    /// `Name`.
    pub const NAME: &str = "TXT0935";
}

/// Change Printer File with the printer file as typed.
#[must_use]
pub fn printer_file(file: &str, msg: Option<&str>, w: &Words<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 31, &w(txt::PRINTER_TITLE));
    put(&mut s, 3, 2, Attr::Blue, &w(txt::TYPE_CHOICE));
    put(&mut s, 5, 4, Attr::Normal, &leader(&w(txt::PRINTER_FILE), 4, 35));
    var(&mut s, 5, 39, 10, "PRTF", file);
    put(&mut s, 5, 54, Attr::Normal, &w(txt::NAME));
    std_keys(&mut s, w);
    message(&mut s, msg);
    s.cursor = Some((5, 39));
    s
}

/// What Change Source File holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    /// The file.
    pub file: String,
    /// Its library.
    pub lib: String,
    /// The member: a name, `*FILE` or `*FIRST`.
    pub member: String,
    /// 1 to 5.
    pub option: String,
    /// A new file's authority.
    pub aut: String,
    /// A new file's text.
    pub text: String,
}

impl SourceFile {
    /// What the display starts with: `QSQLSESS` in library `lib`.
    #[must_use]
    pub fn new(lib: &str) -> Self {
        Self {
            file: "QSQLSESS".into(),
            lib: lib.into(),
            member: "*FILE".into(),
            option: "1".into(),
            aut: "*EXCLUDE".into(),
            text: String::new(),
        }
    }
}

/// Change Source File (save the session).
#[must_use]
pub fn source_file(v: &SourceFile, msg: Option<&str>, w: &Words<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 32, &w(txt::SOURCE_TITLE));
    put(&mut s, 3, 2, Attr::Blue, &w(txt::TYPE_CHOICES));
    let rows: [(usize, usize, &str, &str, &str, usize); 6] = [
        (5, 4, txt::FILE, "FILE", &v.file, 10),
        (6, 6, txt::LIBRARY, "LIB", &v.lib, 10),
        (7, 4, txt::MEMBER, "MBR", &v.member, 10),
        (9, 4, txt::OPTION, "OPTION", &v.option, 1),
        (16, 6, txt::AUTHORITY, "AUT", &v.aut, 10),
        (20, 6, txt::TEXT, "TEXT", &v.text, 50),
    ];
    for (row, col, label, name, value, len) in rows {
        put(&mut s, row, col, Attr::Normal, &leader(&w(label), col, 25));
        var(&mut s, row, 29, len, name, value);
    }
    put(&mut s, 5, 42, Attr::Normal, &w(txt::NAME));
    put(&mut s, 6, 42, Attr::Normal, &w(txt::NAME));
    put(&mut s, 7, 42, Attr::Normal, &w(txt::MEMBER_CHOICES));
    for (i, o) in txt::OPTIONS.iter().enumerate() {
        put(&mut s, 9 + i, 42, Attr::Normal, &w(o));
    }
    put(&mut s, 15, 4, Attr::Normal, &w(txt::NEW_FILE));
    for (i, a) in txt::AUTHORITY_CHOICES.iter().enumerate() {
        put(&mut s, 16 + i, 42, Attr::Normal, &w(a));
    }
    std_keys(&mut s, w);
    message(&mut s, msg);
    s.cursor = Some((5, 29));
    s
}

/// Confirm Remove Entries Request.
#[must_use]
pub fn confirm_remove(w: &Words<'_>) -> Screen {
    let mut s = blank();
    title(&mut s, 26, &w(txt::CONFIRM_TITLE));
    for (i, l) in txt::CONFIRM_LINES.iter().enumerate() {
        put(&mut s, 3 + i, 2, Attr::Normal, &w(l));
    }
    put(&mut s, 7, 2, Attr::Normal, &w(txt::CONFIRM_ENTER));
    put(&mut s, 8, 2, Attr::Normal, &w(txt::CONFIRM_F12));
    let f12 = w(txt::F12);
    keys(&mut s, &[(23, 2, f12.as_str())]);
    s
}

/// F3, F5 and F12 on row 23.
fn std_keys(s: &mut Screen, w: &Words<'_>) {
    let (f3, f5, f12) = (w(txt::F3), w(txt::F5), w(txt::F12));
    keys(s, &[(23, 2, f3.as_str()), (23, 14, f5.as_str()), (23, 29, f12.as_str())]);
}

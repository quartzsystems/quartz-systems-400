// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printed page: the heading, the overflow line and how a record
//! that does not fit moves to the next page.

/// The overflow line: a record whose spacing would carry it past this
/// line starts a new page.
const OVERFLOW: usize = 60;

/// Lines a forms control character advances.
fn advance(cc: char) -> usize {
    match cc {
        '0' => 2,
        '-' => 3,
        _ => 1,
    }
}

/// One more line of space: the first record after an overflow
/// heading outside the source section.
fn more(cc: char) -> char {
    match cc {
        ' ' => '0',
        _ => '-',
    }
}

/// The listing being printed.
pub(crate) struct Out {
    /// The records.
    pub lines: Vec<String>,
    page: u32,
    /// The line last printed on.
    pos: usize,
    title: String,
    /// The column rulers the source section repeats under each heading.
    rulers: Option<(String, String)>,
}

impl Out {
    pub fn new(title: String) -> Self {
        Self {
            lines: Vec::new(),
            page: 0,
            pos: 0,
            title,
            rulers: None,
        }
    }

    fn heading(&mut self) {
        self.page += 1;
        self.lines.push(format!(
            "1{:<64}{:<26}Page {:>5}",
            format!(" 5770WDS {}  QS/400  RN        QS/400 ILE RPG", release::RELEASE),
            self.title,
            self.page
        ));
        self.pos = 1;
        if let Some((a, b)) = self.rulers.clone() {
            self.lines.push(format!("0{a}"));
            self.lines.push(format!(" {b}"));
            self.pos += 3;
        }
    }

    /// Whether the last record reached the overflow line.
    fn overflowed(&self) -> bool {
        self.pos >= OVERFLOW
    }

    /// A record.
    pub fn line(&mut self, cc: char, text: impl AsRef<str>) {
        let mut cc = cc;
        if self.pos + advance(cc) > OVERFLOW {
            self.heading();
            cc = if self.rulers.is_some() { ' ' } else { more(cc) };
        }
        self.put(cc, text.as_ref());
    }

    /// A section's end line: it may pass the overflow line.
    pub fn end_line(&mut self, cc: char, text: impl AsRef<str>) {
        let mut cc = cc;
        if self.overflowed() {
            self.heading();
            if self.rulers.is_none() {
                cc = more(cc);
            }
        }
        self.put(cc, text.as_ref());
    }

    fn put(&mut self, cc: char, text: &str) {
        self.pos += advance(cc);
        self.lines.push(format!("{cc}{text}").trim_end().to_string());
    }

    /// A new page for a section, with its rulers (the source section's);
    /// a page that overflowed first gets its overflow heading.
    pub fn new_page(&mut self, rulers: Option<(String, String)>) {
        if self.page > 0 && self.overflowed() {
            self.heading();
        }
        self.rulers = rulers;
        self.heading();
    }

    /// Change the rulers in the middle of a page: they print as two
    /// records.
    pub fn change_rulers(&mut self, rulers: (String, String)) {
        self.rulers = Some(rulers.clone());
        if self.pos + advance('-') > OVERFLOW {
            // The new page's heading carries them.
            self.heading();
            return;
        }
        self.put('-', &rulers.0);
        self.line(' ', &rulers.1);
    }

    /// Whether these are the rulers in force.
    pub fn rulers_are(&self, r: &(String, String)) -> bool {
        self.rulers.as_ref() == Some(r)
    }
}

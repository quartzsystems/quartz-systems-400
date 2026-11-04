// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CVTRPGSRC`: an RPG III member to ILE RPG fixed form, as IBM's
//! converter writes it. Each specification is cut with the RPG III
//! tables (`rpgspecs::rpg3_form`) and placed with the ILE RPG ones
//! (`rpgspecs::form`); the extension and line counter specifications
//! become definitions and file keywords, data structures and named
//! constants on input specifications become definitions, and every
//! output line remembers the record it came from.

mod calc;
mod defs;
mod file;
mod input;
mod output;
mod place;

use std::fmt;

pub use place::{factor, opcode};

/// One converted line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The ILE RPG record.
    pub text: String,
    /// The RPG III record it came from (0-based).
    pub from: usize,
}

/// A conversion message (`RNM`), on a source record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    /// The identifier.
    pub id: &'static str,
    /// Severity.
    pub severity: u8,
    /// The RPG III record (0-based).
    pub at: usize,
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:02} record {}", self.id, self.severity, self.at + 1)
    }
}

/// What the converter made of a member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversion {
    /// The converted member, in ILE RPG order.
    pub lines: Vec<Line>,
    /// Messages for the report.
    pub messages: Vec<Msg>,
    /// Records read, by kind: specifications, comments, compile-time data.
    pub counts: Counts,
}

/// Record counts for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Records read.
    pub records: usize,
    /// Specification records.
    pub specs: usize,
    /// Comment records (blank records too).
    pub comments: usize,
    /// Compile-time data records (and `**` lines).
    pub data: usize,
    /// Records written.
    pub written: usize,
}

impl Conversion {
    /// The converted member's text, one record per line.
    #[must_use]
    pub fn text(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.text.clone()).collect()
    }

    /// Whether a message above severity 10 stops a compile.
    #[must_use]
    pub fn severe(&self) -> bool {
        self.messages.iter().any(|m| m.severity > 10)
    }
}

/// The section a converted line belongs to, in ILE RPG order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sect {
    H,
    F,
    D,
    I,
    C,
    O,
    Data,
}

/// The converter's state: lines by section.
struct Out {
    lines: Vec<(Sect, Line)>,
    messages: Vec<Msg>,
    /// Comments waiting for the next specification's section.
    pending: Vec<Line>,
}

impl Out {
    fn push(&mut self, sect: Sect, text: &str, from: usize) {
        for c in std::mem::take(&mut self.pending) {
            self.lines.push((sect, c));
        }
        self.lines.push((sect, Line { text: text.trim_end().to_string(), from }));
    }

    fn msg(&mut self, id: &'static str, severity: u8, at: usize) {
        self.messages.push(Msg { id, severity, at });
    }
}

/// The letter of a record (column 6), upper case.
fn letter(r: &str) -> char {
    r.chars().nth(5).unwrap_or(' ').to_ascii_uppercase()
}

/// Whether a record is a comment: `*` in column 7, or blank.
fn is_comment(r: &str) -> bool {
    r.chars().nth(6) == Some('*') || r.get(5..).is_none_or(|t| t.trim().is_empty())
}

/// Convert an RPG III member.
#[must_use]
pub fn convert(records: &[String]) -> Conversion {
    let mut out = Out {
        lines: Vec::new(),
        messages: Vec::new(),
        pending: Vec::new(),
    };
    let mut counts = Counts {
        records: records.len(),
        ..Counts::default()
    };
    let mut files = file::Files::default();
    let mut defs = defs::Defs::default();
    let mut inp = input::Input::default();
    let mut data_from = records.len();
    // Line counter specifications first: their keywords go on the F specs.
    for r in records {
        if r.starts_with("**") {
            break;
        }
        if letter(r) == 'L' && !is_comment(r) {
            files.line_counter(r);
        }
    }
    let mut i = 0;
    while i < records.len() {
        let r = &records[i];
        if r.starts_with("**") {
            data_from = i;
            break;
        }
        if is_comment(r) {
            counts.comments += 1;
            out.pending.push(Line { text: r.trim_end().to_string(), from: i });
            i += 1;
            continue;
        }
        counts.specs += 1;
        match letter(r) {
            'H' => out.push(Sect::H, &file::control(r), i),
            'F' => {
                // The continuation lines after it are its keywords.
                let mut k = i + 1;
                while k < records.len() && letter(&records[k]) == 'F' && !is_comment(&records[k]) && file::is_continuation(&records[k]) {
                    k += 1;
                }
                counts.specs += k - i - 1;
                let lines = files.convert(&records[i..k], i, &mut out);
                for (text, from) in lines {
                    out.push(Sect::F, &text, from);
                }
                i = k;
                continue;
            }
            'E' => {
                for t in defs.extension(r, i, &mut out) {
                    out.push(Sect::D, &t, i);
                }
            }
            'L' => {
                // Taken into the file's keywords; the record is dropped.
                for c in std::mem::take(&mut out.pending) {
                    out.lines.push((Sect::F, c));
                }
            }
            'I' => inp.convert(r, i, &mut out, &mut defs),
            'C' => calc::convert(r, i, &mut out),
            'O' => output::convert(r, i, &mut out),
            _ => {
                out.msg("RNM0511", 30, i);
                out.push(Sect::C, &r.clone(), i);
            }
        }
        i += 1;
    }
    // Comments at the end stay at the end of the specifications.
    for c in std::mem::take(&mut out.pending) {
        out.lines.push((Sect::O, c));
    }
    for (k, r) in records.iter().enumerate().skip(data_from) {
        counts.data += 1;
        out.lines.push((Sect::Data, Line { text: r.trim_end().to_string(), from: k }));
    }
    // A stable sort keeps each section's order.
    let mut lines = out.lines;
    lines.sort_by_key(|(s, _)| *s);
    let lines: Vec<Line> = lines.into_iter().map(|(_, l)| l).collect();
    counts.written = lines.len();
    Conversion {
        lines,
        messages: out.messages,
        counts,
    }
}

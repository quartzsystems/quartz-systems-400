// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDS source: 80-column CP037 records with a sequence number and a
//! change date, as a source member holds them.

use crate::diag::{Diagnostic, ids};

/// Columns in a source record.
pub const RECORD_LEN: usize = 80;

/// One source record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    /// Sequence number times 100 (`SRCSEQ` is 6,2).
    pub seq: u32,
    /// Change date as `yymmdd`, 0 when unknown.
    pub date: u32,
    /// The 80 columns, CP037.
    pub text: [u8; RECORD_LEN],
}

impl SourceLine {
    /// The character in 1-based column `col`.
    #[must_use]
    pub fn ch(&self, col: usize) -> char {
        ccsid::char_of(self.text[col - 1])
    }

    /// Columns `from..=to`, 1-based, decoded.
    #[must_use]
    pub fn cols(&self, from: usize, to: usize) -> String {
        ccsid::from_ebcdic(&self.text[from - 1..to])
    }

    /// Columns `from..=to` as characters.
    #[must_use]
    pub fn chars(&self, from: usize, to: usize) -> Vec<char> {
        self.text[from - 1..to]
            .iter()
            .map(|&b| ccsid::char_of(b))
            .collect()
    }

    /// Whether columns `from..=to` are all blank.
    #[must_use]
    pub fn blank(&self, from: usize, to: usize) -> bool {
        self.text[from - 1..to].iter().all(|&b| b == ccsid::SPACE)
    }

    /// The record with trailing blanks dropped.
    #[must_use]
    pub fn as_text(&self) -> String {
        ccsid::decode_trimmed(&self.text)
    }

    /// The sequence number as `n.nn`.
    #[must_use]
    pub fn seq_text(&self) -> String {
        format!("{}.{:02}", self.seq / 100, self.seq % 100)
    }
}

/// The records of one source member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    lines: Vec<SourceLine>,
}

impl Source {
    /// From CP037 records as a source member holds them: sequence, date
    /// and data. Short records are blank padded, long ones cut at 80.
    pub fn from_records<'a>(records: impl IntoIterator<Item = (u32, u32, &'a [u8])>) -> Self {
        let lines = records
            .into_iter()
            .map(|(seq, date, data)| {
                let mut text = [ccsid::SPACE; RECORD_LEN];
                let n = data.len().min(RECORD_LEN);
                text[..n].copy_from_slice(&data[..n]);
                SourceLine { seq, date, text }
            })
            .collect();
        Self { lines }
    }

    /// From UTF-8 text on the development side: one record per line,
    /// numbered from 1.00. A character CP037 cannot hold becomes a blank
    /// with a warning; so does a tab.
    #[must_use]
    pub fn from_text(text: &str) -> (Self, Vec<Diagnostic>) {
        let mut diags = Vec::new();
        let mut lines = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let mut record = [ccsid::SPACE; RECORD_LEN];
            for (col, ch) in line.chars().take(RECORD_LEN).enumerate() {
                record[col] = if ch == '\t' {
                    diags.push(ids::tab(i, col + 1));
                    ccsid::SPACE
                } else {
                    ccsid::byte_of(ch).unwrap_or_else(|| {
                        diags.push(ids::unmappable(i, col + 1, ch));
                        ccsid::SPACE
                    })
                };
            }
            let seq = u32::try_from((i + 1) * 100).unwrap_or(u32::MAX);
            lines.push(SourceLine {
                seq,
                date: 0,
                text: record,
            });
        }
        (Self { lines }, diags)
    }

    /// The records.
    #[must_use]
    pub fn lines(&self) -> &[SourceLine] {
        &self.lines
    }

    /// Record count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether there are no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_becomes_records() {
        let (src, diags) = Source::from_text("     A          R REC\r\n     A  €\tX\n");
        assert_eq!(src.len(), 2);
        assert_eq!(src.lines()[0].seq_text(), "1.00");
        assert_eq!(src.lines()[0].ch(17), 'R');
        assert_eq!(src.lines()[0].cols(19, 28), "REC       ");
        assert!(src.lines()[0].blank(29, 80));
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].id, "DDS0110");
        assert_eq!(diags[1].id, "DDS0111");
        assert_eq!(src.lines()[1].as_text(), "     A    X");
    }

    #[test]
    fn records_are_padded_and_cut() {
        let long = [ccsid::byte_of('A').unwrap(); 90];
        let src = Source::from_records([(100, 260_101, &b"\xC1\xC2"[..]), (200, 0, &long[..])]);
        assert_eq!(src.lines()[0].as_text(), "AB");
        assert_eq!(src.lines()[0].date, 260_101);
        assert_eq!(src.lines()[1].as_text().len(), 80);
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fixed-column lexer: source records to specifications with their
//! keywords. A specification is a line with something in positions
//! 17-44 plus the following lines with those positions blank, which carry
//! more keywords. Lines before the first specification with 17-44 blank
//! are the file-level keywords.

pub mod columns;
pub mod keyword;

pub use keyword::{FunctionText, Keyword, Parm};

use crate::diag::{Diagnostics, ids};
use crate::source::{RECORD_LEN, Source, SourceLine};
use columns as c;

/// Position 17.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameType {
    /// `R`.
    Record,
    /// `K`.
    Key,
    /// `J`.
    Join,
    /// `S`.
    Select,
    /// `O`.
    Omit,
}

impl NameType {
    fn from_char(ch: char) -> Option<Self> {
        Some(match ch {
            'R' => Self::Record,
            'K' => Self::Key,
            'J' => Self::Join,
            'S' => Self::Select,
            'O' => Self::Omit,
            _ => return None,
        })
    }

    /// The letter in position 17.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Record => 'R',
            Self::Key => 'K',
            Self::Join => 'J',
            Self::Select => 'S',
            Self::Omit => 'O',
        }
    }

    /// What messages call the specification.
    #[must_use]
    pub fn what(self) -> &'static str {
        match self {
            Self::Record => "record format",
            Self::Key => "key",
            Self::Join => "join",
            Self::Select => "select",
            Self::Omit => "omit",
        }
    }
}

/// One specification with its keywords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// Source line index of the first line.
    pub line: usize,
    /// Sequence number of that line.
    pub seq: u32,
    /// True for the file-level keyword lines before the first entry.
    pub file_level: bool,
    /// Position 17.
    pub name_type: Option<NameType>,
    /// Positions 19-28, trimmed and uppercased.
    pub name: Option<String>,
    /// `R` in position 29.
    pub reference: bool,
    /// Positions 30-34.
    pub length: Option<u32>,
    /// Position 35.
    pub data_type: Option<char>,
    /// Positions 36-37.
    pub decimals: Option<u8>,
    /// Position 38.
    pub usage: Option<char>,
    /// Positions 8-16, trimmed.
    pub conditioning: String,
    /// Positions 39-44, trimmed.
    pub location: String,
    /// The keywords from every line of the entry.
    pub keywords: Vec<Keyword>,
}

impl Spec {
    fn file_level(line: usize, seq: u32) -> Self {
        Self {
            line,
            seq,
            file_level: true,
            name_type: None,
            name: None,
            reference: false,
            length: None,
            data_type: None,
            decimals: None,
            usage: None,
            conditioning: String::new(),
            location: String::new(),
            keywords: Vec::new(),
        }
    }

    /// The first keyword called `name`.
    #[must_use]
    pub fn keyword(&self, name: &str) -> Option<&Keyword> {
        self.keywords.iter().find(|k| k.name == name)
    }

    /// Whether positions 29-38 are blank.
    #[must_use]
    pub fn attributes_blank(&self) -> bool {
        !self.reference
            && self.length.is_none()
            && self.data_type.is_none()
            && self.decimals.is_none()
            && self.usage.is_none()
    }
}

/// Lex a whole source member.
pub fn lex(source: &Source, diags: &mut Diagnostics) -> Vec<Spec> {
    let mut lexer = Lexer {
        specs: Vec::new(),
        pending: None,
        diags,
    };
    for (i, line) in source.lines().iter().enumerate() {
        lexer.line(i, line);
    }
    lexer.finish();
    lexer.specs
}

struct Pending {
    spec: Spec,
    text: FunctionText,
    cont: Option<char>,
    last_line: usize,
}

struct Lexer<'a> {
    specs: Vec<Spec>,
    pending: Option<Pending>,
    diags: &'a mut Diagnostics,
}

impl Lexer<'_> {
    fn line(&mut self, i: usize, line: &SourceLine) {
        if line.blank(1, RECORD_LEN) {
            return;
        }
        let form = line.ch(c::FORM);
        if form != ' ' && form != 'A' {
            self.diags.push(ids::form_type(i, form));
            return;
        }
        if line.ch(c::COMMENT) == '*' {
            return;
        }
        if line.blank(c::NAME_TYPE, c::LOCATION.1) {
            if self.pending.is_none() {
                self.pending = Some(Pending::new(Spec::file_level(i, line.seq)));
            }
            self.append(i, line);
            return;
        }
        self.finish();
        let spec = self.spec(i, line);
        self.pending = Some(Pending::new(spec));
        self.append(i, line);
    }

    fn append(&mut self, i: usize, line: &SourceLine) {
        let p = self.pending.as_mut().expect("pending entry");
        p.last_line = i;
        p.text.append(line, i, &mut p.cont);
    }

    fn finish(&mut self) {
        if let Some(mut p) = self.pending.take() {
            if p.cont.is_some() {
                self.diags.push(ids::continuation_expected(p.last_line));
            }
            p.spec.keywords = keyword::parse(&p.text, self.diags);
            self.specs.push(p.spec);
        }
    }

    fn spec(&mut self, i: usize, line: &SourceLine) -> Spec {
        let name_type = match line.ch(c::NAME_TYPE) {
            ' ' => None,
            ch => {
                let t = NameType::from_char(ch);
                if t.is_none() {
                    self.diags.push(ids::bad_name_type(i, ch));
                }
                t
            }
        };
        let name = line.cols(c::NAME.0, c::NAME.1).trim().to_ascii_uppercase();
        let reference = match line.ch(c::REFERENCE) {
            ' ' => false,
            'R' => true,
            ch => {
                self.diags.push(ids::bad_reference_col(i, ch));
                false
            }
        };
        let length = self.number(i, line, c::LENGTH, |t| ids::bad_length(i, t));
        let decimals = self
            .number(i, line, c::DECIMALS, |t| ids::bad_decimals(i, t))
            .and_then(|d| u8::try_from(d).ok());
        let data_type = Some(line.ch(c::DATA_TYPE)).filter(|ch| *ch != ' ');
        let usage = Some(line.ch(c::USAGE)).filter(|ch| *ch != ' ');
        Spec {
            line: i,
            seq: line.seq,
            file_level: false,
            name_type,
            name: Some(name).filter(|n| !n.is_empty()),
            reference,
            length,
            data_type,
            decimals,
            usage,
            conditioning: line
                .cols(c::CONDITIONING.0, c::CONDITIONING.1)
                .trim()
                .to_string(),
            location: line.cols(c::LOCATION.0, c::LOCATION.1).trim().to_string(),
            keywords: Vec::new(),
        }
    }

    fn number(
        &mut self,
        _i: usize,
        line: &SourceLine,
        cols: (usize, usize),
        bad: impl Fn(&str) -> crate::diag::Diagnostic,
    ) -> Option<u32> {
        let text = line.cols(cols.0, cols.1);
        let t = text.trim();
        if t.is_empty() {
            return None;
        }
        let Ok(n) = t.parse() else {
            self.diags.push(bad(t));
            return None;
        };
        Some(n)
    }
}

impl Pending {
    fn new(spec: Spec) -> Self {
        let last_line = spec.line;
        Self {
            spec,
            text: FunctionText::default(),
            cont: None,
            last_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record from (1-based column, text) pairs.
    fn at(parts: &[(usize, &str)]) -> String {
        let mut s = vec![' '; RECORD_LEN];
        for (col, text) in parts {
            for (i, ch) in text.chars().enumerate() {
                s[col - 1 + i] = ch;
            }
        }
        s.into_iter().collect::<String>().trim_end().to_string()
    }

    fn lex_lines(lines: &[String]) -> (Vec<Spec>, Diagnostics) {
        let (src, d) = Source::from_text(&lines.join("\n"));
        assert!(d.is_empty());
        let mut diags = Diagnostics::default();
        let specs = lex(&src, &mut diags);
        (specs, diags)
    }

    #[test]
    fn entries_and_file_level() {
        let src = [
            at(&[(6, "A"), (7, "*  Customer master")]),
            at(&[(6, "A"), (45, "UNIQUE")]),
            at(&[(6, "A"), (45, "TEXT('Cust')")]),
            at(&[
                (6, "A"),
                (17, "R"),
                (19, "CUSREC"),
                (45, "TEXT('Customer record')"),
            ]),
            at(&[
                (6, "A"),
                (19, "CUSNO"),
                (34, "7"),
                (35, "P"),
                (37, "0"),
                (45, "COLHDG('Customer' +"),
            ]),
            at(&[(6, "A"), (52, "'Number')")]),
            at(&[
                (6, "A"),
                (19, "CUSNM"),
                (29, "R"),
                (45, "REFFLD(NAME QGPL/REFS)"),
            ]),
            at(&[
                (6, "A"),
                (19, "CUSTYP"),
                (34, "1"),
                (35, "A"),
                (38, "B"),
                (45, "DFT('A')"),
            ]),
            at(&[(6, "A"), (17, "K"), (19, "CUSNO"), (45, "DESCEND")]),
        ];
        let (specs, diags) = lex_lines(&src);
        assert!(diags.as_slice().is_empty(), "{:?}", diags.as_slice());
        assert_eq!(specs.len(), 6);
        assert!(specs[0].file_level);
        assert_eq!(specs[0].keywords.len(), 2);
        assert_eq!(specs[0].keywords[1].line, 2);
        let rec = &specs[1];
        assert_eq!(rec.name_type, Some(NameType::Record));
        assert_eq!(rec.name.as_deref(), Some("CUSREC"));
        assert!(rec.attributes_blank());
        let f = &specs[2];
        assert_eq!(f.name_type, None);
        assert_eq!(f.length, Some(7));
        assert_eq!(f.data_type, Some('P'));
        assert_eq!(f.decimals, Some(0));
        assert_eq!(f.keywords[0].quoted(1), Some("Number"));
        let r = &specs[3];
        assert!(r.reference);
        assert_eq!(r.length, None);
        assert_eq!(r.usage, None);
        assert_eq!(r.keywords[0].word(1), Some("QGPL/REFS"));
        assert_eq!(specs[4].usage, Some('B'));
        assert_eq!(specs[5].name_type, Some(NameType::Key));
        assert_eq!(specs[5].keywords[0].name, "DESCEND");
    }

    #[test]
    fn column_errors() {
        let src = [
            at(&[(6, "B"), (17, "R"), (19, "CUSREC")]),
            at(&[(6, "A"), (17, "X"), (19, "CUSREC")]),
            at(&[(6, "A"), (19, "CUSNO"), (33, "7X"), (35, "P"), (37, "0")]),
            at(&[(6, "A"), (19, "CUSNM"), (33, "10"), (35, "A"), (37, "Z")]),
            at(&[(6, "A"), (19, "CUSNX"), (29, "Q"), (33, "10"), (35, "A")]),
            at(&[(6, "A"), (45, "TEXT('x' +")]),
            at(&[(6, "A"), (19, "NEXT"), (34, "1"), (35, "A")]),
        ];
        let (specs, diags) = lex_lines(&src);
        let ids: Vec<_> = diags.as_slice().iter().map(|d| d.id).collect();
        assert_eq!(
            ids,
            [
                "DDS0101", "DDS0107", "DDS0105", "DDS0106", "DDS0109", "DDS0103", "DDS0203"
            ]
        );
        assert_eq!(specs.len(), 5);
        assert_eq!(specs[0].name_type, None);
        assert_eq!(specs[1].length, None);
        assert_eq!(specs[1].data_type, Some('P'));
        assert_eq!(specs[4].name.as_deref(), Some("NEXT"));
    }
}

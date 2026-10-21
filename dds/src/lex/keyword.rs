// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keyword grammar of positions 45-80: `KEYWORD`, `KEYWORD(parm
//! ...)`, quoted strings with `''`, `X'..'` hex literals, nested groups,
//! and `+`/`-` continuation across lines.

use std::fmt;

use crate::diag::{Diagnostics, ids};
use crate::lex::columns::FUNCTIONS;
use crate::source::SourceLine;

/// The name a constant (`'text'` in positions 45-80) is lexed under.
pub const CONSTANT: &str = "'";

/// One keyword parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parm {
    /// An unquoted word, uppercased: a name, number or `*SPECIAL` value.
    Word(String),
    /// A quoted string, as written.
    Quoted(String),
    /// `X'...'`.
    Hex(Vec<u8>),
    /// A parenthesized group.
    Group(Vec<Parm>),
}

impl Parm {
    /// The word, if it is one.
    #[must_use]
    pub fn as_word(&self) -> Option<&str> {
        match self {
            Self::Word(w) => Some(w),
            _ => None,
        }
    }

    /// The quoted text, if it is one.
    #[must_use]
    pub fn as_quoted(&self) -> Option<&str> {
        match self {
            Self::Quoted(q) => Some(q),
            _ => None,
        }
    }
}

impl fmt::Display for Parm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word(w) => f.write_str(w),
            Self::Quoted(q) => write!(f, "'{}'", q.replace('\'', "''")),
            Self::Hex(h) => {
                f.write_str("X'")?;
                for b in h {
                    write!(f, "{b:02X}")?;
                }
                f.write_str("'")
            }
            Self::Group(g) => {
                f.write_str("(")?;
                for (i, p) in g.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{p}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// One keyword with its parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyword {
    /// The name, uppercased.
    pub name: String,
    /// Parameters in order; empty for `KEYWORD` alone.
    pub parms: Vec<Parm>,
    /// Source line index where the keyword starts.
    pub line: usize,
}

impl Keyword {
    /// Parameter `i` as a word.
    #[must_use]
    pub fn word(&self, i: usize) -> Option<&str> {
        self.parms.get(i).and_then(Parm::as_word)
    }

    /// Parameter `i` as quoted text.
    #[must_use]
    pub fn quoted(&self, i: usize) -> Option<&str> {
        self.parms.get(i).and_then(Parm::as_quoted)
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.parms.is_empty() {
            f.write_str("(")?;
            for (i, p) in self.parms.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{p}")?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

/// The function columns of one entry joined across its lines, each
/// character tagged with the line it came from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionText {
    /// The characters.
    pub chars: Vec<char>,
    /// Source line index per character.
    pub lines: Vec<usize>,
}

impl FunctionText {
    /// Add positions 45-80 of `line`. `cont` carries a pending `+` or
    /// `-` from the previous line: `+` resumes at the first nonblank,
    /// `-` resumes at position 45 as is. Without one, a blank separates
    /// the lines' keywords.
    pub fn append(&mut self, line: &SourceLine, index: usize, cont: &mut Option<char>) {
        let seg = line.chars(FUNCTIONS.0, FUNCTIONS.1);
        let Some(last) = seg.iter().rposition(|c| *c != ' ') else {
            return;
        };
        let mut start = 0;
        let mut end = last + 1;
        match cont.take() {
            Some('+') => start = seg.iter().position(|c| *c != ' ').unwrap_or(0),
            None if !self.chars.is_empty() => self.push(' ', index),
            _ => {}
        }
        let tail = seg[end - 1];
        if tail == '+' || tail == '-' {
            *cont = Some(tail);
            end -= 1;
        }
        for &ch in &seg[start..end] {
            self.push(ch, index);
        }
    }

    fn push(&mut self, ch: char, line: usize) {
        self.chars.push(ch);
        self.lines.push(line);
    }

    fn line_at(&self, at: usize) -> usize {
        self.lines
            .get(at)
            .or(self.lines.last())
            .copied()
            .unwrap_or(0)
    }
}

/// Parse the keywords of one entry. Malformed keywords are reported and
/// skipped up to the next blank.
pub fn parse(text: &FunctionText, diags: &mut Diagnostics) -> Vec<Keyword> {
    Parser { text, at: 0, diags }.keywords()
}

struct Parser<'a> {
    text: &'a FunctionText,
    at: usize,
    diags: &'a mut Diagnostics,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text.chars.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.at += 1;
        Some(ch)
    }

    fn line(&self) -> usize {
        self.text.line_at(self.at)
    }

    fn skip_blanks(&mut self) {
        while self.peek() == Some(' ') {
            self.at += 1;
        }
    }

    fn skip_to_blank(&mut self) {
        while matches!(self.peek(), Some(c) if c != ' ') {
            self.at += 1;
        }
    }

    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if !keep(c) {
                break;
            }
            s.push(c.to_ascii_uppercase());
            self.at += 1;
        }
        s
    }

    fn keywords(&mut self) -> Vec<Keyword> {
        let mut out = Vec::new();
        loop {
            self.skip_blanks();
            let Some(ch) = self.peek() else {
                break;
            };
            let line = self.line();
            if ch == '\'' {
                // A constant (display and printer files).
                if let Ok(text) = self.quoted(CONSTANT) {
                    out.push(Keyword {
                        name: CONSTANT.into(),
                        parms: vec![Parm::Quoted(text)],
                        line,
                    });
                } else {
                    self.skip_to_blank();
                }
                continue;
            }
            if !ch.is_ascii_alphabetic() {
                self.diags.push(ids::keyword_expected(line, ch));
                self.skip_to_blank();
                continue;
            }
            let name = self.take_while(|c| c.is_ascii_alphanumeric());
            let parms = if self.peek() == Some('(') {
                let Ok(p) = self.list(&name) else {
                    self.skip_to_blank();
                    continue;
                };
                p
            } else {
                Vec::new()
            };
            if let Some(ch) = self.peek()
                && ch != ' '
            {
                self.diags
                    .push(ids::unexpected_after(self.line(), &name, ch));
                self.skip_to_blank();
                continue;
            }
            out.push(Keyword { name, parms, line });
        }
        out
    }

    fn list(&mut self, kw: &str) -> Result<Vec<Parm>, ()> {
        self.bump();
        let mut parms = Vec::new();
        loop {
            self.skip_blanks();
            match self.peek() {
                None => {
                    self.diags.push(ids::missing_paren(self.line(), kw));
                    return Err(());
                }
                Some(')') => {
                    self.bump();
                    return Ok(parms);
                }
                Some('\'') => parms.push(Parm::Quoted(self.quoted(kw)?)),
                Some('(') => parms.push(Parm::Group(self.list(kw)?)),
                Some(_) => parms.push(self.word(kw)?),
            }
        }
    }

    fn quoted(&mut self, kw: &str) -> Result<String, ()> {
        let line = self.line();
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                None => {
                    self.diags.push(ids::unterminated_quote(line, kw));
                    return Err(());
                }
                Some('\'') => {
                    if self.peek() == Some('\'') {
                        self.bump();
                        s.push('\'');
                    } else {
                        return Ok(s);
                    }
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn word(&mut self, kw: &str) -> Result<Parm, ()> {
        let w = self.take_while(|c| !matches!(c, ' ' | '(' | ')' | '\''));
        if w == "X" && self.peek() == Some('\'') {
            let line = self.line();
            let digits = self.quoted(kw)?;
            return decode_hex(&digits).map(Parm::Hex).ok_or_else(|| {
                self.diags.push(ids::bad_hex(line, &digits));
            });
        }
        Ok(Parm::Word(w))
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> FunctionText {
        FunctionText {
            chars: s.chars().collect(),
            lines: vec![0; s.chars().count()],
        }
    }

    fn parse_ok(s: &str) -> Vec<Keyword> {
        let mut d = Diagnostics::default();
        let k = parse(&text(s), &mut d);
        assert!(d.as_slice().is_empty(), "{:?}", d.as_slice());
        k
    }

    #[test]
    fn plain_and_parameterized() {
        let k = parse_ok("UNIQUE  colhdg('Cust' 'No.')  REFFLD(CUSNO QGPL/REFFILE)");
        assert_eq!(k.len(), 3);
        assert_eq!(k[0].name, "UNIQUE");
        assert!(k[0].parms.is_empty());
        assert_eq!(k[1].name, "COLHDG");
        assert_eq!(k[1].quoted(0), Some("Cust"));
        assert_eq!(k[1].quoted(1), Some("No."));
        assert_eq!(k[2].word(1), Some("QGPL/REFFILE"));
        assert_eq!(k[2].to_string(), "REFFLD(CUSNO QGPL/REFFILE)");
    }

    #[test]
    fn quotes_hex_and_groups() {
        let k = parse_ok("TEXT('It''s (fine)') DFT(X'C1C2') CHKMSGID(ABC (&FLD X))");
        assert_eq!(k[0].quoted(0), Some("It's (fine)"));
        assert_eq!(k[1].parms[0], Parm::Hex(vec![0xC1, 0xC2]));
        assert_eq!(
            k[2].parms[1],
            Parm::Group(vec![Parm::Word("&FLD".into()), Parm::Word("X".into())])
        );
        assert_eq!(k[0].to_string(), "TEXT('It''s (fine)')");
    }

    #[test]
    fn errors_are_reported_and_skipped() {
        let mut d = Diagnostics::default();
        let k = parse(&text("TEXT('open  UNIQUE"), &mut d);
        assert!(k.is_empty());
        assert_eq!(d.as_slice()[0].id, "DDS0202");

        let mut d = Diagnostics::default();
        let k = parse(&text("COLHDG('A' UNIQUE"), &mut d);
        assert!(k.is_empty());
        assert_eq!(d.as_slice()[0].id, "DDS0203");

        let mut d = Diagnostics::default();
        let k = parse(&text("TEXT('a')x UNIQUE 9ABC DFT(X'C')"), &mut d);
        assert_eq!(k.len(), 1);
        assert_eq!(k[0].name, "UNIQUE");
        let ids: Vec<_> = d.as_slice().iter().map(|d| d.id).collect();
        assert_eq!(ids, ["DDS0204", "DDS0201", "DDS0211"]);
    }

    fn line(func: &str) -> SourceLine {
        let mut text = [ccsid::SPACE; 80];
        for (i, ch) in func.chars().enumerate() {
            text[44 + i] = ccsid::byte_of(ch).unwrap();
        }
        SourceLine {
            seq: 100,
            date: 0,
            text,
        }
    }

    #[test]
    fn continuation_signs() {
        let mut ft = FunctionText::default();
        let mut cont = None;
        ft.append(&line("TEXT('Customer +"), 0, &mut cont);
        assert_eq!(cont, Some('+'));
        ft.append(&line("    master')"), 1, &mut cont);
        assert_eq!(cont, None);
        ft.append(&line("COLHDG('A   -"), 2, &mut cont);
        ft.append(&line("B')"), 3, &mut cont);
        ft.append(&line("UNIQUE"), 4, &mut cont);
        let s: String = ft.chars.iter().collect();
        assert_eq!(s, "TEXT('Customer master') COLHDG('A   B') UNIQUE");
        assert_eq!(ft.lines[0], 0);
        assert_eq!(*ft.lines.last().unwrap(), 4);
        let mut d = Diagnostics::default();
        let k = parse(&ft, &mut d);
        assert_eq!(k[0].quoted(0), Some("Customer master"));
        assert_eq!(k[1].quoted(0), Some("A   B"));
        assert_eq!(k[2].line, 4);
    }
}

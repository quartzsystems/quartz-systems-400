// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tokens of free-form statements, keyword areas and fixed-form
//! factors. A `*` right after an operand multiplies; one that follows
//! a blank or no operand and comes before a letter or digit starts a
//! special word (`*INLR`, `*BLANKS`). A character literal left open
//! at the end of a line continues on the next one when its last
//! nonblank character is `+` (resume at the next line's first nonblank)
//! or `-` (resume at the first column of the area).

use crate::ast::Pos;
use crate::diag::{Diag, ids};

/// What a token is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A name, upper case.
    Name(String),
    /// A built-in function name with its `%`, upper case.
    Bif(String),
    /// A special word with its `*`, upper case (`*INLR`, `*ALL`).
    Special(String),
    /// A numeric literal as written.
    Num(String),
    /// A character literal, quotes undoubled.
    Str(String),
    /// A prefixed literal: the prefix (`X G U D T Z`) and its text.
    Typed(char, String),
    /// `*ALL'x'`, `*ALLX'..'`, `*ALLG'..'`: the kind and the text.
    All(String, String),
    /// Punctuation and operators.
    Punct(&'static str),
    /// `;`.
    Semi,
}

/// A token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tok {
    /// What it is.
    pub kind: Kind,
    /// Where it starts.
    pub at: Pos,
}

impl Tok {
    /// The token as text, for messages and listings.
    #[must_use]
    pub fn text(&self) -> String {
        match &self.kind {
            Kind::Name(s) | Kind::Bif(s) | Kind::Special(s) | Kind::Num(s) => s.clone(),
            Kind::Punct(s) => (*s).to_string(),
            Kind::Str(s) => format!("'{}'", s.replace('\'', "''")),
            Kind::Typed(p, s) => format!("{p}'{s}'"),
            Kind::All(k, s) => format!("{k}'{s}'"),
            Kind::Semi => ";".into(),
        }
    }

    /// Whether this is the punctuation `p`.
    #[must_use]
    pub fn is(&self, p: &str) -> bool {
        matches!(&self.kind, Kind::Punct(q) if *q == p)
    }

    /// The name, when the token is one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            Kind::Name(s) => Some(s),
            _ => None,
        }
    }
}

const PUNCT: &[&str] = &[
    "**=", "+=", "-=", "*=", "/=", "**", "<=", ">=", "<>", "(", ")", ":", ",", ".", "=", "<", ">", "+",
    "-", "*", "/",
];

/// Characters a name may hold after its first.
#[must_use]
pub fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '@' | '$')
}

/// Characters a name may start with.
#[must_use]
pub fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || matches!(c, '_' | '#' | '@' | '$')
}

/// One line of an area to tokenize: its listing number, the column of
/// its first character, and the text.
#[derive(Clone, Debug)]
pub struct Segment<'a> {
    /// Listing line number.
    pub line: u32,
    /// Column of `text`'s first character.
    pub col: u16,
    /// The text.
    pub text: &'a str,
}

/// Tokenize the lines of an area, with literal continuation and `//`
/// comments to the end of a line.
#[must_use]
pub fn lex(segments: &[Segment<'_>], diags: &mut Vec<Diag>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    // An open literal carried to the next line: prefix, text, start.
    let mut open: Option<(Option<char>, String, Pos, String)> = None;
    for seg in segments {
        let chars: Vec<char> = seg.text.chars().collect();
        let mut i = 0;
        let pos = |i: usize| Pos {
            line: seg.line,
            col: seg.col + u16::try_from(i).unwrap_or(u16::MAX),
        };
        if let Some((prefix, mut text, at, cont)) = open.take() {
            // Resume the literal.
            if cont == "+" {
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
            }
            match read_quoted(&chars, i, &mut text) {
                Quoted::Closed(next) => {
                    out.push(literal(prefix, text, at));
                    i = next;
                }
                Quoted::Open(c) => {
                    open = Some((prefix, text, at, c));
                    continue;
                }
                Quoted::Unterminated => {
                    diags.push(Diag::new(ids::MISSING_APOSTROPHE, 20, at));
                    out.push(literal(prefix, text, at));
                    continue;
                }
            }
        }
        while i < chars.len() {
            let c = chars[i];
            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            if c == '/' && chars.get(i + 1) == Some(&'/') {
                break;
            }
            let at = pos(i);
            if c == ';' {
                out.push(Tok { kind: Kind::Semi, at });
                i += 1;
                continue;
            }
            // Prefixed literals.
            if matches!(c.to_ascii_uppercase(), 'X' | 'G' | 'U' | 'D' | 'T' | 'Z')
                && chars.get(i + 1) == Some(&'\'')
                && !after_name(&chars, i)
            {
                let mut text = String::new();
                match read_quoted(&chars, i + 2, &mut text) {
                    Quoted::Closed(next) => {
                        out.push(literal(Some(c.to_ascii_uppercase()), text, at));
                        i = next;
                    }
                    Quoted::Open(cont) => {
                        open = Some((Some(c.to_ascii_uppercase()), text, at, cont));
                        i = chars.len();
                    }
                    Quoted::Unterminated => {
                        diags.push(Diag::new(ids::MISSING_APOSTROPHE, 20, at));
                        out.push(literal(Some(c.to_ascii_uppercase()), text, at));
                        i = chars.len();
                    }
                }
                continue;
            }
            if c == '\'' {
                let mut text = String::new();
                match read_quoted(&chars, i + 1, &mut text) {
                    Quoted::Closed(next) => {
                        out.push(literal(None, text, at));
                        i = next;
                    }
                    Quoted::Open(cont) => {
                        open = Some((None, text, at, cont));
                        i = chars.len();
                    }
                    Quoted::Unterminated => {
                        diags.push(Diag::new(ids::MISSING_APOSTROPHE, 20, at));
                        out.push(literal(None, text, at));
                        i = chars.len();
                    }
                }
                continue;
            }
            if c == '%' && chars.get(i + 1).is_some_and(char::is_ascii_alphabetic) {
                let mut j = i + 1;
                while j < chars.len() && name_char(chars[j]) {
                    j += 1;
                }
                let w: String = chars[i..j].iter().collect::<String>().to_ascii_uppercase();
                out.push(Tok { kind: Kind::Bif(w), at });
                i = j;
                continue;
            }
            // A special word follows no operand, or a blank (`if *in01`,
            // `x AND *INLR`); `a*b` and `a * b` multiply.
            let blank_before = i > 0 && chars[i - 1] == ' ';
            if c == '*'
                && (!operand_ended(&out) || blank_before)
                && chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric)
            {
                let mut j = i + 1;
                while j < chars.len() && name_char(chars[j]) {
                    j += 1;
                }
                let w: String = chars[i..j].iter().collect::<String>().to_ascii_uppercase();
                // *ALL'x', *ALLX'..', *ALLG'..'
                if matches!(w.as_str(), "*ALL" | "*ALLX" | "*ALLG") && chars.get(j) == Some(&'\'') {
                    let mut text = String::new();
                    let next = if let Quoted::Closed(n) = read_quoted(&chars, j + 1, &mut text) {
                        n
                    } else {
                        diags.push(Diag::new(ids::MISSING_APOSTROPHE, 20, at));
                        chars.len()
                    };
                    out.push(Tok {
                        kind: Kind::All(w, text),
                        at,
                    });
                    i = next;
                    continue;
                }
                out.push(Tok {
                    kind: Kind::Special(w),
                    at,
                });
                i = j;
                continue;
            }
            if c.is_ascii_digit() || c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit) && !operand_ended(&out) {
                let j = number_end(&chars, i);
                let w: String = chars[i..j].iter().collect();
                out.push(Tok {
                    kind: Kind::Num(w.to_ascii_uppercase()),
                    at,
                });
                i = j;
                continue;
            }
            if name_start(c) {
                let mut j = i + 1;
                while j < chars.len() && name_char(chars[j]) {
                    j += 1;
                }
                // A name continued with `...` at the end of the line is
                // the definition parser's business; here it is a name.
                let w: String = chars[i..j].iter().collect::<String>().to_ascii_uppercase();
                out.push(Tok { kind: Kind::Name(w), at });
                i = j;
                continue;
            }
            if let Some(p) = PUNCT.iter().find(|p| {
                let pc: Vec<char> = p.chars().collect();
                chars.get(i..i + pc.len()) == Some(&pc[..])
            }) {
                out.push(Tok {
                    kind: Kind::Punct(p),
                    at,
                });
                i += p.len();
                continue;
            }
            diags.push(Diag::new(ids::INVALID_CHARACTER, 30, at).arg(c.to_string()));
            i += 1;
        }
    }
    if let Some((prefix, text, at, _)) = open {
        diags.push(Diag::new(ids::MISSING_APOSTROPHE, 20, at));
        out.push(literal(prefix, text, at));
    }
    out
}

/// Tokenize one piece of text (a fixed-form factor, a keyword area).
#[must_use]
pub fn lex_text(text: &str, at: Pos, diags: &mut Vec<Diag>) -> Vec<Tok> {
    lex(
        &[Segment {
            line: at.line,
            col: at.col,
            text,
        }],
        diags,
    )
}

/// Whether the last token ends an operand, so a `*` is multiplication.
fn operand_ended(out: &[Tok]) -> bool {
    out.last().is_some_and(|t| match &t.kind {
        Kind::Name(_) | Kind::Num(_) | Kind::Str(_) | Kind::Typed(..) | Kind::All(..) | Kind::Special(_) => true,
        Kind::Punct(p) => *p == ")",
        _ => false,
    })
}

/// Whether position `i` continues a name (so `ABX'` is not a hex literal).
fn after_name(chars: &[char], i: usize) -> bool {
    i > 0 && name_char(chars[i - 1])
}

/// The end of a numeric literal: digits, a decimal point, digits, and
/// an exponent `E[+-]digits` for a float.
fn number_end(chars: &[char], start: usize) -> usize {
    let mut j = start;
    while j < chars.len() && chars[j].is_ascii_digit() {
        j += 1;
    }
    if j < chars.len() && (chars[j] == '.' || chars[j] == ',') && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
        j += 1;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
    } else if j < chars.len() && chars[j] == '.' && !chars.get(j + 1).is_some_and(|c| name_start(*c)) {
        j += 1;
    }
    if j < chars.len() && matches!(chars[j], 'E' | 'e') {
        let mut k = j + 1;
        if k < chars.len() && matches!(chars[k], '+' | '-') {
            k += 1;
        }
        if k < chars.len() && chars[k].is_ascii_digit() {
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            j = k;
        }
    }
    j
}

/// How reading a quoted literal ended.
enum Quoted {
    /// At the closing quote; the index after it.
    Closed(usize),
    /// Open at the end of the line with a continuation (`+` or `-`).
    Open(String),
    /// Open at the end of the line without one.
    Unterminated,
}

/// Read literal text from `i` (just past the opening quote) into `text`.
fn read_quoted(chars: &[char], mut i: usize, text: &mut String) -> Quoted {
    while i < chars.len() {
        if chars[i] == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                text.push('\'');
                i += 2;
                continue;
            }
            return Quoted::Closed(i + 1);
        }
        text.push(chars[i]);
        i += 1;
    }
    // Open: is the last nonblank a continuation character?
    let trimmed = text.trim_end().to_string();
    if let Some(last) = trimmed.chars().last()
        && (last == '+' || last == '-')
    {
        let cont = last.to_string();
        text.truncate(trimmed.len() - 1);
        return Quoted::Open(cont);
    }
    Quoted::Unterminated
}

fn literal(prefix: Option<char>, text: String, at: Pos) -> Tok {
    Tok {
        kind: match prefix {
            None => Kind::Str(text),
            Some(p) => Kind::Typed(p, text),
        },
        at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<Kind> {
        let mut d = Vec::new();
        let t = lex_text(s, Pos { line: 1, col: 1 }, &mut d);
        assert!(d.is_empty(), "{d:?}");
        t.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn star_is_an_operator_after_an_operand() {
        assert_eq!(
            kinds("x = a * *INLR"),
            vec![
                Kind::Name("X".into()),
                Kind::Punct("="),
                Kind::Name("A".into()),
                Kind::Punct("*"),
                Kind::Special("*INLR".into()),
            ]
        );
        assert_eq!(
            kinds("*in01 = *on;"),
            vec![
                Kind::Special("*IN01".into()),
                Kind::Punct("="),
                Kind::Special("*ON".into()),
                Kind::Semi,
            ]
        );
        assert_eq!(kinds("a**2")[1], Kind::Punct("**"));
    }

    #[test]
    fn literals_of_every_kind() {
        assert_eq!(
            kinds("'It''s' x'C1' d'2026-10-23' = *ALL'-' 1.5E3 + .5 12"),
            vec![
                Kind::Str("It's".into()),
                Kind::Typed('X', "C1".into()),
                Kind::Typed('D', "2026-10-23".into()),
                Kind::Punct("="),
                Kind::All("*ALL".into(), "-".into()),
                Kind::Num("1.5E3".into()),
                Kind::Punct("+"),
                Kind::Num(".5".into()),
                Kind::Num("12".into()),
            ]
        );
    }

    #[test]
    fn continued_literals_and_comments() {
        let mut d = Vec::new();
        let segs = [
            Segment { line: 1, col: 8, text: "x = 'abc  +" },
            Segment { line: 2, col: 8, text: "     def'; // a comment" },
        ];
        let t = lex(&segs, &mut d);
        assert!(d.is_empty());
        assert_eq!(t[2].kind, Kind::Str("abc  def".into()));
        let segs = [
            Segment { line: 1, col: 8, text: "x = 'abc-" },
            Segment { line: 2, col: 8, text: "   def';" },
        ];
        let t = lex(&segs, &mut d);
        assert_eq!(t[2].kind, Kind::Str("abc   def".into()));
    }
}

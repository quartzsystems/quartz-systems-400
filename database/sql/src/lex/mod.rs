// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lexer: SQL text to tokens with their positions. Regular
//! identifiers fold to upper case; delimited ones keep their case.

use std::fmt::Write as _;

use crate::error::{Result, SqlError};

/// A token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A regular identifier or keyword, upper case.
    Word(String),
    /// A delimited identifier, as written.
    Delimited(String),
    /// A character string literal.
    Str(String),
    /// A hexadecimal string literal.
    Hex(Vec<u8>),
    /// A numeric literal, as written.
    Num(String),
    /// `?`.
    Param,
    /// `:NAME` or `:NAME:IND`.
    Host {
        /// Variable name, upper case.
        name: String,
        /// Indicator variable.
        ind: Option<String>,
    },
    /// An operator or punctuation.
    Sym(&'static str),
    /// End of input.
    End,
}

impl Tok {
    /// The token as the user would recognise it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Word(w) => w.clone(),
            Self::Delimited(d) => format!("\"{d}\""),
            Self::Str(s) => format!("'{s}'"),
            Self::Hex(h) => {
                let mut s = String::from("X'");
                for b in h {
                    let _ = write!(s, "{b:02X}");
                }
                s.push('\'');
                s
            }
            Self::Num(n) => n.clone(),
            Self::Param => "?".into(),
            Self::Host { name, ind } => match ind {
                Some(i) => format!(":{name}:{i}"),
                None => format!(":{name}"),
            },
            Self::Sym(s) => (*s).to_string(),
            Self::End => "end of statement".into(),
        }
    }
}

/// A token with its position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The token.
    pub tok: Tok,
    /// Line, from 1.
    pub line: u32,
    /// Column, from 1.
    pub col: u32,
}

const SYMS: &[&str] = &[
    "<>", "!=", "<=", ">=", "||", "=", "<", ">", "+", "-", "*", "/", "(", ")", ",", ".", ";", "?",
    "¬=",
];

struct Lexer<'a> {
    src: &'a [u8],
    text: &'a str,
    at: usize,
    line: u32,
    col: u32,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.get(self.at + 1).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.at += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else if b & 0xC0 != 0x80 {
            self.col += 1;
        }
        Some(b)
    }

    fn syntax(&self, found: &str, expected: &str) -> SqlError {
        SqlError::Syntax {
            line: self.line,
            col: self.col,
            found: found.into(),
            expected: expected.into(),
        }
    }

    fn skip_space(&mut self) -> Result<()> {
        loop {
            match (self.peek(), self.peek2()) {
                (Some(b), _) if b.is_ascii_whitespace() => {
                    self.bump();
                }
                (Some(b'-'), Some(b'-')) => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.bump();
                    }
                }
                (Some(b'/'), Some(b'*')) => {
                    self.bump();
                    self.bump();
                    loop {
                        match (self.peek(), self.peek2()) {
                            (Some(b'*'), Some(b'/')) => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            (None, _) => return Err(self.syntax("end of text", "*/")),
                            _ => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// A quoted string from after the opening quote; a doubled quote is one.
    fn quoted(&mut self, q: u8) -> Result<String> {
        let start = self.at;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(self.syntax("end of text", &format!("closing {}", char::from(q))));
                }
                Some(b) if b == q => {
                    if self.peek() == Some(q) {
                        self.bump();
                        out.push(char::from(q));
                    } else {
                        return Ok(out);
                    }
                }
                Some(_) => {
                    // Copy whole UTF-8 characters.
                    let end = self.at;
                    while self.peek().is_some_and(|b| b & 0xC0 == 0x80) {
                        self.bump();
                    }
                    out.push_str(&self.text[end - 1..self.at]);
                    let _ = start;
                }
            }
        }
    }

    fn word(&mut self) -> String {
        let start = self.at;
        while self.peek().is_some_and(|b| {
            b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#' || b == b'@'
        }) {
            self.bump();
        }
        self.text[start..self.at].to_ascii_uppercase()
    }

    fn number(&mut self) -> Result<String> {
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some(b'.') && self.peek2().is_none_or(|b| !b.is_ascii_alphabetic()) {
            self.bump();
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(b'E' | b'e')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            if !self.peek().is_some_and(|b| b.is_ascii_digit()) {
                return Err(self.syntax(&self.text[start..self.at], "exponent digits"));
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.bump();
            }
        }
        Ok(self.text[start..self.at].to_string())
    }

    fn next(&mut self) -> Result<Token> {
        self.skip_space()?;
        let (line, col) = (self.line, self.col);
        let token = |tok| Token { tok, line, col };
        let Some(b) = self.peek() else {
            return Ok(token(Tok::End));
        };
        // Hex and graphic string literals: X'..', G'..', N'..'.
        if matches!(b, b'X' | b'x') && self.peek2() == Some(b'\'') {
            self.bump();
            self.bump();
            let s = self.quoted(b'\'')?;
            let digits: String = s.chars().filter(|c| !c.is_whitespace()).collect();
            if !digits.len().is_multiple_of(2) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(SqlError::Literal {
                    text: format!("X'{s}'"),
                    what: "an even number of hexadecimal digits is needed".into(),
                });
            }
            let bytes = (0..digits.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("hex"))
                .collect();
            return Ok(token(Tok::Hex(bytes)));
        }
        if matches!(b, b'G' | b'g' | b'N' | b'n') && self.peek2() == Some(b'\'') {
            self.bump();
            self.bump();
            return Ok(token(Tok::Str(self.quoted(b'\'')?)));
        }
        if b == b'\'' {
            self.bump();
            return Ok(token(Tok::Str(self.quoted(b'\'')?)));
        }
        if b == b'"' {
            self.bump();
            return Ok(token(Tok::Delimited(self.quoted(b'"')?)));
        }
        if b.is_ascii_digit() || (b == b'.' && self.peek2().is_some_and(|c| c.is_ascii_digit())) {
            return Ok(token(Tok::Num(self.number()?)));
        }
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b == b'#' || b == b'@' {
            return Ok(token(Tok::Word(self.word())));
        }
        if b == b':' {
            self.bump();
            let name = self.word();
            if name.is_empty() {
                return Err(self.syntax(":", "a host variable name"));
            }
            let ind = if self.peek() == Some(b':') {
                self.bump();
                let i = self.word();
                if i.is_empty() {
                    return Err(self.syntax(":", "an indicator variable name"));
                }
                Some(i)
            } else {
                None
            };
            return Ok(token(Tok::Host { name, ind }));
        }
        if b == b'?' {
            self.bump();
            return Ok(token(Tok::Param));
        }
        for s in SYMS {
            if self.text[self.at..].starts_with(s) {
                for _ in 0..s.len() {
                    self.bump();
                }
                let sym = match *s {
                    "!=" | "¬=" => "<>",
                    other => other,
                };
                return Ok(token(Tok::Sym(sym)));
            }
        }
        let ch = self.text[self.at..].chars().next().expect("some");
        Err(self.syntax(&ch.to_string(), "a token"))
    }
}

/// Tokenize the whole text; the last token is `End`.
pub fn tokenize(text: &str) -> Result<Vec<Token>> {
    let mut lx = Lexer {
        src: text.as_bytes(),
        text,
        at: 0,
        line: 1,
        col: 1,
    };
    let mut out = Vec::new();
    loop {
        let t = lx.next()?;
        let end = t.tok == Tok::End;
        out.push(t);
        if end {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokenize(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn tokens() {
        assert_eq!(
            toks(
                "select a.\"MiXed\", 'it''s', X'C1C2', 1.5e3 from t where x <> ? and y = :H:I -- c"
            ),
            [
                Tok::Word("SELECT".into()),
                Tok::Word("A".into()),
                Tok::Sym("."),
                Tok::Delimited("MiXed".into()),
                Tok::Sym(","),
                Tok::Str("it's".into()),
                Tok::Sym(","),
                Tok::Hex(vec![0xC1, 0xC2]),
                Tok::Sym(","),
                Tok::Num("1.5e3".into()),
                Tok::Word("FROM".into()),
                Tok::Word("T".into()),
                Tok::Word("WHERE".into()),
                Tok::Word("X".into()),
                Tok::Sym("<>"),
                Tok::Param,
                Tok::Word("AND".into()),
                Tok::Word("Y".into()),
                Tok::Sym("="),
                Tok::Host {
                    name: "H".into(),
                    ind: Some("I".into())
                },
                Tok::End,
            ]
        );
        assert_eq!(
            toks("a != b /* x */ || c"),
            [
                Tok::Word("A".into()),
                Tok::Sym("<>"),
                Tok::Word("B".into()),
                Tok::Sym("||"),
                Tok::Word("C".into()),
                Tok::End
            ]
        );
        let t = tokenize("a\n  b").unwrap();
        assert_eq!((t[1].line, t[1].col), (2, 3));
        assert!(matches!(tokenize("'open"), Err(SqlError::Syntax { .. })));
        assert!(matches!(tokenize("X'ABC'"), Err(SqlError::Literal { .. })));
    }
}

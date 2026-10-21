// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `.qir` lexer. Lines matter: a newline ends a declaration or an
//! instruction, so it is a token. `;` starts a comment.

use crate::error::{QirError, Result};

/// A token.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// A word: keyword, opcode (dots allowed inside), name or type.
    Ident(String),
    /// `$name`: a variable.
    Var(String),
    /// `%name`: a register.
    Reg(String),
    /// `*WORD`: a special value.
    Special(String),
    /// An integer literal.
    Int(i128),
    /// A float literal (with an exponent).
    Float(f64),
    /// A decimal literal with a point, as written.
    Dec(String),
    /// A quoted string.
    Str(String),
    /// `x"..."` bytes.
    Hex(Vec<u8>),
    /// Punctuation.
    Sym(&'static str),
    /// End of line.
    Newline,
    /// End of input.
    End,
}

impl Tok {
    /// The token as the reader would recognise it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Ident(s) | Self::Dec(s) => s.clone(),
            Self::Var(s) => format!("${s}"),
            Self::Reg(s) => format!("%{s}"),
            Self::Special(s) => format!("*{s}"),
            Self::Int(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::Str(s) => format!("\"{s}\""),
            Self::Hex(h) => format!("x\"{}\"", hex(h)),
            Self::Sym(s) => (*s).to_string(),
            Self::Newline => "end of line".into(),
            Self::End => "end of input".into(),
        }
    }
}

/// Bytes as upper-case hexadecimal.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02X}");
        out
    })
}

/// A token with its position.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// The token.
    pub tok: Tok,
    /// Line, from 1.
    pub line: u32,
    /// Column, from 1.
    pub col: u32,
}

const SYMS: &[&str] = &[
    "->", "..", "(", ")", "{", "}", "[", "]", ",", ":", "=", ".", "/", "+", "@",
];

struct Lexer<'a> {
    src: &'a [u8],
    at: usize,
    line: u32,
    col: u32,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }

    fn peek_at(&self, n: usize) -> Option<u8> {
        self.src.get(self.at + n).copied()
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

    fn err(&self, msg: impl Into<String>) -> QirError {
        QirError::syntax(self.line, self.col, msg)
    }

    fn word(&mut self, dots: bool) -> String {
        let start = self.at;
        while let Some(b) = self.peek() {
            let ok = b.is_ascii_alphanumeric()
                || b == b'_'
                || b == b'#'
                || (dots && b == b'.' && self.peek_at(1).is_some_and(|c| c.is_ascii_alphabetic()));
            if !ok {
                break;
            }
            self.bump();
        }
        String::from_utf8_lossy(&self.src[start..self.at]).into_owned()
    }

    fn string(&mut self) -> Result<String> {
        // Opening quote already consumed.
        let mut out = Vec::new();
        loop {
            match self.bump() {
                None | Some(b'\n') => return Err(self.err("string is not closed")),
                Some(b'"') => {
                    if self.peek() == Some(b'"') {
                        self.bump();
                        out.push(b'"');
                    } else {
                        break;
                    }
                }
                Some(b) => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| self.err("string is not UTF-8"))
    }

    fn number(&mut self) -> Result<Tok> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        let mut point = false;
        let mut exp = false;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() {
                self.bump();
            } else if b == b'.'
                && !point
                && !exp
                && self.peek_at(1).is_some_and(|c| c.is_ascii_digit())
            {
                point = true;
                self.bump();
            } else if (b == b'e' || b == b'E') && !exp {
                let next = self.peek_at(1);
                let digit_after_sign = matches!(next, Some(b'+' | b'-'))
                    && self.peek_at(2).is_some_and(|c| c.is_ascii_digit());
                if next.is_some_and(|c| c.is_ascii_digit()) || digit_after_sign {
                    exp = true;
                    self.bump();
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.bump();
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let text = String::from_utf8_lossy(&self.src[start..self.at]).into_owned();
        if exp {
            text.parse::<f64>()
                .map(Tok::Float)
                .map_err(|_| self.err(format!("{text} is not a number")))
        } else if point {
            Ok(Tok::Dec(text))
        } else {
            text.parse::<i128>()
                .map(Tok::Int)
                .map_err(|_| self.err(format!("{text} is not an integer")))
        }
    }

    fn next(&mut self) -> Result<Token> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => {
                    self.bump();
                }
                Some(b';') => {
                    while let Some(b) = self.peek() {
                        if b == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        let (line, col) = (self.line, self.col);
        let tok = match self.peek() {
            None => Tok::End,
            Some(b'\n') => {
                self.bump();
                Tok::Newline
            }
            Some(b'$') => {
                self.bump();
                let w = self.word(false);
                if w.is_empty() {
                    return Err(self.err("a variable name follows $"));
                }
                Tok::Var(w)
            }
            Some(b'%') => {
                self.bump();
                let w = self.word(false);
                if w.is_empty() {
                    return Err(self.err("a register name follows %"));
                }
                Tok::Reg(w)
            }
            Some(b'*') => {
                self.bump();
                let w = self.word(false);
                if w.is_empty() {
                    return Err(self.err("a word follows *"));
                }
                Tok::Special(w.to_ascii_uppercase())
            }
            Some(b'"') => {
                self.bump();
                Tok::Str(self.string()?)
            }
            Some(b'x' | b'X') if self.peek_at(1) == Some(b'"') => {
                self.bump();
                self.bump();
                let s = self.string()?;
                let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
                if !s.len().is_multiple_of(2) {
                    return Err(self.err("hex literal has an odd number of digits"));
                }
                let mut out = Vec::with_capacity(s.len() / 2);
                for i in (0..s.len()).step_by(2) {
                    let b = u8::from_str_radix(&s[i..i + 2], 16)
                        .map_err(|_| self.err("hex literal has a non-hex digit"))?;
                    out.push(b);
                }
                Tok::Hex(out)
            }
            Some(b) if b.is_ascii_digit() => self.number()?,
            Some(b'-') if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.number()?,
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => Tok::Ident(self.word(true)),
            Some(_) => {
                let rest = &self.src[self.at..];
                let sym = SYMS
                    .iter()
                    .find(|s| rest.starts_with(s.as_bytes()))
                    .ok_or_else(|| {
                        self.err(format!("unexpected character {:?}", char::from(rest[0])))
                    })?;
                for _ in 0..sym.len() {
                    self.bump();
                }
                Tok::Sym(sym)
            }
        };
        Ok(Token { tok, line, col })
    }
}

/// Tokenize a `.qir` text. Runs of newlines collapse to one.
pub fn lex(text: &str) -> Result<Vec<Token>> {
    let mut lx = Lexer {
        src: text.as_bytes(),
        at: 0,
        line: 1,
        col: 1,
    };
    let mut out: Vec<Token> = Vec::new();
    loop {
        let t = lx.next()?;
        let end = t.tok == Tok::End;
        if t.tok == Tok::Newline && out.last().is_none_or(|p| p.tok == Tok::Newline) {
            continue;
        }
        out.push(t);
        if end {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn words_and_sigils() {
        assert_eq!(
            toks("%r = cmp.eq $a.b, -5 ; c"),
            vec![
                Tok::Reg("r".into()),
                Tok::Sym("="),
                Tok::Ident("cmp.eq".into()),
                Tok::Var("a".into()),
                Tok::Sym("."),
                Tok::Ident("b".into()),
                Tok::Sym(","),
                Tok::Int(-5),
                Tok::End
            ]
        );
    }

    #[test]
    fn numbers_and_strings() {
        assert_eq!(
            toks("1.50 2e3 x\"C1 c2\" \"a\"\"b\" *ALL"),
            vec![
                Tok::Dec("1.50".into()),
                Tok::Float(2000.0),
                Tok::Hex(vec![0xC1, 0xC2]),
                Tok::Str("a\"b".into()),
                Tok::Special("ALL".into()),
                Tok::End
            ]
        );
    }

    #[test]
    fn newlines_collapse() {
        assert_eq!(
            toks("a\n\n\nb\n"),
            vec![
                Tok::Ident("a".into()),
                Tok::Newline,
                Tok::Ident("b".into()),
                Tok::Newline,
                Tok::End
            ]
        );
    }
}

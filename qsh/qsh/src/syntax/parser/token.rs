// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Tokens: words, descriptor numbers, operators and newlines.

use super::Parser;
use super::word::Mode;
use crate::syntax::ast::{Span, Word, WordPart};
use crate::syntax::error::{ParseError, ParseErrorKind};

/// An operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Op {
    AndIf,
    OrIf,
    Semi,
    DSemi,
    Amp,
    Pipe,
    LParen,
    RParen,
    Less,
    Great,
    DGreat,
    Clobber,
    LessGreat,
    LessAnd,
    GreatAnd,
    DLess,
    DLessDash,
}

impl Op {
    pub(super) fn text(self) -> &'static str {
        match self {
            Self::AndIf => "&&",
            Self::OrIf => "||",
            Self::Semi => ";",
            Self::DSemi => ";;",
            Self::Amp => "&",
            Self::Pipe => "|",
            Self::LParen => "(",
            Self::RParen => ")",
            Self::Less => "<",
            Self::Great => ">",
            Self::DGreat => ">>",
            Self::Clobber => ">|",
            Self::LessGreat => "<>",
            Self::LessAnd => "<&",
            Self::GreatAnd => ">&",
            Self::DLess => "<<",
            Self::DLessDash => "<<-",
        }
    }

    pub(super) fn is_redirect(self) -> bool {
        matches!(
            self,
            Self::Less
                | Self::Great
                | Self::DGreat
                | Self::Clobber
                | Self::LessGreat
                | Self::LessAnd
                | Self::GreatAnd
                | Self::DLess
                | Self::DLessDash
        )
    }
}

/// A token.
#[derive(Clone, Debug)]
pub(super) enum Tok {
    Word(Word),
    Io(u32),
    Op(Op),
    Newline,
    Eof,
}

impl Tok {
    /// The text named in an "unexpected token" message.
    pub(super) fn describe(&self) -> String {
        match self {
            Self::Word(w) => w
                .as_literal()
                .map_or_else(|| w.unquoted_text(), str::to_string),
            Self::Io(n) => n.to_string(),
            Self::Op(op) => op.text().to_string(),
            Self::Newline => "newline".to_string(),
            Self::Eof => "end of file".to_string(),
        }
    }

    /// The unquoted literal text of a word token.
    pub(super) fn literal(&self) -> Option<&str> {
        match self {
            Self::Word(w) => w.as_literal(),
            _ => None,
        }
    }
}

fn is_meta(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>'
    )
}

pub(super) fn ends_word(c: Option<char>) -> bool {
    c.is_none_or(is_meta)
}

impl Parser<'_> {
    /// Look at the next token.
    pub(super) fn peek(&mut self) -> Result<&Tok, ParseError> {
        if self.peeked.is_none() {
            let t = self.lex()?;
            self.peeked = Some(t);
        }
        Ok(&self.peeked.as_ref().expect("peeked").0)
    }

    /// The span of the next token.
    pub(super) fn peek_span(&mut self) -> Result<Span, ParseError> {
        self.peek()?;
        Ok(self.peeked.as_ref().expect("peeked").1)
    }

    /// Consume the next token.
    pub(super) fn next_tok(&mut self) -> Result<(Tok, Span), ParseError> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        self.lex()
    }

    /// An error naming the next token.
    pub(super) fn unexpected(&mut self, what: &'static str) -> ParseError {
        match self.peek() {
            Ok(Tok::Eof) => self.err(ParseErrorKind::Eof(what)),
            Ok(t) => {
                let d = t.describe();
                let line = self.peeked.as_ref().map_or(self.inp.line, |(_, s)| s.line);
                ParseError {
                    line,
                    kind: ParseErrorKind::Unexpected(d),
                }
            }
            Err(e) => e,
        }
    }

    fn lex(&mut self) -> Result<(Tok, Span), ParseError> {
        // Blanks and a comment.
        loop {
            match self.inp.peek() {
                Some(' ' | '\t') => {
                    self.inp.bump();
                }
                Some('#') => {
                    while !matches!(self.inp.raw(), None | Some('\n')) {
                        self.inp.bump();
                    }
                }
                _ => break,
            }
        }
        let start = self.inp.pos;
        let line = self.inp.line;
        let span = |p: &Self| Span {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end: u32::try_from(p.inp.pos).unwrap_or(u32::MAX),
            line,
        };
        let Some(c) = self.inp.peek() else {
            if !self.heres.is_empty() {
                self.read_heres()?;
            }
            return Ok((Tok::Eof, span(self)));
        };
        if c == '\n' {
            self.inp.bump();
            self.read_heres()?;
            return Ok((Tok::Newline, span(self)));
        }
        if let Some(op) = self.lex_op(c) {
            return Ok((Tok::Op(op), span(self)));
        }
        let parts = self.read_parts(Mode::Normal)?;
        let mut sp = span(self);
        if let [WordPart::Lit(s)] = parts.as_slice()
            && s.bytes().all(|b| b.is_ascii_digit())
            && matches!(self.inp.raw(), Some('<' | '>'))
            && let Ok(n) = s.parse::<u32>()
        {
            return Ok((Tok::Io(n), sp));
        }
        sp.end = u32::try_from(self.inp.pos).unwrap_or(u32::MAX);
        Ok((Tok::Word(Word { parts, span: sp }), sp))
    }

    fn lex_op(&mut self, c: char) -> Option<Op> {
        let op = match c {
            '&' => {
                self.inp.bump();
                if self.inp.peek() == Some('&') {
                    self.inp.bump();
                    Op::AndIf
                } else {
                    Op::Amp
                }
            }
            '|' => {
                self.inp.bump();
                if self.inp.peek() == Some('|') {
                    self.inp.bump();
                    Op::OrIf
                } else {
                    Op::Pipe
                }
            }
            ';' => {
                self.inp.bump();
                if self.inp.peek() == Some(';') {
                    self.inp.bump();
                    Op::DSemi
                } else {
                    Op::Semi
                }
            }
            '(' => {
                self.inp.bump();
                Op::LParen
            }
            ')' => {
                self.inp.bump();
                Op::RParen
            }
            '<' => {
                self.inp.bump();
                match self.inp.peek() {
                    Some('<') => {
                        self.inp.bump();
                        if self.inp.peek() == Some('-') {
                            self.inp.bump();
                            Op::DLessDash
                        } else {
                            Op::DLess
                        }
                    }
                    Some('&') => {
                        self.inp.bump();
                        Op::LessAnd
                    }
                    Some('>') => {
                        self.inp.bump();
                        Op::LessGreat
                    }
                    _ => Op::Less,
                }
            }
            '>' => {
                self.inp.bump();
                match self.inp.peek() {
                    Some('>') => {
                        self.inp.bump();
                        Op::DGreat
                    }
                    Some('&') => {
                        self.inp.bump();
                        Op::GreatAnd
                    }
                    Some('|') => {
                        self.inp.bump();
                        Op::Clobber
                    }
                    _ => Op::Great,
                }
            }
            _ => return None,
        };
        Some(op)
    }
}

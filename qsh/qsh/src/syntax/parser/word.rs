// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Words: quoting, parameter expansions, command substitutions and
//! arithmetic, read straight into [`WordPart`]s.

use std::sync::Arc;

use super::Parser;
use super::token::{Op, Tok, ends_word};
use crate::syntax::ast::{List, Param, ParamName, ParamOp, TestOp, Word, WordPart};
use crate::syntax::error::{ParseError, ParseErrorKind};

/// Which characters end a word and which are special inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// An ordinary word: metacharacters end it.
    Normal,
    /// Inside `"..."`.
    DQuote,
    /// An expanding here-document body: only `$`, backquote, backslash.
    Here,
    /// The word of `${name op word}`; `dq` when the expansion is itself
    /// inside double quotes.
    Brace { dq: bool },
    /// Inside `$((...))`.
    Arith,
}

impl Mode {
    fn in_dq(self) -> bool {
        matches!(
            self,
            Self::DQuote | Self::Here | Self::Brace { dq: true } | Self::Arith
        )
    }
}

fn flush(lit: &mut String, parts: &mut Vec<WordPart>) {
    if !lit.is_empty() {
        parts.push(WordPart::Lit(std::mem::take(lit)));
    }
}

impl Parser<'_> {
    /// Read word parts up to the end the mode defines.
    pub(crate) fn read_parts(&mut self, mode: Mode) -> Result<Vec<WordPart>, ParseError> {
        self.enter()?;
        let r = self.read_parts_inner(mode);
        self.leave();
        r
    }

    fn read_parts_inner(&mut self, mode: Mode) -> Result<Vec<WordPart>, ParseError> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let mut depth = 0u32;
        loop {
            let c = self.inp.peek();
            match mode {
                Mode::Normal => {
                    if ends_word(c) {
                        break;
                    }
                }
                Mode::DQuote => match c {
                    None => return Err(self.err(ParseErrorKind::Eof("a quoted string"))),
                    Some('"') => break,
                    _ => {}
                },
                Mode::Here => {
                    if c.is_none() {
                        break;
                    }
                }
                Mode::Brace { .. } => match c {
                    None => return Err(self.err(ParseErrorKind::Eof("a parameter substitution"))),
                    Some('}') => break,
                    _ => {}
                },
                Mode::Arith => match c {
                    None => return Err(self.err(ParseErrorKind::Eof("an arithmetic expansion"))),
                    Some('(') => {
                        depth += 1;
                        self.inp.bump();
                        lit.push('(');
                        continue;
                    }
                    Some(')') => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                        self.inp.bump();
                        lit.push(')');
                        continue;
                    }
                    _ => {}
                },
            }
            let Some(c) = c else { break };
            match c {
                '\\' => {
                    self.inp.bump();
                    match self.inp.raw() {
                        None => lit.push('\\'),
                        Some(n) => {
                            let escapes = match mode {
                                Mode::Normal | Mode::Brace { dq: false } => true,
                                Mode::Here => matches!(n, '$' | '`' | '\\'),
                                _ => matches!(n, '$' | '`' | '"' | '\\'),
                            };
                            if escapes {
                                self.inp.bump();
                                flush(&mut lit, &mut parts);
                                parts.push(WordPart::Quoted(n.to_string()));
                            } else {
                                lit.push('\\');
                            }
                        }
                    }
                }
                '\'' if !mode.in_dq() => {
                    self.inp.bump();
                    let mut s = String::new();
                    loop {
                        match self.inp.bump() {
                            None => return Err(self.err(ParseErrorKind::Eof("a quoted string"))),
                            Some('\'') => break,
                            Some(ch) => s.push(ch),
                        }
                    }
                    flush(&mut lit, &mut parts);
                    parts.push(WordPart::Quoted(s));
                }
                '"' if !matches!(mode, Mode::DQuote | Mode::Here) => {
                    self.inp.bump();
                    let inner = self.read_parts(Mode::DQuote)?;
                    self.inp.next();
                    flush(&mut lit, &mut parts);
                    parts.push(WordPart::Double(inner));
                }
                '`' => {
                    self.inp.bump();
                    let list = self.backquote(mode.in_dq())?;
                    flush(&mut lit, &mut parts);
                    parts.push(WordPart::Command(Arc::new(list)));
                }
                '$' => match self.dollar(mode)? {
                    Some(p) => {
                        flush(&mut lit, &mut parts);
                        parts.push(p);
                    }
                    None => lit.push('$'),
                },
                _ => {
                    self.inp.bump();
                    lit.push(c);
                }
            }
        }
        flush(&mut lit, &mut parts);
        Ok(parts)
    }

    /// At `$`: consume it and what follows; `None` for a lone `$`.
    fn dollar(&mut self, mode: Mode) -> Result<Option<WordPart>, ParseError> {
        let next = self.inp.peek2();
        match next {
            Some('{') => {
                self.inp.next();
                self.inp.next();
                let dq = mode.in_dq();
                self.enter()?;
                let r = self.braced(dq);
                self.leave();
                r.map(|p| Some(WordPart::Param(Box::new(p))))
            }
            Some('(') => {
                self.inp.next();
                // `$((` is arithmetic unless its parentheses say otherwise.
                if self.inp.peek2() == Some('(')
                    && let Some(w) = self.try_arith()?
                {
                    return Ok(Some(WordPart::Arith(w)));
                }
                self.inp.next();
                let list = self.cmdsub()?;
                Ok(Some(WordPart::Command(Arc::new(list))))
            }
            Some(c) if c == '_' || c.is_ascii_alphabetic() => {
                self.inp.next();
                let mut name = String::new();
                while let Some(c) = self.inp.peek() {
                    if c == '_' || c.is_ascii_alphanumeric() {
                        name.push(c);
                        self.inp.bump();
                    } else {
                        break;
                    }
                }
                Ok(Some(param(ParamName::Var(name), ParamOp::Plain)))
            }
            Some(d) if d.is_ascii_digit() => {
                self.inp.next();
                self.inp.next();
                let name = if d == '0' {
                    ParamName::Special('0')
                } else {
                    ParamName::Positional(d as usize - '0' as usize)
                };
                Ok(Some(param(name, ParamOp::Plain)))
            }
            Some(c @ ('@' | '*' | '#' | '?' | '-' | '$' | '!')) => {
                self.inp.next();
                self.inp.next();
                Ok(Some(param(ParamName::Special(c), ParamOp::Plain)))
            }
            _ => {
                self.inp.next();
                Ok(None)
            }
        }
    }

    /// At the second `(` of `$((`: an arithmetic expansion, or `None`
    /// with the cursor back at that `(` when it is a command
    /// substitution of a subshell.
    fn try_arith(&mut self) -> Result<Option<Word>, ParseError> {
        let (pos, line, heres) = (self.inp.pos, self.inp.line, self.heres.len());
        let start = self.span_here();
        self.inp.next();
        self.inp.next();
        if let Ok(parts) = self.read_parts(Mode::Arith)
            && self.inp.peek() == Some(')')
            && self.inp.peek2() == Some(')')
        {
            self.inp.next();
            self.inp.next();
            return Ok(Some(Word { parts, span: start }));
        }
        self.inp.pos = pos;
        self.inp.line = line;
        self.heres.truncate(heres);
        self.peeked = None;
        Ok(None)
    }

    /// After `$(`: the list up to the matching `)`.
    fn cmdsub(&mut self) -> Result<List, ParseError> {
        let saved = self.peeked.take();
        let alias_next = std::mem::take(&mut self.alias_next);
        let list = self.compound_list(&[")"], true)?;
        match self.next_tok()? {
            (Tok::Op(Op::RParen), _) => {}
            (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a command substitution"))),
            (t, s) => {
                return Err(ParseError {
                    line: s.line,
                    kind: ParseErrorKind::Unexpected(t.describe()),
                });
            }
        }
        self.peeked = saved;
        self.alias_next = alias_next;
        Ok(list)
    }

    /// After a backquote: the text to the closing one, parsed.
    fn backquote(&mut self, dq: bool) -> Result<List, ParseError> {
        let line = self.inp.line;
        let mut text = String::new();
        loop {
            match self.inp.bump() {
                None => return Err(self.err(ParseErrorKind::Eof("a backquoted command"))),
                Some('`') => break,
                Some('\\') => match self.inp.raw() {
                    Some(c @ ('$' | '`' | '\\')) => {
                        self.inp.bump();
                        text.push(c);
                    }
                    Some('"') if dq => {
                        self.inp.bump();
                        text.push('"');
                    }
                    Some('\n') => {
                        self.inp.bump();
                    }
                    _ => text.push('\\'),
                },
                Some(c) => text.push(c),
            }
        }
        let mut sub = Parser::at_line(&text, line);
        sub.aliases = self.aliases;
        sub.depth = self.depth;
        sub.parse_all().map_err(|e| {
            if e.is_incomplete() {
                ParseError {
                    line: e.line,
                    kind: ParseErrorKind::Unexpected("`".to_string()),
                }
            } else {
                e
            }
        })
    }

    /// After `${`: the rest of the expansion through `}`.
    fn braced(&mut self, dq: bool) -> Result<Param, ParseError> {
        if self.inp.peek() == Some('#') {
            let (pos, line) = (self.inp.pos, self.inp.line);
            self.inp.bump();
            if self.inp.peek() == Some('}') {
                self.inp.bump();
                return Ok(Param {
                    name: ParamName::Special('#'),
                    op: ParamOp::Plain,
                });
            }
            if let Some(name) = self.param_name()
                && self.inp.peek() == Some('}')
            {
                self.inp.bump();
                return Ok(Param {
                    name,
                    op: ParamOp::Length,
                });
            }
            self.inp.pos = pos;
            self.inp.line = line;
        }
        let Some(name) = self.param_name() else {
            return Err(self.bad_subst());
        };
        let op = match self.inp.peek() {
            Some('}') => {
                self.inp.bump();
                return Ok(Param {
                    name,
                    op: ParamOp::Plain,
                });
            }
            Some(':') => {
                self.inp.bump();
                let Some(op) = self.inp.peek().and_then(test_op) else {
                    return Err(self.bad_subst());
                };
                self.inp.bump();
                let word = self.brace_word(Mode::Brace { dq })?;
                ParamOp::Test {
                    op,
                    colon: true,
                    word,
                }
            }
            Some(c @ ('%' | '#')) => {
                self.inp.bump();
                let longest = self.inp.peek() == Some(c);
                if longest {
                    self.inp.bump();
                }
                let pattern = self.brace_word(Mode::Brace { dq: false })?;
                ParamOp::Trim {
                    suffix: c == '%',
                    longest,
                    pattern,
                }
            }
            Some(c) if test_op(c).is_some() => {
                self.inp.bump();
                let word = self.brace_word(Mode::Brace { dq })?;
                ParamOp::Test {
                    op: test_op(c).expect("test op"),
                    colon: false,
                    word,
                }
            }
            _ => return Err(self.bad_subst()),
        };
        Ok(Param { name, op })
    }

    fn brace_word(&mut self, mode: Mode) -> Result<Word, ParseError> {
        let span = self.span_here();
        let parts = self.read_parts(mode)?;
        self.inp.bump();
        Ok(Word { parts, span })
    }

    fn bad_subst(&mut self) -> ParseError {
        // Skip to the closing brace so the message names the line.
        if self.inp.raw().is_none() {
            return self.err(ParseErrorKind::Eof("a parameter substitution"));
        }
        self.err(ParseErrorKind::BadSubst)
    }

    fn param_name(&mut self) -> Option<ParamName> {
        let c = self.inp.peek()?;
        if c == '_' || c.is_ascii_alphabetic() {
            let mut name = String::new();
            while let Some(c) = self.inp.peek() {
                if c == '_' || c.is_ascii_alphanumeric() {
                    name.push(c);
                    self.inp.bump();
                } else {
                    break;
                }
            }
            return Some(ParamName::Var(name));
        }
        if c.is_ascii_digit() {
            let mut n: usize = 0;
            while let Some(d) = self.inp.peek().filter(char::is_ascii_digit) {
                n = n
                    .saturating_mul(10)
                    .saturating_add(d as usize - '0' as usize);
                self.inp.bump();
            }
            return Some(if n == 0 {
                ParamName::Special('0')
            } else {
                ParamName::Positional(n)
            });
        }
        if matches!(c, '@' | '*' | '#' | '?' | '-' | '$' | '!') {
            self.inp.bump();
            return Some(ParamName::Special(c));
        }
        None
    }
}

fn param(name: ParamName, op: ParamOp) -> WordPart {
    WordPart::Param(Box::new(Param { name, op }))
}

fn test_op(c: char) -> Option<TestOp> {
    Some(match c {
        '-' => TestOp::Default,
        '=' => TestOp::Assign,
        '?' => TestOp::Error,
        '+' => TestOp::Alternate,
        _ => return None,
    })
}

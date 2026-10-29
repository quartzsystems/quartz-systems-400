// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The grammar: lists, and-or lists, pipelines, commands.

use std::sync::{Arc, OnceLock};

use super::token::{Op, Tok};
use super::{Parser, PendingHere, heredoc, is_name};
use crate::syntax::ast::{
    AndOr, Assign, CaseArm, Command, Compound, Connector, FunctionDef, Item, List, Pipeline,
    RedirOp, RedirTarget, Redirect, SimpleCommand, Span, Word, WordPart,
};
use crate::syntax::error::{ParseError, ParseErrorKind};

/// Words reserved in command position.
const RESERVED: &[&str] = &[
    "if", "then", "else", "elif", "fi", "do", "done", "case", "esac", "while", "until", "for", "{",
    "}", "!",
];

impl Parser<'_> {
    /// One complete command: and-or lists up to a newline.
    pub(super) fn complete_command(&mut self) -> Result<Option<List>, ParseError> {
        while matches!(self.peek()?, Tok::Newline) {
            self.next_tok()?;
        }
        if matches!(self.peek()?, Tok::Eof) {
            return Ok(None);
        }
        let mut list = List::default();
        loop {
            let start = self.peek_span()?.start as usize;
            let and_or = self.and_or()?;
            let end = self.end_pos();
            let mut item = Item {
                and_or,
                background: false,
                text: self.inp.text(start, end),
            };
            match self.peek()? {
                Tok::Op(Op::Semi) => {
                    self.next_tok()?;
                }
                Tok::Op(Op::Amp) => {
                    self.next_tok()?;
                    item.background = true;
                }
                Tok::Newline | Tok::Eof => {}
                _ => return Err(self.unexpected("a command")),
            }
            list.items.push(item);
            match self.peek()? {
                Tok::Newline => {
                    self.next_tok()?;
                    break;
                }
                Tok::Eof => break,
                _ => {}
            }
        }
        Ok(Some(list))
    }

    /// Offset just past the last consumed token.
    fn end_pos(&self) -> usize {
        match &self.peeked {
            Some((_, s)) => {
                // Trim the blanks between the last token and the peeked one.
                let mut e = s.start as usize;
                while e > 0 && matches!(self.inp.chars.get(e - 1), Some(' ' | '\t')) {
                    e -= 1;
                }
                e
            }
            None => self.inp.pos,
        }
    }

    fn skip_newlines(&mut self) -> Result<(), ParseError> {
        while matches!(self.peek()?, Tok::Newline) {
            self.next_tok()?;
        }
        Ok(())
    }

    /// Is the next token one of the terminators (reserved words, `)`
    /// or `;;`)?
    fn at_term(&mut self, terms: &[&str]) -> Result<bool, ParseError> {
        Ok(match self.peek()? {
            Tok::Eof => true,
            Tok::Op(Op::RParen) => terms.contains(&")"),
            Tok::Op(Op::DSemi) => terms.contains(&";;"),
            t => t.literal().is_some_and(|w| terms.contains(&w)),
        })
    }

    /// A compound list up to one of `terms`, which is left unconsumed.
    pub(super) fn compound_list(
        &mut self,
        terms: &[&str],
        allow_empty: bool,
    ) -> Result<List, ParseError> {
        self.skip_newlines()?;
        let mut list = List::default();
        loop {
            if self.at_term(terms)? {
                break;
            }
            let start = self.peek_span()?.start as usize;
            let and_or = self.and_or()?;
            let end = self.end_pos();
            let mut item = Item {
                and_or,
                background: false,
                text: self.inp.text(start, end),
            };
            match self.peek()? {
                Tok::Op(Op::Semi) | Tok::Newline => {
                    self.next_tok()?;
                }
                Tok::Op(Op::Amp) => {
                    self.next_tok()?;
                    item.background = true;
                }
                _ => {
                    list.items.push(item);
                    if self.at_term(terms)? {
                        break;
                    }
                    return Err(self.unexpected("a command list"));
                }
            }
            list.items.push(item);
            self.skip_newlines()?;
        }
        if list.items.is_empty() && !allow_empty {
            return Err(self.unexpected("a command list"));
        }
        if matches!(self.peek()?, Tok::Eof) && !terms.is_empty() {
            return Err(self.err(ParseErrorKind::Eof("a compound command")));
        }
        Ok(list)
    }

    fn and_or(&mut self) -> Result<AndOr, ParseError> {
        let first = self.pipeline()?;
        let mut rest = Vec::new();
        loop {
            let conn = match self.peek()? {
                Tok::Op(Op::AndIf) => Connector::And,
                Tok::Op(Op::OrIf) => Connector::Or,
                _ => break,
            };
            self.next_tok()?;
            self.skip_newlines()?;
            rest.push((conn, self.pipeline()?));
        }
        Ok(AndOr { first, rest })
    }

    fn pipeline(&mut self) -> Result<Pipeline, ParseError> {
        let span = self.peek_span()?;
        let mut negate = false;
        if self.peek()?.literal() == Some("!") {
            self.next_tok()?;
            negate = true;
        }
        let mut commands = vec![self.command()?];
        while matches!(self.peek()?, Tok::Op(Op::Pipe)) {
            self.next_tok()?;
            self.skip_newlines()?;
            commands.push(self.command()?);
        }
        Ok(Pipeline {
            negate,
            commands,
            span,
        })
    }

    /// Expand an alias at the next token when it is one.
    fn try_alias(&mut self) -> Result<bool, ParseError> {
        let Some(aliases) = self.aliases else {
            return Ok(false);
        };
        let (name, span) = match &self.peeked {
            Some((t, s)) => match t.literal() {
                Some(w) if !RESERVED.contains(&w) => (w.to_string(), *s),
                _ => return Ok(false),
            },
            None => return Ok(false),
        };
        let Some(value) = aliases.get(&name) else {
            return Ok(false);
        };
        let (start, end) = (span.start as usize, span.end as usize);
        // The cursor is right after the peeked word.
        if self.inp.pos != end {
            return Ok(false);
        }
        self.inp.pos = start;
        if self.inp.alias_active(&name) {
            self.inp.pos = end;
            return Ok(false);
        }
        self.peeked = None;
        self.alias_next = value.ends_with([' ', '\t']);
        let value = value.clone();
        self.inp.splice(start, end, &value, &name);
        Ok(true)
    }

    fn command(&mut self) -> Result<Command, ParseError> {
        self.enter()?;
        let r = self.command_inner();
        self.leave();
        r
    }

    fn command_inner(&mut self) -> Result<Command, ParseError> {
        self.alias_next = false;
        self.peek()?;
        let mut guard = 0;
        while self.try_alias()? {
            self.peek()?;
            guard += 1;
            if guard > 1000 {
                break;
            }
        }
        let tok = self.peek()?.clone();
        match &tok {
            Tok::Op(Op::LParen) => {
                self.next_tok()?;
                let list = self.compound_list(&[")"], false)?;
                self.expect_op(Op::RParen, "a subshell")?;
                self.compound_tail(Compound::Subshell(list))
            }
            Tok::Word(w) => match w.as_literal() {
                Some("{") => {
                    self.next_tok()?;
                    let list = self.compound_list(&["}"], false)?;
                    self.expect_word("}", "a brace group")?;
                    self.compound_tail(Compound::Brace(list))
                }
                Some("if") => {
                    let c = self.if_command()?;
                    self.compound_tail(c)
                }
                Some(k @ ("while" | "until")) => {
                    let until = k == "until";
                    self.next_tok()?;
                    let cond = self.compound_list(&["do"], false)?;
                    self.expect_word("do", "a loop")?;
                    let body = self.compound_list(&["done"], false)?;
                    self.expect_word("done", "a loop")?;
                    self.compound_tail(Compound::Loop { until, cond, body })
                }
                Some("for") => {
                    let c = self.for_command()?;
                    self.compound_tail(c)
                }
                Some("case") => {
                    let c = self.case_command()?;
                    self.compound_tail(c)
                }
                Some(r) if RESERVED.contains(&r) => Err(self.unexpected("a command")),
                _ => self.simple_command(),
            },
            Tok::Io(_) => self.simple_command(),
            Tok::Op(op) if op.is_redirect() => self.simple_command(),
            _ => Err(self.unexpected("a command")),
        }
    }

    fn expect_op(&mut self, op: Op, what: &'static str) -> Result<(), ParseError> {
        if matches!(self.peek()?, Tok::Op(o) if *o == op) {
            self.next_tok()?;
            Ok(())
        } else {
            Err(self.unexpected(what))
        }
    }

    fn expect_word(&mut self, word: &str, what: &'static str) -> Result<(), ParseError> {
        if self.peek()?.literal() == Some(word) {
            self.next_tok()?;
            Ok(())
        } else {
            Err(self.unexpected(what))
        }
    }

    fn compound_tail(&mut self, c: Compound) -> Result<Command, ParseError> {
        let mut redirs = Vec::new();
        loop {
            match self.peek()? {
                Tok::Io(_) => {
                    let (Tok::Io(n), span) = self.next_tok()? else {
                        unreachable!()
                    };
                    redirs.push(self.redirect(Some(n), span)?);
                }
                Tok::Op(op) if op.is_redirect() => {
                    let span = self.peek_span()?;
                    redirs.push(self.redirect(None, span)?);
                }
                _ => break,
            }
        }
        Ok(Command::Compound(c, redirs))
    }

    fn if_command(&mut self) -> Result<Compound, ParseError> {
        self.next_tok()?;
        let mut arms = Vec::new();
        let mut else_body = None;
        loop {
            let cond = self.compound_list(&["then"], false)?;
            self.expect_word("then", "an if command")?;
            let body = self.compound_list(&["elif", "else", "fi"], false)?;
            arms.push((cond, body));
            match self.peek()?.literal() {
                Some("elif") => {
                    self.next_tok()?;
                }
                Some("else") => {
                    self.next_tok()?;
                    else_body = Some(self.compound_list(&["fi"], false)?);
                    self.expect_word("fi", "an if command")?;
                    break;
                }
                Some("fi") => {
                    self.next_tok()?;
                    break;
                }
                _ => return Err(self.unexpected("an if command")),
            }
        }
        Ok(Compound::If { arms, else_body })
    }

    fn for_command(&mut self) -> Result<Compound, ParseError> {
        self.next_tok()?;
        let var = match self.next_tok()? {
            (Tok::Word(w), _) if w.as_literal().is_some_and(is_name) => {
                w.as_literal().unwrap_or_default().to_string()
            }
            (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a for command"))),
            (t, s) => {
                return Err(ParseError {
                    line: s.line,
                    kind: ParseErrorKind::Unexpected(t.describe()),
                });
            }
        };
        let mut words = None;
        match self.peek()? {
            Tok::Op(Op::Semi) => {
                self.next_tok()?;
                self.skip_newlines()?;
            }
            _ => {
                self.skip_newlines()?;
                if self.peek()?.literal() == Some("in") {
                    self.next_tok()?;
                    let mut list = Vec::new();
                    loop {
                        match self.peek()? {
                            Tok::Word(_) => {
                                let (Tok::Word(w), _) = self.next_tok()? else {
                                    unreachable!()
                                };
                                list.push(w);
                            }
                            Tok::Op(Op::Semi) | Tok::Newline => {
                                self.next_tok()?;
                                break;
                            }
                            _ => return Err(self.unexpected("a for command")),
                        }
                    }
                    words = Some(list);
                    self.skip_newlines()?;
                }
            }
        }
        self.expect_word("do", "a for command")?;
        let body = self.compound_list(&["done"], false)?;
        self.expect_word("done", "a for command")?;
        Ok(Compound::For { var, words, body })
    }

    fn case_command(&mut self) -> Result<Compound, ParseError> {
        self.next_tok()?;
        let subject = match self.next_tok()? {
            (Tok::Word(w), _) => w,
            (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a case command"))),
            (t, s) => {
                return Err(ParseError {
                    line: s.line,
                    kind: ParseErrorKind::Unexpected(t.describe()),
                });
            }
        };
        self.skip_newlines()?;
        self.expect_word("in", "a case command")?;
        self.skip_newlines()?;
        let mut arms = Vec::new();
        loop {
            if self.peek()?.literal() == Some("esac") {
                self.next_tok()?;
                break;
            }
            if matches!(self.peek()?, Tok::Op(Op::LParen)) {
                self.next_tok()?;
            }
            let mut patterns = Vec::new();
            loop {
                match self.next_tok()? {
                    (Tok::Word(w), _) => patterns.push(w),
                    (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a case command"))),
                    (t, s) => {
                        return Err(ParseError {
                            line: s.line,
                            kind: ParseErrorKind::Unexpected(t.describe()),
                        });
                    }
                }
                match self.next_tok()? {
                    (Tok::Op(Op::Pipe), _) => {}
                    (Tok::Op(Op::RParen), _) => break,
                    (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a case command"))),
                    (t, s) => {
                        return Err(ParseError {
                            line: s.line,
                            kind: ParseErrorKind::Unexpected(t.describe()),
                        });
                    }
                }
            }
            let body = self.compound_list(&[";;", "esac"], true)?;
            arms.push(CaseArm { patterns, body });
            match self.peek()? {
                Tok::Op(Op::DSemi) => {
                    self.next_tok()?;
                    self.skip_newlines()?;
                }
                t if t.literal() == Some("esac") => {
                    self.next_tok()?;
                    break;
                }
                _ => return Err(self.unexpected("a case command")),
            }
        }
        Ok(Compound::Case { subject, arms })
    }

    fn simple_command(&mut self) -> Result<Command, ParseError> {
        let span = self.peek_span()?;
        let mut cmd = SimpleCommand {
            span,
            ..SimpleCommand::default()
        };
        loop {
            match self.peek()? {
                Tok::Io(_) => {
                    let (Tok::Io(n), span) = self.next_tok()? else {
                        unreachable!()
                    };
                    cmd.redirs.push(self.redirect(Some(n), span)?);
                }
                Tok::Op(op) if op.is_redirect() => {
                    let span = self.peek_span()?;
                    cmd.redirs.push(self.redirect(None, span)?);
                }
                Tok::Word(_) => {
                    if cmd.words.is_empty() || std::mem::take(&mut self.alias_next) {
                        // A command name after assignments may be an alias.
                        if (!cmd.assigns.is_empty()
                            || !cmd.words.is_empty()
                            || !cmd.redirs.is_empty())
                            && self.try_alias()?
                        {
                            continue;
                        }
                    }
                    let (Tok::Word(w), _) = self.next_tok()? else {
                        unreachable!()
                    };
                    if cmd.words.is_empty()
                        && let Some(a) = assignment(&w)
                    {
                        cmd.assigns.push(a);
                        continue;
                    }
                    let first = cmd.words.is_empty();
                    cmd.words.push(w);
                    if first
                        && cmd.assigns.is_empty()
                        && cmd.redirs.is_empty()
                        && matches!(self.peek()?, Tok::Op(Op::LParen))
                    {
                        let w = cmd.words.pop().expect("word");
                        return self.function_def(&w, span);
                    }
                }
                _ => break,
            }
        }
        if cmd.words.is_empty() && cmd.assigns.is_empty() && cmd.redirs.is_empty() {
            return Err(self.unexpected("a command"));
        }
        Ok(Command::Simple(cmd))
    }

    fn function_def(&mut self, name: &Word, span: Span) -> Result<Command, ParseError> {
        let Some(n) = name.as_literal().filter(|n| is_name(n)) else {
            return Err(ParseError {
                line: span.line,
                kind: ParseErrorKind::BadFunctionName(name.unquoted_text()),
            });
        };
        let n = n.to_string();
        self.next_tok()?;
        self.expect_op(Op::RParen, "a function definition")?;
        self.skip_newlines()?;
        let is_compound = match self.peek()? {
            Tok::Op(Op::LParen) => true,
            t => matches!(
                t.literal(),
                Some("{" | "if" | "while" | "until" | "for" | "case")
            ),
        };
        if !is_compound {
            return Err(self.unexpected("a function definition"));
        }
        let Command::Compound(body, redirs) = self.command()? else {
            return Err(self.unexpected("a function definition"));
        };
        let text = self.inp.text(span.start as usize, self.end_pos());
        Ok(Command::Function(Arc::new(FunctionDef {
            name: n,
            body,
            redirs,
            text,
            span,
        })))
    }

    fn redirect(&mut self, fd: Option<u32>, span: Span) -> Result<Redirect, ParseError> {
        let (Tok::Op(op), _) = self.next_tok()? else {
            return Err(self.unexpected("a redirection"));
        };
        let op = match op {
            Op::Less => RedirOp::In,
            Op::Great => RedirOp::Out,
            Op::DGreat => RedirOp::Append,
            Op::Clobber => RedirOp::Clobber,
            Op::LessGreat => RedirOp::ReadWrite,
            Op::LessAnd => RedirOp::DupIn,
            Op::GreatAnd => RedirOp::DupOut,
            Op::DLess => RedirOp::Here { strip_tabs: false },
            Op::DLessDash => RedirOp::Here { strip_tabs: true },
            _ => return Err(self.unexpected("a redirection")),
        };
        let (word, wspan) = match self.next_tok()? {
            (Tok::Word(w), s) => (w, s),
            (Tok::Eof, _) => return Err(self.err(ParseErrorKind::Eof("a redirection"))),
            (t, s) => {
                return Err(ParseError {
                    line: s.line,
                    kind: ParseErrorKind::Unexpected(t.describe()),
                });
            }
        };
        let target = if let RedirOp::Here { strip_tabs } = op {
            let raw = self.inp.text(wspan.start as usize, wspan.end as usize);
            let (delim, expand) = heredoc::delimiter(&raw);
            let cell = Arc::new(OnceLock::new());
            self.heres.push(PendingHere {
                delim,
                strip_tabs,
                expand,
                cell: Arc::clone(&cell),
            });
            RedirTarget::Here(cell)
        } else {
            RedirTarget::Word(word)
        };
        Ok(Redirect {
            fd,
            op,
            target,
            span,
        })
    }
}

/// `NAME=value` as an assignment.
fn assignment(w: &Word) -> Option<Assign> {
    let WordPart::Lit(first) = w.parts.first()? else {
        return None;
    };
    let (name, rest) = first.split_once('=')?;
    if !is_name(name) {
        return None;
    }
    let mut parts = Vec::with_capacity(w.parts.len());
    if !rest.is_empty() {
        parts.push(WordPart::Lit(rest.to_string()));
    }
    parts.extend(w.parts[1..].iter().cloned());
    Some(Assign {
        name: name.to_string(),
        value: Word {
            parts,
            span: w.span,
        },
    })
}

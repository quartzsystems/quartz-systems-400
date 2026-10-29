// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser: a hand-written lexer ([`token`], [`word`]) feeding a
//! recursive-descent grammar ([`command`]). The shell parses one
//! complete command at a time so aliases defined by a command apply to
//! the next.

mod command;
mod heredoc;
mod input;
mod token;
mod word;

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use super::ast::{HereDoc, List, Span, Word};
use super::error::{ParseError, ParseErrorKind};
use input::Input;
use token::Tok;

/// Deepest nesting of commands and words the parser accepts.
const MAX_DEPTH: u32 = 100;

/// A here-document waiting for its body.
#[derive(Debug)]
struct PendingHere {
    delim: String,
    strip_tabs: bool,
    expand: bool,
    cell: Arc<OnceLock<HereDoc>>,
}

/// The shell parser over one source text.
#[derive(Debug)]
pub struct Parser<'a> {
    inp: Input,
    aliases: Option<&'a BTreeMap<String, String>>,
    peeked: Option<(Tok, Span)>,
    heres: Vec<PendingHere>,
    depth: u32,
    /// The input is all there is: a missing here-document delimiter
    /// ends the body instead of asking for more.
    final_input: bool,
    /// The last alias ended with a blank: check the next word too.
    alias_next: bool,
}

impl<'a> Parser<'a> {
    /// A parser over `src`, complete input, no aliases.
    pub fn new(src: &str) -> Self {
        Self::at_line(src, 1)
    }

    /// A parser whose first line is numbered `line`.
    pub fn at_line(src: &str, line: u32) -> Self {
        Self {
            inp: Input::new(src, line),
            aliases: None,
            peeked: None,
            heres: Vec::new(),
            depth: 0,
            final_input: true,
            alias_next: false,
        }
    }

    /// Expand these aliases in command position.
    #[must_use]
    pub fn with_aliases(mut self, aliases: &'a BTreeMap<String, String>) -> Self {
        self.aliases = Some(aliases);
        self
    }

    /// More input may follow (interactive): end of input inside a
    /// construct, here-documents included, is reported as incomplete.
    #[must_use]
    pub fn partial(mut self) -> Self {
        self.final_input = false;
        self
    }

    /// Replace the alias table between commands.
    pub fn set_aliases(&mut self, aliases: &'a BTreeMap<String, String>) {
        self.aliases = Some(aliases);
    }

    /// The next complete command, `None` at end of input.
    pub fn next_command(&mut self) -> Result<Option<List>, ParseError> {
        self.complete_command()
    }

    /// The whole input as one list.
    pub fn parse_all(&mut self) -> Result<List, ParseError> {
        let mut all = List::default();
        while let Some(list) = self.complete_command()? {
            all.items.extend(list.items);
        }
        Ok(all)
    }

    /// The current line number.
    pub fn line(&self) -> u32 {
        self.peeked.as_ref().map_or(self.inp.line, |(_, s)| s.line)
    }

    /// The input not yet consumed (after the last complete command).
    pub fn remaining(&self) -> String {
        let pos = self
            .peeked
            .as_ref()
            .map_or(self.inp.pos, |(_, s)| s.start as usize);
        self.inp.text(pos, self.inp.chars.len())
    }

    fn err(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            line: self.inp.line,
            kind,
        }
    }

    fn enter(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err(ParseErrorKind::TooDeep));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn span_here(&self) -> Span {
        let p = u32::try_from(self.inp.pos).unwrap_or(u32::MAX);
        Span {
            start: p,
            end: p,
            line: self.inp.line,
        }
    }
}

/// Parse a whole script.
pub fn parse(src: &str) -> Result<List, ParseError> {
    Parser::new(src).parse_all()
}

/// Parse text as an expanding here-document body or any other text in
/// which only `$`, backquote and backslash are special.
pub fn parse_here_text(src: &str) -> Result<Word, ParseError> {
    let mut p = Parser::new(src);
    let parts = p.read_parts(word::Mode::Here)?;
    Ok(Word {
        parts,
        span: Span::default(),
    })
}

/// `s` is a POSIX name: a letter or underscore, then letters, digits
/// and underscores.
pub fn is_name(s: &str) -> bool {
    let mut it = s.chars();
    matches!(it.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && it.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

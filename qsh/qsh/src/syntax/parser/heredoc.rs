// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Here-documents: the delimiter rule and reading bodies when a line
//! ends.

use super::{Parser, PendingHere};
use crate::syntax::ast::{HereDoc, Span, Word, WordPart};
use crate::syntax::error::{ParseError, ParseErrorKind};

/// The delimiter with quotes removed, and whether the body expands
/// (no quoting anywhere in the delimiter word).
pub(super) fn delimiter(raw: &str) -> (String, bool) {
    let mut out = String::new();
    let mut quoted = false;
    let mut it = raw.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\'' => {
                quoted = true;
                for c in it.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    out.push(c);
                }
            }
            '"' => {
                quoted = true;
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' if matches!(it.peek(), Some('"' | '\\' | '$' | '`')) => {
                            out.push(it.next().expect("peeked"));
                        }
                        _ => out.push(c),
                    }
                }
            }
            '\\' => {
                quoted = true;
                if let Some(c) = it.next() {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    (out, !quoted)
}

impl Parser<'_> {
    /// Read the bodies of every pending here-document, in order.
    pub(super) fn read_heres(&mut self) -> Result<(), ParseError> {
        for h in std::mem::take(&mut self.heres) {
            let body = self.read_here_body(&h)?;
            let word = if h.expand {
                let mut sub = Parser::at_line(&body, self.inp.line);
                sub.depth = self.depth;
                let parts = sub.read_parts(super::word::Mode::Here)?;
                Word {
                    parts,
                    span: Span::default(),
                }
            } else {
                Word {
                    parts: vec![WordPart::Quoted(body)],
                    span: Span::default(),
                }
            };
            let _ = h.cell.set(HereDoc { body: word });
        }
        Ok(())
    }

    fn read_here_body(&mut self, h: &PendingHere) -> Result<String, ParseError> {
        let mut body = String::new();
        loop {
            if self.inp.raw().is_none() {
                if self.final_input {
                    return Ok(body);
                }
                return Err(self.err(ParseErrorKind::Eof("a here-document")));
            }
            let mut line = String::new();
            while let Some(c) = self.inp.bump() {
                if c == '\n' {
                    break;
                }
                line.push(c);
            }
            let text = if h.strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line.as_str()
            };
            if text == h.delim {
                return Ok(body);
            }
            body.push_str(text);
            body.push('\n');
        }
    }
}

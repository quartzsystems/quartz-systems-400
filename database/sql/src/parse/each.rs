// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A script one statement at a time, for an editor: a statement that
//! does not parse is reported and skipped to the next `;`, and the rest
//! still parse.

use super::Parser;
use crate::ast::Statement;
use crate::error::Result;
use crate::lex::{Tok, Token};

/// One statement of a script and the tokens it took.
#[derive(Debug)]
pub struct Parsed {
    /// The statement, or why it did not parse.
    pub result: Result<Statement>,
    /// Its first token, an index into [`Parser::tokens`].
    pub first: usize,
    /// Its last token (the `;` excluded).
    pub last: usize,
}

impl Parser {
    /// Every token, the end marker last.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.toks
    }

    /// Each `;`-separated statement on its own.
    pub fn statements_each(&mut self) -> Vec<Parsed> {
        let mut out = Vec::new();
        while !self.at_end() {
            if self.eat_sym(";") {
                continue;
            }
            let first = self.at;
            let mut result = self.checked_body();
            if result.is_ok() && !self.at_end() && !self.is_sym(";") {
                result = Err(self.expected(";"));
            }
            if result.is_err() {
                self.refused = None;
                self.skip_locked = false;
                self.depth = 0;
                // Past the token in error to the next `;`.
                if self.at == first {
                    self.advance();
                }
                while !self.at_end() && !self.is_sym(";") {
                    self.advance();
                }
            }
            let last = self.at.saturating_sub(1).max(first);
            out.push(Parsed {
                result,
                first,
                last,
            });
            if matches!(self.tok(), Tok::End) {
                break;
            }
        }
        out
    }
}

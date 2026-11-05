// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The clauses of objects and names: `LIKE` copy options, `DROP SCHEMA`,
//! `GRANT` and `REVOKE`.

use super::Parser;
use crate::error::Result;

impl Parser {
    /// The byte offset of token `i` in the statement text (the end for
    /// the end token).
    fn offset_of(&self, i: usize) -> usize {
        let Some(t) = self.toks.get(i).filter(|t| !matches!(t.tok, crate::lex::Tok::End)) else {
            return self.src.len();
        };
        let (mut line, mut col) = (1u32, 1u32);
        for (at, ch) in self.src.char_indices() {
            if line == t.line && col == t.col {
                return at;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        self.src.len()
    }

    /// Where the parse is now, for [`Parser::text_from`].
    pub(crate) fn mark(&self) -> usize {
        self.at
    }

    /// The text from the token at `mark` up to the current one, as
    /// written, trimmed.
    pub(crate) fn text_from(&self, mark: usize) -> String {
        let (a, b) = (self.offset_of(mark), self.offset_of(self.at));
        self.src.get(a..b.max(a)).unwrap_or("").trim().trim_end_matches(';').trim().to_string()
    }

    /// The copy options after `LIKE` or `AS (query)`: identity is never
    /// copied, column defaults always are.
    pub(crate) fn copy_options(&mut self) -> Result<()> {
        loop {
            if self.eat_words(&["EXCLUDING", "IDENTITY"]) {
                self.eat_words(&["COLUMN", "ATTRIBUTES"]);
            } else if self.eat_words(&["INCLUDING", "COLUMN", "DEFAULTS"]) {
            } else if self.is_words(&["INCLUDING", "IDENTITY"]) {
                return Err(Self::unsupported("INCLUDING IDENTITY: the identity attribute is not copied"));
            } else if self.is_words(&["EXCLUDING", "COLUMN", "DEFAULTS"])
                || self.is_words(&["USING", "TYPE", "DEFAULTS"])
            {
                return Err(Self::unsupported("EXCLUDING COLUMN DEFAULTS and USING TYPE DEFAULTS: column defaults are copied"));
            } else if self.is_word("INCLUDING") || self.is_word("EXCLUDING") {
                return Err(Self::unsupported("that copy option"));
            } else {
                return Ok(());
            }
        }
    }
}

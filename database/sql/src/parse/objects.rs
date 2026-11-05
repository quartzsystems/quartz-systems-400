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
    pub(crate) fn offset_of(&self, i: usize) -> usize {
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

    /// `GRANT ... TO` (`revoke` false) or `REVOKE ... FROM`.
    pub(crate) fn grant(&mut self, revoke: bool) -> Result<crate::ast::Statement> {
        use crate::ast::{Grant, GrantObject, Grantee, Privilege, Statement};
        use crate::lex::Tok;
        self.advance();
        let mut g = Grant {
            all: false,
            privileges: Vec::new(),
            object: GrantObject::Tables(Vec::new()),
            grantees: Vec::new(),
            grant_option: false,
        };
        if self.eat_word("ALL") {
            self.eat_word("PRIVILEGES");
            g.all = true;
        } else {
            loop {
                let Tok::Word(w) = self.tok().clone() else {
                    return Err(self.expected("a privilege"));
                };
                if w == "ON" {
                    return Err(self.expected("a privilege"));
                }
                self.advance();
                let columns = if self.is_sym("(") { self.ident_list()? } else { Vec::new() };
                g.privileges.push(Privilege { name: w, columns });
                if !self.eat_sym(",") {
                    break;
                }
            }
        }
        self.expect_word("ON")?;
        let kind = if self.eat_word("TABLE") {
            None
        } else if self.eat_words(&["SPECIFIC", "PROCEDURE"]) {
            Some("SPECIFIC PROCEDURE")
        } else if self.eat_words(&["SPECIFIC", "FUNCTION"]) {
            Some("SPECIFIC FUNCTION")
        } else if self.eat_words(&["DISTINCT", "TYPE"]) {
            Some("TYPE")
        } else {
            ["SCHEMA", "PROCEDURE", "FUNCTION", "SEQUENCE", "VARIABLE", "TYPE", "PACKAGE", "XSROBJECT"]
                .into_iter()
                .find(|k| self.eat_word(k))
        };
        let mut names = Vec::new();
        loop {
            names.push(if kind == Some("SCHEMA") { crate::ast::ObjectName { schema: None, name: self.ident()? } } else { self.object_name()? });
            // A routine's signature: kept for the later agents, not used.
            if matches!(kind, Some("PROCEDURE" | "FUNCTION")) && self.eat_sym("(") {
                let mut depth = 1;
                while depth > 0 && !matches!(self.tok(), Tok::End) {
                    if self.is_sym("(") {
                        depth += 1;
                    } else if self.is_sym(")") {
                        depth -= 1;
                    }
                    self.advance();
                }
            }
            if !self.eat_sym(",") {
                break;
            }
        }
        g.object = match kind {
            None => GrantObject::Tables(names),
            Some(k) => GrantObject::Other { kind: k.into(), names },
        };
        self.expect_word(if revoke { "FROM" } else { "TO" })?;
        loop {
            if self.eat_word("PUBLIC") {
                g.grantees.push(Grantee::Public);
            } else {
                self.eat_word("USER");
                g.grantees.push(Grantee::User(self.ident()?));
            }
            if !self.eat_sym(",") {
                break;
            }
        }
        if !revoke && self.eat_words(&["WITH", "GRANT", "OPTION"]) {
            g.grant_option = true;
        }
        Ok(if revoke { Statement::Revoke(g) } else { Statement::Grant(g) })
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

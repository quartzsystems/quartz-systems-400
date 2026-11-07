// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVEPOINT`, `RELEASE [TO] SAVEPOINT`, `ROLLBACK [WORK] TO SAVEPOINT`.

use super::Parser;
use crate::ast::{SavepointStmt, Statement};
use crate::error::Result;

impl Parser {
    /// `SAVEPOINT name [UNIQUE] ON ROLLBACK RETAIN CURSORS [ON ROLLBACK RETAIN LOCKS]`.
    pub(crate) fn savepoint(&mut self) -> Result<Statement> {
        self.expect_word("SAVEPOINT")?;
        let name = self.ident()?;
        let unique = self.eat_word("UNIQUE");
        if unique && self.is_word("UNIQUE") {
            return Err(self.keyword());
        }
        self.expect_words(&["ON", "ROLLBACK", "RETAIN", "CURSORS"])?;
        if self.eat_words(&["ON", "ROLLBACK", "RETAIN"]) {
            self.expect_word("LOCKS")?;
        }
        Ok(Statement::Savepoint(SavepointStmt::Set { name, unique }))
    }

    /// `RELEASE [TO] SAVEPOINT name`.
    pub(crate) fn release_savepoint(&mut self) -> Result<Statement> {
        self.expect_word("RELEASE")?;
        self.eat_word("TO");
        if !self.is_word("SAVEPOINT") {
            return Err(Self::unsupported("RELEASE of a connection"));
        }
        self.advance();
        Ok(Statement::Savepoint(SavepointStmt::Release(self.ident()?)))
    }

    /// After `ROLLBACK [WORK] TO`: `SAVEPOINT [name]`.
    pub(crate) fn rollback_to(&mut self) -> Result<Statement> {
        self.expect_word("SAVEPOINT")?;
        let name = if self.at_end() || self.is_sym(";") {
            None
        } else {
            Some(self.ident()?)
        };
        Ok(Statement::Savepoint(SavepointStmt::RollbackTo(name)))
    }
}

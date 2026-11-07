// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DECLARE GLOBAL TEMPORARY TABLE`: the table as `CREATE TABLE` reads
//! it, then the temporary table's own clauses in any order.

use super::Parser;
use crate::ast::{DeclareTemp, Statement};
use crate::error::Result;

impl Parser {
    /// After `DECLARE GLOBAL TEMPORARY`.
    pub(crate) fn declare_temp(&mut self) -> Result<Statement> {
        self.expect_word("TABLE")?;
        let Statement::CreateTable(table) = self.create_table(false)? else {
            unreachable!("create_table makes a CREATE TABLE");
        };
        let mut d = DeclareTemp {
            table,
            delete_rows: true,
            not_logged: false,
            rollback_preserve: false,
            replace: false,
        };
        let mut on_commit = false;
        loop {
            if self.is_words(&["ON", "COMMIT"]) {
                if on_commit {
                    return Err(self.expected("the end of the statement"));
                }
                self.at += 2;
                on_commit = true;
                d.delete_rows = !self.eat_word("PRESERVE");
                if d.delete_rows {
                    self.expect_word("DELETE")?;
                }
                self.expect_word("ROWS")?;
            } else if self.eat_words(&["NOT", "LOGGED"]) {
                d.not_logged = true;
                if self.eat_words(&["ON", "ROLLBACK"]) {
                    d.rollback_preserve = self.eat_word("PRESERVE");
                    if !d.rollback_preserve {
                        self.expect_word("DELETE")?;
                    }
                    self.expect_word("ROWS")?;
                }
            } else if self.eat_words(&["WITH", "REPLACE"]) {
                d.replace = true;
            } else if self.eat_word("RCDFMT") {
                d.table.rcdfmt = Some(self.ident()?);
            } else if self.is_word("LOGGED")
                || self.is_word("CCSID")
                || self.is_words(&["ON", "ROLLBACK"])
            {
                // Db2 for i has no such clause here (`SQL0199` on the primary).
                return Err(self.keyword());
            } else {
                break;
            }
        }
        Ok(Statement::DeclareTemp(Box::new(d)))
    }
}

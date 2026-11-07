// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `TRUNCATE [TABLE] t [DROP|REUSE STORAGE] [IGNORE|RESTRICT WHEN DELETE
//! TRIGGERS] [CONTINUE|RESTART IDENTITY] [IMMEDIATE]`.

use super::Parser;
use crate::ast::{Statement, Truncate};
use crate::error::Result;

impl Parser {
    pub(crate) fn truncate(&mut self) -> Result<Statement> {
        self.expect_word("TRUNCATE")?;
        self.eat_word("TABLE");
        let table = self.object_name()?;
        let mut t = Truncate {
            table,
            reuse: false,
            restrict_triggers: false,
            restart_identity: false,
        };
        loop {
            if self.eat_words(&["DROP", "STORAGE"]) {
                t.reuse = false;
            } else if self.eat_words(&["REUSE", "STORAGE"]) {
                t.reuse = true;
            } else if self.eat_words(&["IGNORE", "DELETE", "TRIGGERS"]) {
                t.restrict_triggers = false;
            } else if self.eat_words(&["RESTRICT", "WHEN", "DELETE", "TRIGGERS"]) {
                t.restrict_triggers = true;
            } else if self.eat_words(&["CONTINUE", "IDENTITY"]) {
                t.restart_identity = false;
            } else if self.eat_words(&["RESTART", "IDENTITY"]) {
                t.restart_identity = true;
            } else if self.eat_word("IMMEDIATE") {
            } else {
                return Ok(Statement::Truncate(t));
            }
        }
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE TRIGGER` and `DROP TRIGGER`.

use super::Parser;
use crate::ast::{CreateTrigger, Statement, TriggerEvent, TriggerTime};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    /// `CREATE [OR REPLACE] TRIGGER`, after `OR REPLACE`.
    pub(crate) fn create_trigger(&mut self, replace: bool) -> Result<Statement> {
        self.expect_word("TRIGGER")?;
        let name = self.object_name()?;
        let time = if self.eat_words(&["NO", "CASCADE", "BEFORE"]) || self.eat_word("BEFORE") {
            TriggerTime::Before
        } else if self.eat_word("AFTER") {
            TriggerTime::After
        } else if self.eat_words(&["INSTEAD", "OF"]) {
            TriggerTime::InsteadOf
        } else {
            return Err(self.expected("BEFORE, AFTER or INSTEAD OF"));
        };
        let event = if self.eat_word("INSERT") {
            TriggerEvent::Insert
        } else if self.eat_word("DELETE") {
            TriggerEvent::Delete
        } else if self.eat_word("UPDATE") {
            let mut cols = Vec::new();
            if self.eat_word("OF") {
                cols.push(self.ident()?);
                while self.eat_sym(",") {
                    cols.push(self.ident()?);
                }
            }
            TriggerEvent::Update(cols)
        } else {
            return Err(self.expected("INSERT, DELETE or UPDATE"));
        };
        if self.is_word("OR") {
            // BLOCKED(sql): one trigger for several events, with the
            // INSERTING, UPDATING and DELETING predicates (7.2).
            return Err(Self::unsupported("a trigger for more than one event (INSERT OR UPDATE ...)"));
        }
        self.expect_word("ON")?;
        let table = self.object_name()?;
        let mut t = CreateTrigger {
            replace,
            name,
            time,
            event,
            table,
            old_row: None,
            new_row: None,
            old_table: None,
            new_table: None,
            statement: false,
            db2sql: None,
            options: Vec::new(),
            when: None,
            body: Box::new(crate::ast::PlStmt { label: None, line: 0, kind: crate::ast::PlKind::Leave(crate::ast::Ident { value: String::new(), delimited: false }) }),
            body_text: String::new(),
        };
        if self.eat_word("REFERENCING") {
            self.referencing(&mut t)?;
        }
        loop {
            if self.eat_words(&["FOR", "EACH", "ROW"]) {
                t.statement = false;
            } else if self.eat_words(&["FOR", "EACH", "STATEMENT"]) {
                t.statement = true;
            } else if self.eat_words(&["MODE", "DB2ROW"]) {
                t.db2sql = Some(false);
            } else if self.eat_words(&["MODE", "DB2SQL"]) {
                t.db2sql = Some(true);
            } else if self.eat_words(&["NOT", "SECURED"]) {
            } else if self.is_word("SECURED") {
                return Err(Self::unsupported("SECURED triggers (row and column access control)"));
            } else if self.eat_words(&["SET", "OPTION"]) {
                loop {
                    let Tok::Word(k) = self.advance() else { return Err(self.expected("an option")) };
                    self.expect_sym("=")?;
                    let m = self.mark();
                    self.eat_sym("*");
                    self.advance();
                    t.options.push((k, self.text_from(m)));
                    if !self.eat_sym(",") {
                        break;
                    }
                }
            } else {
                break;
            }
        }
        if self.eat_word("WHEN") {
            self.expect_sym("(")?;
            t.when = Some(self.pl_expr()?);
            self.expect_sym(")")?;
        }
        if self.at_end() || self.is_sym(";") {
            return Err(self.expected("the triggered statement"));
        }
        let m = self.mark();
        t.body = Box::new(self.pl_stmt()?);
        t.body_text = self.text_from(m);
        if !self.at_end() && !self.is_sym(";") {
            return Err(self.expected("end of statement"));
        }
        Ok(Statement::CreateTrigger(Box::new(t)))
    }

    /// `OLD [ROW] [AS] name`, `NEW [ROW] [AS] name`, `OLD TABLE [AS] name`,
    /// `NEW TABLE [AS] name` (`OLD_TABLE`, `NEW_TABLE`), in any order.
    fn referencing(&mut self, t: &mut CreateTrigger) -> Result<()> {
        let mut any = false;
        loop {
            let (old, table) = if self.eat_word("OLD_TABLE") {
                (true, true)
            } else if self.eat_word("NEW_TABLE") {
                (false, true)
            } else if self.eat_word("OLD") {
                (true, self.eat_word("TABLE") || { self.eat_word("ROW"); false })
            } else if self.eat_word("NEW") {
                (false, self.eat_word("TABLE") || { self.eat_word("ROW"); false })
            } else {
                break;
            };
            self.eat_word("AS");
            let n = self.ident()?;
            let slot = match (old, table) {
                (true, false) => &mut t.old_row,
                (false, false) => &mut t.new_row,
                (true, true) => &mut t.old_table,
                (false, true) => &mut t.new_table,
            };
            if slot.is_some() {
                return Err(crate::error::SqlError::Coded {
                    code: -104,
                    state: "42601",
                    what: format!("{} is the second name of the same kind in REFERENCING", n.value),
                });
            }
            *slot = Some(n);
            any = true;
        }
        if !any {
            return Err(self.expected("OLD, NEW, OLD TABLE or NEW TABLE"));
        }
        Ok(())
    }
}

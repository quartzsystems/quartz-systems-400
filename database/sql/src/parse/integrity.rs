// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Constraints, identity options, generated columns and `ALTER TABLE`.

use super::Parser;
use crate::ast::{
    AlterAction, AlterTable, ColumnDef, Constraint, DropKind, Generated, Ident, Identity, RefRule, Statement,
};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    pub(crate) fn starts_constraint(&self) -> bool {
        self.is_word("CONSTRAINT")
            || self.is_words(&["PRIMARY", "KEY"])
            || self.is_word("UNIQUE") && matches!(self.tok_at(1), Tok::Sym("("))
            || self.is_words(&["FOREIGN", "KEY"])
            || self.is_word("CHECK") && matches!(self.tok_at(1), Tok::Sym("("))
    }

    /// A table constraint.
    pub(crate) fn constraint(&mut self) -> Result<Constraint> {
        let name = if self.eat_word("CONSTRAINT") { Some(self.ident()?) } else { None };
        let c = if self.eat_words(&["PRIMARY", "KEY"]) {
            Constraint::PrimaryKey { name, columns: self.ident_list()? }
        } else if self.eat_word("UNIQUE") {
            Constraint::Unique { name, columns: self.ident_list()? }
        } else if self.eat_words(&["FOREIGN", "KEY"]) {
            let columns = self.ident_list()?;
            self.expect_word("REFERENCES")?;
            self.references(name, columns)?
        } else if self.is_word("CHECK") {
            self.advance();
            self.check(name)?
        } else {
            return Err(self.expected("PRIMARY KEY, UNIQUE, FOREIGN KEY or CHECK"));
        };
        self.constraint_attributes()?;
        Ok(c)
    }

    /// `CHECK (condition)`, the `CHECK` read.
    fn check(&mut self, name: Option<Ident>) -> Result<Constraint> {
        self.expect_sym("(")?;
        let from = self.at;
        let cond = self.expr()?;
        let text: String = self.toks[from..self.at].iter().map(|t| t.tok.describe() + " ").collect();
        self.expect_sym(")")?;
        Ok(Constraint::Check { name, cond, text })
    }

    /// `parent [(cols)] [ON DELETE rule] [ON UPDATE rule]`, after `REFERENCES`.
    fn references(&mut self, name: Option<Ident>, columns: Vec<Ident>) -> Result<Constraint> {
        let parent = self.object_name()?;
        let parent_columns = if self.is_sym("(") { self.ident_list()? } else { Vec::new() };
        let (mut on_delete, mut on_update) = (RefRule::NoAction, RefRule::NoAction);
        while self.eat_word("ON") {
            if self.eat_word("DELETE") {
                on_delete = if self.eat_words(&["NO", "ACTION"]) {
                    RefRule::NoAction
                } else if self.eat_word("RESTRICT") {
                    RefRule::Restrict
                } else if self.eat_word("CASCADE") {
                    RefRule::Cascade
                } else if self.eat_words(&["SET", "NULL"]) {
                    RefRule::SetNull
                } else if self.eat_words(&["SET", "DEFAULT"]) {
                    RefRule::SetDefault
                } else {
                    return Err(self.expected("NO ACTION, RESTRICT, CASCADE, SET NULL or SET DEFAULT"));
                };
            } else {
                self.expect_word("UPDATE")?;
                // IBM's update rules are NO ACTION and RESTRICT only.
                on_update = if self.eat_words(&["NO", "ACTION"]) {
                    RefRule::NoAction
                } else if self.eat_word("RESTRICT") {
                    RefRule::Restrict
                } else {
                    return Err(self.expected("RESTRICT"));
                };
            }
        }
        Ok(Constraint::ForeignKey { name, columns, parent, parent_columns, on_delete, on_update })
    }

    /// What may follow a constraint: refused unless it says what IBM's
    /// constraints are anyway.
    fn constraint_attributes(&mut self) -> Result<()> {
        loop {
            if self.eat_words(&["NOT", "DEFERRABLE"]) || self.eat_words(&["INITIALLY", "IMMEDIATE"]) || self.eat_word("ENFORCED") {
                continue;
            }
            if self.is_words(&["NOT", "ENFORCED"]) || self.is_word("DEFERRABLE") || self.is_words(&["INITIALLY", "DEFERRED"]) {
                return Err(Self::unsupported("informational (NOT ENFORCED) and deferrable constraints"));
            }
            if self.is_words(&["ENABLE", "QUERY", "OPTIMIZATION"]) || self.is_words(&["DISABLE", "QUERY", "OPTIMIZATION"]) {
                return Err(Self::unsupported("ENABLE/DISABLE QUERY OPTIMIZATION on a constraint"));
            }
            return Ok(());
        }
    }

    /// A column's own constraint, when one starts here.
    pub(crate) fn column_constraint(&mut self, col: &ColumnDef) -> Result<Option<Constraint>> {
        let named = self.is_word("CONSTRAINT");
        let name = if named {
            self.advance();
            Some(self.ident()?)
        } else {
            None
        };
        let cols = vec![col.name.clone()];
        let c = if self.eat_words(&["PRIMARY", "KEY"]) {
            Constraint::PrimaryKey { name, columns: cols }
        } else if self.eat_word("UNIQUE") {
            Constraint::Unique { name, columns: cols }
        } else if self.eat_word("CHECK") {
            self.check(name)?
        } else if self.eat_word("REFERENCES") {
            self.references(name, cols)?
        } else if named {
            return Err(self.expected("PRIMARY KEY, UNIQUE, CHECK or REFERENCES"));
        } else {
            return Ok(None);
        };
        self.constraint_attributes()?;
        Ok(Some(c))
    }

    /// After `GENERATED`: `ALWAYS | BY DEFAULT` then `AS IDENTITY [(...)]`,
    /// `FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP` or `AS (...)`.
    pub(crate) fn generated(&mut self, c: &mut ColumnDef) -> Result<()> {
        let always = if self.eat_word("ALWAYS") {
            true
        } else {
            self.expect_words(&["BY", "DEFAULT"])?;
            false
        };
        if self.eat_words(&["FOR", "EACH", "ROW", "ON", "UPDATE", "AS", "ROW", "CHANGE", "TIMESTAMP"]) {
            c.generated = Some(Generated::RowChangeTimestamp { always });
            return Ok(());
        }
        self.expect_word("AS")?;
        if self.eat_word("IDENTITY") {
            let mut id = Identity { always, ..Identity::default() };
            if self.eat_sym("(") {
                self.identity_options(&mut id, false)?;
                self.expect_sym(")")?;
            }
            c.identity = Some(id);
            return Ok(());
        }
        if self.eat_sym("(") {
            // The auditing columns of 7.3 and later.
            let what = if self.eat_words(&["DATA", "CHANGE", "OPERATION"]) {
                Some("DATA CHANGE OPERATION".to_string())
            } else {
                let audit = [
                    "SESSION_USER",
                    "USER",
                    "CURRENT_USER",
                    "SYSTEM_USER",
                    "CURRENT_SERVER",
                    "CURRENT SERVER",
                ];
                let mut found = None;
                for w in audit {
                    let parts: Vec<&str> = w.split(' ').collect();
                    if self.is_words(&parts) && matches!(self.tok_at(parts.len()), Tok::Sym(")")) {
                        self.eat_words(&parts);
                        found = Some(w.to_string());
                        break;
                    }
                }
                found
            };
            if let Some(what) = what {
                self.expect_sym(")")?;
                c.generated = Some(Generated::Audit { always, what });
                return Ok(());
            }
            let e = self.expr()?;
            self.expect_sym(")")?;
            c.generated = Some(Generated::Expr(Box::new(e)));
            return Ok(());
        }
        Err(self.expected("IDENTITY, ( or FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP"))
    }

    /// Identity or sequence options, commas between them optional, until
    /// one does not start; `alter` takes `RESTART`.
    pub(crate) fn identity_options(&mut self, id: &mut Identity, alter: bool) -> Result<()> {
        loop {
            if self.eat_words(&["START", "WITH"]) {
                id.start = Some(self.big_integer()?);
            } else if self.eat_words(&["INCREMENT", "BY"]) {
                id.increment = Some(self.big_integer()?);
            } else if self.eat_words(&["NO", "MINVALUE"]) {
                id.min = Some(None);
            } else if self.eat_word("MINVALUE") {
                id.min = Some(Some(self.big_integer()?));
            } else if self.eat_words(&["NO", "MAXVALUE"]) {
                id.max = Some(None);
            } else if self.eat_word("MAXVALUE") {
                id.max = Some(Some(self.big_integer()?));
            } else if self.eat_words(&["NO", "CYCLE"]) || self.eat_word("NOCYCLE") {
                id.cycle = Some(false);
            } else if self.eat_word("CYCLE") {
                id.cycle = Some(true);
            } else if self.eat_words(&["NO", "CACHE"]) || self.eat_word("NOCACHE") {
                id.cache = Some(0);
            } else if self.eat_word("CACHE") {
                let n = self.integer()?;
                id.cache = Some(u32::try_from(n).unwrap_or(u32::MAX));
            } else if self.eat_words(&["NO", "ORDER"]) || self.eat_word("NOORDER") {
                id.order = Some(false);
            } else if self.eat_word("ORDER") {
                id.order = Some(true);
            } else if alter && self.eat_word("RESTART") {
                id.restart = Some(if self.eat_word("WITH") { Some(self.big_integer()?) } else { None });
            } else {
                return Ok(());
            }
            self.eat_sym(",");
        }
    }

    /// A signed integer of up to 38 digits.
    fn big_integer(&mut self) -> Result<i128> {
        let neg = if self.eat_sym("-") {
            true
        } else {
            self.eat_sym("+");
            false
        };
        match self.tok().clone() {
            Tok::Num(n) if n.chars().all(|c| c.is_ascii_digit()) => {
                self.advance();
                let v: i128 = n.parse().map_err(|_| self.expected("a smaller integer"))?;
                Ok(if neg { -v } else { v })
            }
            _ => Err(self.expected("an integer")),
        }
    }

    pub(crate) fn alter(&mut self) -> Result<Statement> {
        self.expect_word("ALTER")?;
        if self.eat_word("SEQUENCE") {
            return self.alter_sequence();
        }
        self.expect_word("TABLE")?;
        let name = self.object_name()?;
        let mut actions = Vec::new();
        loop {
            if self.eat_word("ADD") {
                if self.is_word("PARTITION") || self.is_words(&["PARTITIONING", "KEY"]) || self.is_word("MATERIALIZED") {
                    return Err(Self::unsupported("partitioned and materialized query tables"));
                }
                if self.starts_constraint() {
                    actions.push(AlterAction::AddConstraint(self.constraint()?));
                } else {
                    self.eat_word("COLUMN");
                    actions.push(AlterAction::AddColumn(self.column_def()?));
                }
            } else if self.eat_word("DROP") {
                if self.is_word("PARTITION") || self.is_word("PARTITIONING") || self.is_word("MATERIALIZED") {
                    return Err(Self::unsupported("partitioned and materialized query tables"));
                }
                let cascade = |p: &mut Self| {
                    if p.eat_word("RESTRICT") {
                        false
                    } else {
                        p.eat_word("CASCADE");
                        true
                    }
                };
                if self.eat_words(&["PRIMARY", "KEY"]) {
                    let cascade = cascade(self);
                    actions.push(AlterAction::DropPrimaryKey { cascade });
                } else if self.is_word("CONSTRAINT") || self.is_word("UNIQUE") || self.is_words(&["FOREIGN", "KEY"]) || self.is_word("CHECK") {
                    let kind = if self.eat_word("CONSTRAINT") {
                        DropKind::Any
                    } else if self.eat_word("UNIQUE") {
                        DropKind::Unique
                    } else if self.eat_word("CHECK") {
                        DropKind::Check
                    } else {
                        self.expect_words(&["FOREIGN", "KEY"])?;
                        DropKind::ForeignKey
                    };
                    let name = self.ident()?;
                    let cascade = cascade(self);
                    actions.push(AlterAction::DropConstraint { name, kind, cascade });
                } else {
                    self.eat_word("COLUMN");
                    let col = self.ident()?;
                    let cascade = cascade(self);
                    actions.push(AlterAction::DropColumn { name: col, cascade });
                }
            } else if self.eat_word("ALTER") {
                self.eat_word("COLUMN");
                let col = self.ident()?;
                actions.push(self.alter_column(col)?);
            } else {
                break;
            }
            self.eat_sym(",");
        }
        if actions.is_empty() {
            if self.is_words(&["VOLATILE"]) || self.is_words(&["NOT", "VOLATILE"]) || self.is_word("APPEND") || self.is_word("ACTIVATE") || self.is_word("DEACTIVATE") {
                return Err(Self::unsupported("that ALTER TABLE attribute"));
            }
            return Err(self.expected("ADD, DROP or ALTER"));
        }
        Ok(Statement::AlterTable(AlterTable { name, actions }))
    }

    /// One `ALTER COLUMN` change, the column read.
    fn alter_column(&mut self, col: Ident) -> Result<AlterAction> {
        if self.eat_words(&["SET", "DATA", "TYPE"]) {
            return Ok(AlterAction::SetDataType(col, self.type_spec()?));
        }
        if self.eat_words(&["SET", "NOT", "NULL"]) {
            return Ok(AlterAction::SetNotNull(col, true));
        }
        if self.eat_words(&["DROP", "NOT", "NULL"]) {
            return Ok(AlterAction::SetNotNull(col, false));
        }
        if self.eat_words(&["SET", "DEFAULT"]) || self.eat_words(&["SET", "WITH", "DEFAULT"]) {
            return Ok(AlterAction::SetDefault(col, Some(self.default_value()?)));
        }
        if self.eat_words(&["DROP", "DEFAULT"]) {
            return Ok(AlterAction::SetDefault(col, None));
        }
        if self.eat_words(&["DROP", "IDENTITY"]) {
            return Ok(AlterAction::DropIdentity(col));
        }
        if self.eat_words(&["DROP", "GENERATED"]) || self.eat_words(&["DROP", "ROW", "CHANGE", "TIMESTAMP"]) {
            return Ok(AlterAction::DropGenerated(col));
        }
        if self.is_word("SET") && (self.is_words(&["SET", "IMPLICITLY", "HIDDEN"]) || self.is_words(&["SET", "NOT", "HIDDEN"])) {
            return Err(Self::unsupported("changing whether a column is hidden"));
        }
        // SET GENERATED ALWAYS|BY DEFAULT [AS IDENTITY], SET INCREMENT BY ...,
        // RESTART [WITH n]: one or more identity options.
        let mut id = Identity::default();
        let mut always = None;
        let mut any = false;
        loop {
            if self.eat_words(&["SET", "GENERATED"]) {
                always = Some(if self.eat_word("ALWAYS") {
                    true
                } else {
                    self.expect_words(&["BY", "DEFAULT"])?;
                    false
                });
                if self.eat_words(&["AS", "IDENTITY"]) && self.eat_sym("(") {
                    self.identity_options(&mut id, false)?;
                    self.expect_sym(")")?;
                }
                any = true;
                continue;
            }
            let before = self.at;
            if self.eat_word("SET") {
                self.identity_options(&mut id, false)?;
            } else {
                self.identity_options(&mut id, true)?;
            }
            if self.at == before {
                break;
            }
            any = true;
        }
        if !any {
            return Err(self.expected("SET DATA TYPE, SET/DROP DEFAULT, SET/DROP NOT NULL, SET GENERATED, DROP IDENTITY or RESTART"));
        }
        Ok(AlterAction::AlterIdentity { column: col, always, options: id })
    }
}

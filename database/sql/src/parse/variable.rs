// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE VARIABLE`, `SET` of global variables and `CREATE TYPE` of a
//! distinct type.

use super::Parser;
use crate::ast::{CreateType, CreateVariable, Expr, Literal, ObjectName, Statement};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    /// After `CREATE [OR REPLACE] VARIABLE`.
    pub(crate) fn create_variable(&mut self, replace: bool) -> Result<Statement> {
        let name = self.object_name()?;
        if self.is_words(&["FOR", "SYSTEM"]) {
            return Err(Self::unsupported("CREATE VARIABLE FOR SYSTEM NAME"));
        }
        if self.is_word("ARRAY") || self.is_sym_at(1, ".") {
            return Err(Self::unsupported("a global variable of a user-defined or array type"));
        }
        let data_type = self.type_spec()?;
        let mut default = None;
        let has_default = self.eat_word("DEFAULT");
        let constant = !has_default && self.eat_word("CONSTANT");
        if has_default || constant {
            let mark = self.mark();
            let e = self.expr()?;
            default = Some((e, self.text_from(mark)));
        }
        Ok(Statement::CreateVariable(CreateVariable { name, data_type, default, constant, replace }))
    }

    /// `SET` of global variables, after `SET`: `v = e [, v = e]...` or
    /// `(v, ...) = (e, ...)`. `None` when what follows is not that.
    pub(crate) fn set_variables(&mut self) -> Result<Option<Statement>> {
        let mut targets = Vec::new();
        let mut values = Vec::new();
        if self.eat_sym("(") {
            loop {
                targets.push(self.object_name()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
            self.expect_sym("=")?;
            self.expect_sym("(")?;
            loop {
                values.push(self.assigned()?);
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
            if targets.len() != values.len() {
                return Err(crate::error::SqlError::Coded {
                    code: -117,
                    state: "42802",
                    what: "the number of values differs from the number of variables".into(),
                });
            }
            return Ok(Some(Statement::SetVariables { targets, values }));
        }
        if !matches!(self.tok(), Tok::Word(_) | Tok::Delimited(_)) {
            return Ok(None);
        }
        loop {
            targets.push(self.object_name()?);
            self.expect_sym("=")?;
            values.push(self.assigned()?);
            if !self.eat_sym(",") {
                break;
            }
        }
        Ok(Some(Statement::SetVariables { targets, values }))
    }

    /// A value assigned: `DEFAULT`, `NULL` or an expression.
    fn assigned(&mut self) -> Result<Option<Expr>> {
        if self.eat_word("DEFAULT") {
            return Ok(None);
        }
        if self.eat_word("NULL") {
            return Ok(Some(Expr::Literal(Literal::Null)));
        }
        self.expr().map(Some)
    }

    /// After `CREATE [OR REPLACE] TYPE`.
    pub(crate) fn create_type(&mut self, replace: bool) -> Result<Statement> {
        let name: ObjectName = self.object_name()?;
        if replace {
            return Err(Self::unsupported("CREATE OR REPLACE TYPE"));
        }
        self.expect_word("AS")?;
        let source = self.type_spec()?;
        if self.eat_word("ARRAY") {
            self.expect_sym("[")?;
            let max = if self.eat_sym("]") {
                crate::exec::array::UNBOUNDED
            } else {
                let n = match self.tok().clone() {
                    crate::lex::Tok::Num(n) => n.parse::<u64>().ok().filter(|n| (1..=crate::exec::array::UNBOUNDED).contains(n)),
                    _ => None,
                };
                let Some(n) = n else { return Err(self.expected("the largest cardinality, 1 to 2147483647")) };
                self.advance();
                self.expect_sym("]")?;
                n
            };
            return Ok(Statement::CreateType(CreateType { name, source, array: Some(max) }));
        }
        // The comparisons are generated either way, as on IBM i.
        let _ = self.eat_words(&["WITH", "COMPARISONS"]);
        if self.eat_words(&["NOT", "FINAL"]) || self.is_word("UNDER") {
            return Err(Self::unsupported("structured types"));
        }
        Ok(Statement::CreateType(CreateType { name, source, array: None }))
    }
}

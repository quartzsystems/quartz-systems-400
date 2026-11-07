// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arrays: `ARRAY[e, ...]`, `ARRAY[fullselect]`, `ARRAY[]`, an element
//! `e[i]`, `ARRAY_AGG(e ORDER BY ...)` and `UNNEST(a, ...) [WITH
//! ORDINALITY]` in `FROM` (`crate::exec::array`).

use super::Parser;
use crate::ast::{Expr, Function, ObjectName, TableRef, WithinGroup};
use crate::error::Result;

fn func(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Function(Function {
        name: name.into(),
        distinct: false,
        star: false,
        args,
    })
}

impl Parser {
    /// At `ARRAY [`: the constructor.
    pub(crate) fn array_ctor(&mut self) -> Result<Expr> {
        self.expect_word("ARRAY")?;
        self.expect_sym("[")?;
        if self.starts_query() {
            let q = self.query()?;
            self.expect_sym("]")?;
            return Ok(func("*ARRAYQ", vec![Expr::Subquery(Box::new(q))]));
        }
        let mut args = Vec::new();
        if !self.eat_sym("]") {
            args.push(self.expr()?);
            while self.eat_sym(",") {
                args.push(self.expr()?);
            }
            self.expect_sym("]")?;
        }
        Ok(func("*ARRAY", args))
    }

    /// Any `[i]` after a primary.
    pub(crate) fn subscripts(&mut self, mut e: Expr) -> Result<Expr> {
        while self.eat_sym("[") {
            let i = self.expr()?;
            self.expect_sym("]")?;
            e = func("*ELEMENT", vec![e, i]);
        }
        Ok(e)
    }

    /// Inside `ARRAY_AGG(e`: its `ORDER BY`, up to the closing parenthesis.
    pub(crate) fn array_agg_order(&mut self, f: Function) -> Result<Expr> {
        self.expect_words(&["ORDER", "BY"])?;
        let order = self.order_list()?;
        self.expect_sym(")")?;
        Ok(Expr::WithinGroup(Box::new(WithinGroup {
            func: f,
            order,
            overflow: None,
        })))
    }

    /// At `UNNEST (`: the table reference.
    pub(crate) fn unnest(&mut self) -> Result<TableRef> {
        self.expect_word("UNNEST")?;
        self.expect_sym("(")?;
        let mut args = vec![self.expr()?];
        while self.eat_sym(",") {
            args.push(self.expr()?);
        }
        self.expect_sym(")")?;
        let ordinality = self.eat_words(&["WITH", "ORDINALITY"]);
        let (alias, columns) = self.correlation()?;
        let alias = alias.ok_or_else(|| self.expected("a correlation name"))?;
        let name = ObjectName::new(if ordinality { "*UNNESTO" } else { "*UNNEST" });
        Ok(TableRef::Lateral(Box::new(TableRef::Function {
            name,
            args,
            alias,
            columns,
        })))
    }
}

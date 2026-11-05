// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What may follow a function call: `WITHIN GROUP (ORDER BY ...)`,
//! `RESPECT|IGNORE NULLS`, `FROM FIRST|LAST` and `OVER (...)` with its
//! window; `LISTAGG`'s `ON OVERFLOW`; and the grouping elements of
//! `GROUP BY` (`ROLLUP`, `CUBE`, `GROUPING SETS`, `()`).

use super::Parser;
use crate::ast::{Expr, Frame, FrameBound, Function, Over, Overflow, Window, WithinGroup};
use crate::error::Result;
use crate::lex::Tok;

/// The name a `GROUPING SETS (...)` element is kept under.
pub const GROUPING_SETS: &str = "GROUPING SETS";

impl Parser {
    /// `ON OVERFLOW ERROR | TRUNCATE [filler] WITH|WITHOUT COUNT` inside
    /// `LISTAGG (...)`.
    pub(crate) fn listagg_overflow(&mut self, name: &str) -> Result<Option<Overflow>> {
        if name != "LISTAGG" || !self.eat_words(&["ON", "OVERFLOW"]) {
            return Ok(None);
        }
        if self.eat_word("ERROR") {
            return Ok(Some(Overflow::Error));
        }
        self.expect_word("TRUNCATE")?;
        let filler = match self.tok().clone() {
            Tok::Str(s) => {
                self.advance();
                Some(s)
            }
            _ => None,
        };
        let count = if self.eat_word("WITH") {
            true
        } else {
            self.expect_word("WITHOUT")?;
            false
        };
        self.expect_word("COUNT")?;
        Ok(Some(Overflow::Truncate { filler, count }))
    }

    /// The clauses after a call's closing parenthesis.
    pub(crate) fn call_tail(&mut self, f: Function, overflow: Option<Overflow>) -> Result<Expr> {
        let mut e = Expr::Function(f);
        if self.is_words(&["WITHIN", "GROUP"]) {
            self.advance();
            self.advance();
            self.expect_sym("(")?;
            self.expect_words(&["ORDER", "BY"])?;
            let order = self.order_list()?;
            self.expect_sym(")")?;
            let Expr::Function(f) = e else { unreachable!() };
            e = Expr::WithinGroup(Box::new(WithinGroup { func: f, order, overflow }));
        } else if overflow.is_some() {
            let Expr::Function(f) = e else { unreachable!() };
            e = Expr::WithinGroup(Box::new(WithinGroup { func: f, order: Vec::new(), overflow }));
        }
        // NTH_VALUE(x, n) FROM FIRST: the only order there is.
        if self.is_words(&["FROM", "FIRST"]) && self.is_word_at(2, "IGNORE")
            || self.is_words(&["FROM", "FIRST"]) && self.is_word_at(2, "RESPECT")
            || self.is_words(&["FROM", "FIRST"]) && self.is_word_at(2, "OVER")
        {
            self.advance();
            self.advance();
        } else if self.is_words(&["FROM", "LAST"]) {
            return Err(Self::unsupported("NTH_VALUE ... FROM LAST"));
        }
        let mut ignore_nulls = false;
        let mut nulls_given = false;
        if self.is_words(&["IGNORE", "NULLS"]) {
            self.advance();
            self.advance();
            ignore_nulls = true;
            nulls_given = true;
        } else if self.is_words(&["RESPECT", "NULLS"]) {
            self.advance();
            self.advance();
            nulls_given = true;
        }
        if !self.is_word("OVER") {
            if nulls_given {
                return Err(self.expected("OVER"));
            }
            return Ok(e);
        }
        self.advance();
        self.expect_sym("(")?;
        let mut window = Window::default();
        if self.eat_words(&["PARTITION", "BY"]) {
            window.partition.push(self.expr()?);
            while self.eat_sym(",") {
                window.partition.push(self.expr()?);
            }
        }
        if self.eat_words(&["ORDER", "BY"]) {
            window.order = self.order_list()?;
        }
        if self.is_word("ROWS") || self.is_word("RANGE") {
            let rows = self.is_word("ROWS");
            self.advance();
            let frame = if self.eat_word("BETWEEN") {
                let start = self.frame_bound()?;
                self.expect_word("AND")?;
                let end = self.frame_bound()?;
                Frame { rows, start, end }
            } else {
                Frame { rows, start: self.frame_bound()?, end: FrameBound::CurrentRow }
            };
            window.frame = Some(frame);
        }
        self.expect_sym(")")?;
        let slot = self.windows;
        self.windows += 1;
        Ok(Expr::Over(Box::new(Over { func: e, ignore_nulls, window, slot })))
    }

    fn frame_bound(&mut self) -> Result<FrameBound> {
        if self.eat_words(&["UNBOUNDED", "PRECEDING"]) {
            return Ok(FrameBound::UnboundedPreceding);
        }
        if self.eat_words(&["UNBOUNDED", "FOLLOWING"]) {
            return Ok(FrameBound::UnboundedFollowing);
        }
        if self.eat_words(&["CURRENT", "ROW"]) {
            return Ok(FrameBound::CurrentRow);
        }
        let n = self.additive()?;
        if self.eat_word("PRECEDING") {
            Ok(FrameBound::Preceding(n))
        } else {
            self.expect_word("FOLLOWING")?;
            Ok(FrameBound::Following(n))
        }
    }

    /// Order items separated by commas.
    pub(crate) fn order_list(&mut self) -> Result<Vec<crate::ast::OrderItem>> {
        let mut v = vec![self.order_item()?];
        while self.eat_sym(",") {
            v.push(self.order_item()?);
        }
        Ok(v)
    }

    /// One element of `GROUP BY`.
    pub(crate) fn grouping_element(&mut self) -> Result<Expr> {
        if self.eat_words(&["GROUPING", "SETS"]) {
            return Ok(Expr::Function(Function {
                name: GROUPING_SETS.into(),
                distinct: false,
                star: false,
                args: self.grouping_list()?,
            }));
        }
        for w in ["ROLLUP", "CUBE"] {
            if self.is_word(w) && self.is_sym_at(1, "(") {
                self.advance();
                return Ok(Expr::Function(Function { name: w.into(), distinct: false, star: false, args: self.grouping_list()? }));
            }
        }
        if self.is_sym("(") && self.is_sym_at(1, ")") {
            self.advance();
            self.advance();
            return Ok(Expr::Row(Vec::new()));
        }
        self.expr()
    }

    fn grouping_list(&mut self) -> Result<Vec<Expr>> {
        self.expect_sym("(")?;
        let mut v = vec![self.grouping_element()?];
        while self.eat_sym(",") {
            v.push(self.grouping_element()?);
        }
        self.expect_sym(")")?;
        Ok(v)
    }
}

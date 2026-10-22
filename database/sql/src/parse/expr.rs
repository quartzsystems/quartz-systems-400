// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions and search conditions, by precedence: `OR`, `AND`, `NOT`,
//! predicates, then `+ -`, then `* / ||`, then prefix signs and primaries.

use super::{Parser, RESERVED};
use crate::ast::{
    BinOp, CmpOp, ColumnRef, DurationUnit, Expr, Function, Ident, Literal, Quantifier, Register,
};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    /// A search condition or value expression.
    pub(crate) fn expr(&mut self) -> Result<Expr> {
        let mut left = self.and_expr()?;
        while self.eat_word("OR") {
            let right = self.and_expr()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr> {
        let mut left = self.not_expr()?;
        while self.eat_word("AND") {
            let right = self.not_expr()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> Result<Expr> {
        if self.eat_word("NOT") {
            return Ok(Expr::Not(Box::new(self.not_expr()?)));
        }
        self.predicate()
    }

    /// A value expression followed by an optional predicate tail.
    fn predicate(&mut self) -> Result<Expr> {
        if self.is_word("EXISTS") {
            self.advance();
            self.expect_sym("(")?;
            let q = self.query()?;
            self.expect_sym(")")?;
            return Ok(Expr::Exists(Box::new(q), false));
        }
        let left = self.additive()?;
        let in_pred = |p: &Self, ahead: usize| {
            p.is_word_at(ahead, "IN") && matches!(p.tok_at(ahead + 1), Tok::Sym("("))
        };
        let not = self.is_word("NOT")
            && (self.is_word_at(1, "BETWEEN") || self.is_word_at(1, "LIKE") || in_pred(self, 1));
        if not {
            self.advance();
        }
        if self.eat_word("BETWEEN") {
            let lo = self.additive()?;
            self.expect_word("AND")?;
            let hi = self.additive()?;
            return Ok(Expr::Between(
                Box::new(left),
                Box::new(lo),
                Box::new(hi),
                not,
            ));
        }
        if in_pred(self, 0) {
            self.advance();
            self.expect_sym("(")?;
            if self.starts_query() {
                let q = self.query()?;
                self.expect_sym(")")?;
                return Ok(Expr::InSubquery(Box::new(left), Box::new(q), not));
            }
            let mut list = vec![self.expr()?];
            while self.eat_sym(",") {
                list.push(self.expr()?);
            }
            self.expect_sym(")")?;
            return Ok(Expr::InList(Box::new(left), list, not));
        }
        if self.eat_word("LIKE") {
            let pattern = self.additive()?;
            let escape = if self.eat_word("ESCAPE") {
                Some(Box::new(self.additive()?))
            } else {
                None
            };
            return Ok(Expr::Like(Box::new(left), Box::new(pattern), escape, not));
        }
        if self.eat_word("IS") {
            let not = self.eat_word("NOT");
            if self.eat_word("NULL") {
                return Ok(Expr::IsNull(Box::new(left), not));
            }
            return Err(self.expected("NULL"));
        }
        let op = match self.tok() {
            Tok::Sym("=") => CmpOp::Eq,
            Tok::Sym("<>") => CmpOp::Ne,
            Tok::Sym("<") => CmpOp::Lt,
            Tok::Sym("<=") => CmpOp::Le,
            Tok::Sym(">") => CmpOp::Gt,
            Tok::Sym(">=") => CmpOp::Ge,
            _ => return Ok(left),
        };
        self.advance();
        let quantifier = if self.eat_word("ALL") {
            Some(Quantifier::All)
        } else if self.eat_word("ANY") || self.eat_word("SOME") {
            Some(Quantifier::Any)
        } else {
            None
        };
        let right = if quantifier.is_some() {
            self.expect_sym("(")?;
            let q = self.query()?;
            self.expect_sym(")")?;
            Expr::Subquery(Box::new(q))
        } else {
            self.additive()?
        };
        Ok(Expr::Compare(
            Box::new(left),
            op,
            Box::new(right),
            quantifier,
        ))
    }

    fn additive(&mut self) -> Result<Expr> {
        let mut left = self.term()?;
        loop {
            let op = match self.tok() {
                Tok::Sym("+") => BinOp::Add,
                Tok::Sym("-") => BinOp::Sub,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.term()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
    }

    fn term(&mut self) -> Result<Expr> {
        let mut left = self.factor()?;
        loop {
            let op = match self.tok() {
                Tok::Sym("*") => BinOp::Mul,
                Tok::Sym("/") => BinOp::Div,
                Tok::Sym("||") => BinOp::Concat,
                Tok::Word(w) if w == "CONCAT" => BinOp::Concat,
                _ => return Ok(left),
            };
            self.advance();
            let right = self.factor()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
    }

    fn factor(&mut self) -> Result<Expr> {
        if self.eat_sym("-") {
            return Ok(Expr::Neg(Box::new(self.factor()?)));
        }
        if self.eat_sym("+") {
            return self.factor();
        }
        let e = self.primary()?;
        // A labeled duration: `e DAYS`.
        let unit = match self.tok() {
            Tok::Word(w) => match w.as_str() {
                "YEAR" | "YEARS" => Some(DurationUnit::Years),
                "MONTH" | "MONTHS" => Some(DurationUnit::Months),
                "DAY" | "DAYS" => Some(DurationUnit::Days),
                "HOUR" | "HOURS" => Some(DurationUnit::Hours),
                "MINUTE" | "MINUTES" => Some(DurationUnit::Minutes),
                "SECOND" | "SECONDS" => Some(DurationUnit::Seconds),
                "MICROSECOND" | "MICROSECONDS" => Some(DurationUnit::Microseconds),
                _ => None,
            },
            _ => None,
        };
        if let Some(u) = unit
            && !self.is_sym_at(1, "(")
        {
            self.advance();
            return Ok(Expr::Duration(Box::new(e), u));
        }
        Ok(e)
    }

    pub(crate) fn is_sym_at(&self, ahead: usize, s: &str) -> bool {
        matches!(self.tok_at(ahead), Tok::Sym(x) if *x == s)
    }

    /// Whether a query starts here.
    pub(crate) fn starts_query(&self) -> bool {
        self.is_word("SELECT")
            || self.is_word("VALUES")
            || self.is_word("WITH")
            || (self.is_sym("(") && {
                // Look through nested parentheses.
                let mut i = 0;
                while matches!(self.tok_at(i), Tok::Sym("(")) {
                    i += 1;
                }
                matches!(self.tok_at(i), Tok::Word(w) if w == "SELECT" || w == "VALUES")
            })
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.tok().clone() {
            Tok::Str(s) => {
                self.advance();
                Ok(Expr::Literal(Literal::Str(s)))
            }
            Tok::Hex(h) => {
                self.advance();
                Ok(Expr::Literal(Literal::Hex(h)))
            }
            Tok::Num(n) => {
                self.advance();
                Ok(Expr::Literal(Literal::Num(n)))
            }
            Tok::Param => {
                self.advance();
                let i = self.params;
                self.params += 1;
                Ok(Expr::Param(i))
            }
            Tok::Host { name, ind } => {
                self.advance();
                Ok(Expr::Host { name, ind })
            }
            Tok::Sym("(") => {
                self.advance();
                if self.starts_query() {
                    let q = self.query()?;
                    self.expect_sym(")")?;
                    return Ok(Expr::Subquery(Box::new(q)));
                }
                let first = self.expr()?;
                if self.eat_sym(",") {
                    let mut items = vec![first, self.expr()?];
                    while self.eat_sym(",") {
                        items.push(self.expr()?);
                    }
                    self.expect_sym(")")?;
                    return Ok(Expr::Row(items));
                }
                self.expect_sym(")")?;
                Ok(first)
            }
            Tok::Word(w) => self.word_primary(&w),
            Tok::Delimited(_) => self.column_or_call(),
            _ => Err(self.expected("an expression")),
        }
    }

    fn word_primary(&mut self, w: &str) -> Result<Expr> {
        match w {
            "NULL" => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            "DEFAULT" => {
                self.advance();
                Ok(Expr::Default)
            }
            "CASE" => self.case(),
            "CAST" => {
                self.advance();
                self.expect_sym("(")?;
                let e = self.expr()?;
                self.expect_word("AS")?;
                let t = self.type_spec()?;
                self.expect_sym(")")?;
                Ok(Expr::Cast(Box::new(e), t))
            }
            "DATE" | "TIME" | "TIMESTAMP" if matches!(self.tok_at(1), Tok::Str(_)) => {
                self.advance();
                let Tok::Str(s) = self.advance() else {
                    unreachable!()
                };
                Ok(Expr::Literal(match w {
                    "DATE" => Literal::Date(s),
                    "TIME" => Literal::Time(s),
                    _ => Literal::Timestamp(s),
                }))
            }
            "CURRENT" | "CURRENT_DATE" | "CURRENT_TIME" | "CURRENT_TIMESTAMP" | "CURRENT_USER"
            | "CURRENT_SCHEMA" | "CURRENT_SERVER" | "CURRENT_PATH" | "CURRENT_TIMEZONE"
            | "USER" | "SESSION_USER" | "SYSTEM_USER"
                if !self.is_sym_at(1, "(") && !self.is_sym_at(1, ".") =>
            {
                self.register(w)
            }
            _ if RESERVED.contains(&w) => Err(self.expected("an expression")),
            _ => self.column_or_call(),
        }
    }

    fn register(&mut self, w: &str) -> Result<Expr> {
        self.advance();
        let r = match w {
            "CURRENT_DATE" => Register::Date,
            "CURRENT_TIME" => Register::Time,
            "CURRENT_TIMESTAMP" => Register::Timestamp,
            "CURRENT_USER" | "USER" | "SESSION_USER" | "SYSTEM_USER" => Register::User,
            "CURRENT_SCHEMA" => Register::Schema,
            "CURRENT_SERVER" => Register::Server,
            "CURRENT_PATH" => Register::Path,
            "CURRENT_TIMEZONE" => Register::Timezone,
            _ => {
                let Tok::Word(next) = self.tok().clone() else {
                    return Err(self.expected("a special register after CURRENT"));
                };
                let r = match next.as_str() {
                    "DATE" => Register::Date,
                    "TIME" => Register::Time,
                    "TIMESTAMP" => Register::Timestamp,
                    "USER" => Register::User,
                    "SERVER" => Register::Server,
                    "SCHEMA" => Register::Schema,
                    "PATH" => Register::Path,
                    "TIMEZONE" => Register::Timezone,
                    _ => return Err(Self::unsupported(&format!("CURRENT {next}"))),
                };
                self.advance();
                r
            }
        };
        Ok(Expr::Register(r))
    }

    fn case(&mut self) -> Result<Expr> {
        self.expect_word("CASE")?;
        let operand = if self.is_word("WHEN") {
            None
        } else {
            Some(Box::new(self.expr()?))
        };
        let mut whens = Vec::new();
        while self.eat_word("WHEN") {
            let w = self.expr()?;
            self.expect_word("THEN")?;
            let t = self.result_expr()?;
            whens.push((w, t));
        }
        if whens.is_empty() {
            return Err(self.expected("WHEN"));
        }
        let otherwise = if self.eat_word("ELSE") {
            Some(Box::new(self.result_expr()?))
        } else {
            None
        };
        self.expect_word("END")?;
        Ok(Expr::Case {
            operand,
            whens,
            otherwise,
        })
    }

    /// A `THEN`/`ELSE` result: an expression or `NULL`.
    fn result_expr(&mut self) -> Result<Expr> {
        self.expr()
    }

    /// A column reference or a function call.
    fn column_or_call(&mut self) -> Result<Expr> {
        let first = self.ident()?;
        if self.is_sym("(") && !first.delimited {
            return self.call(first.value);
        }
        if self.eat_sym(".") {
            // T.C; T.* is handled by the select list.
            let column = self.ident()?;
            if self.is_sym(".") {
                return Err(crate::error::SqlError::ThreePartName {
                    name: format!("{first}.{column}..."),
                });
            }
            return Ok(Expr::Column(ColumnRef {
                table: Some(first),
                column,
            }));
        }
        Ok(Expr::Column(ColumnRef {
            table: None,
            column: first,
        }))
    }

    fn call(&mut self, name: String) -> Result<Expr> {
        self.expect_sym("(")?;
        let mut f = Function {
            name,
            distinct: false,
            star: false,
            args: Vec::new(),
        };
        if self.eat_sym("*") {
            f.star = true;
            self.expect_sym(")")?;
            return Ok(Expr::Function(f));
        }
        if self.eat_sym(")") {
            return Ok(Expr::Function(f));
        }
        if self.eat_word("DISTINCT") {
            f.distinct = true;
        } else {
            self.eat_word("ALL");
        }
        if matches!(f.name.as_str(), "TRIM" | "LTRIM" | "RTRIM")
            && let Tok::Word(w) = self.tok().clone()
            && matches!(
                w.as_str(),
                "BOTH" | "LEADING" | "TRAILING" | "B" | "L" | "T"
            )
            && !self.is_sym_at(1, "(")
            && !self.is_sym_at(1, ")")
            && !self.is_sym_at(1, ",")
        {
            self.advance();
            f.args.push(Expr::Literal(Literal::Str(w)));
        }
        f.args.push(self.expr()?);
        while self.eat_sym(",") {
            f.args.push(self.expr()?);
        }
        // SUBSTRING(e FROM n FOR m), TRIM(BOTH 'x' FROM e), POSITION(a IN b),
        // EXTRACT(part FROM e) spellings.
        if self.eat_word("FROM") {
            f.args.push(self.expr()?);
            if self.eat_word("FOR") {
                f.args.push(self.expr()?);
            }
        } else if self.eat_word("IN") {
            f.args.push(self.expr()?);
        }
        self.expect_sym(")")?;
        Ok(Expr::Function(f))
    }

    /// A column name, for `ORDER BY`, keys and assignments.
    pub(crate) fn column_ref(&mut self) -> Result<ColumnRef> {
        let first = self.ident()?;
        if self.eat_sym(".") {
            let column = self.ident()?;
            return Ok(ColumnRef {
                table: Some(first),
                column,
            });
        }
        Ok(ColumnRef {
            table: None,
            column: first,
        })
    }

    /// `expr [AS] name`: the optional name.
    pub(crate) fn alias(&mut self) -> Result<Option<Ident>> {
        if self.eat_word("AS") {
            return Ok(Some(self.ident()?));
        }
        self.optional_name()
    }
}

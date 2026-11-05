// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Table references that start with a keyword: `LATERAL (...)`,
//! `TABLE (query)`, `TABLE (f(args))`, `JSON_TABLE (...)` and `FINAL|NEW
//! TABLE (change)`.

use super::Parser;
use crate::ast::{ChangeRows, Ident, Statement, TableRef};
use crate::error::Result;

impl Parser {
    pub(crate) fn correlation(&mut self) -> Result<(Option<Ident>, Vec<Ident>)> {
        let alias = self.alias()?;
        let columns = if alias.is_some() && self.is_sym("(") { self.ident_list()? } else { Vec::new() };
        Ok((alias, columns))
    }

    /// One of the keyword table references, or `None`.
    pub(crate) fn special_table(&mut self) -> Result<Option<TableRef>> {
        // DB2 for i 7.5 has FINAL and NEW TABLE of an INSERT only.
        for (word, kind) in [("FINAL", ChangeRows::Final), ("NEW", ChangeRows::New)] {
            if self.is_word(word) && self.is_word_at(1, "TABLE") && self.is_sym_at(2, "(") {
                self.advance();
                self.advance();
                self.advance();
                let statement = match self.tok() {
                    crate::lex::Tok::Word(w) if w == "INSERT" => self.statement_body()?,
                    crate::lex::Tok::Word(_) => return Err(self.keyword()),
                    _ => return Err(self.expected("INSERT")),
                };
                self.expect_sym(")")?;
                let (alias, columns) = self.correlation()?;
                let alias = match (alias, &statement) {
                    (Some(a), _) => a,
                    (None, Statement::Insert(i)) => i.table.name.clone(),
                    (None, _) => Ident::new("INSERT"),
                };
                return Ok(Some(TableRef::DataChange { kind, statement: Box::new(statement), alias, columns }));
            }
        }
        if self.is_word("JSON_TABLE") && self.is_sym_at(1, "(") {
            self.advance();
            self.advance();
            return self.json_table().map(Some);
        }
        let lateral = self.is_word("LATERAL");
        if !(lateral || self.is_word("TABLE")) || !self.is_sym_at(1, "(") {
            return Ok(None);
        }
        self.advance();
        self.expect_sym("(")?;
        if self.starts_query() {
            let q = self.query()?;
            self.expect_sym(")")?;
            let (alias, columns) = self.correlation()?;
            let alias = alias.ok_or_else(|| self.expected("a correlation name"))?;
            return Ok(Some(TableRef::Lateral(Box::new(TableRef::Subquery { query: Box::new(q), alias, columns }))));
        }
        if lateral {
            return Err(self.expected("a fullselect"));
        }
        // TABLE (f(args)).
        let name = self.object_name()?;
        self.expect_sym("(")?;
        let mut args = Vec::new();
        if !self.eat_sym(")") {
            args.push(self.expr()?);
            while self.eat_sym(",") {
                args.push(self.expr()?);
            }
            self.expect_sym(")")?;
        }
        self.expect_sym(")")?;
        let (alias, columns) = self.correlation()?;
        let alias = alias.ok_or_else(|| self.expected("a correlation name"))?;
        Ok(Some(TableRef::Lateral(Box::new(TableRef::Function { name, args, alias, columns }))))
    }
}

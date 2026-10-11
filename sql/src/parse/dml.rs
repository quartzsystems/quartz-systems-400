// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSERT`, `UPDATE`, `DELETE` and `MERGE`.

use super::Parser;
use crate::ast::{
    Assignment, Delete, Expr, Insert, InsertSource, Merge, MergeAction, Statement, Update, Where,
};
use crate::error::Result;

impl Parser {
    pub(crate) fn insert(&mut self) -> Result<Statement> {
        self.expect_word("INSERT")?;
        self.expect_word("INTO")?;
        let table = self.object_name()?;
        let columns = if self.is_sym("(") && !self.starts_query() {
            self.ident_list()?
        } else {
            Vec::new()
        };
        if self.eat_word("OVERRIDING") {
            return Err(Self::unsupported("OVERRIDING USER VALUE"));
        }
        let source = if self.eat_words(&["DEFAULT", "VALUES"]) {
            InsertSource::DefaultValues
        } else if self.eat_word("VALUES") {
            let mut rows = vec![self.values_row()?];
            while self.eat_sym(",") {
                rows.push(self.values_row()?);
            }
            InsertSource::Values(rows)
        } else {
            InsertSource::Query(Box::new(self.query()?))
        };
        let isolation = self.isolation();
        Ok(Statement::Insert(Insert {
            table,
            columns,
            source,
            isolation,
        }))
    }

    pub(crate) fn update(&mut self) -> Result<Statement> {
        self.expect_word("UPDATE")?;
        let table = self.object_name()?;
        let alias = self.alias()?;
        self.expect_word("SET")?;
        let mut set = vec![self.assignment()?];
        while self.eat_sym(",") {
            set.push(self.assignment()?);
        }
        let where_clause = self.where_clause()?;
        let isolation = self.isolation();
        Ok(Statement::Update(Update {
            table,
            alias,
            set,
            where_clause,
            isolation,
        }))
    }

    /// `col = value` or `(c1, c2) = (v1, v2)`.
    pub(crate) fn assignment(&mut self) -> Result<Assignment> {
        if self.is_sym("(") {
            let columns = self.ident_list()?;
            self.expect_sym("=")?;
            let values = match self.expr()? {
                Expr::Row(items) => items,
                single => vec![single],
            };
            if values.len() != columns.len() && values.len() != 1 {
                return Err(self.expected("as many values as columns"));
            }
            return Ok(Assignment { columns, values });
        }
        let column = self.column_ref()?.column;
        self.expect_sym("=")?;
        let value = self.expr()?;
        Ok(Assignment {
            columns: vec![column],
            values: vec![value],
        })
    }

    fn where_clause(&mut self) -> Result<Where> {
        if !self.eat_word("WHERE") {
            return Ok(Where::All);
        }
        if self.eat_words(&["CURRENT", "OF"]) {
            return Ok(Where::CurrentOf(self.ident()?));
        }
        Ok(Where::Cond(self.expr()?))
    }

    pub(crate) fn delete(&mut self) -> Result<Statement> {
        self.expect_word("DELETE")?;
        self.expect_word("FROM")?;
        let table = self.object_name()?;
        let alias = self.alias()?;
        let where_clause = self.where_clause()?;
        let isolation = self.isolation();
        Ok(Statement::Delete(Delete {
            table,
            alias,
            where_clause,
            isolation,
        }))
    }

    pub(crate) fn merge(&mut self) -> Result<Statement> {
        self.expect_word("MERGE")?;
        self.expect_word("INTO")?;
        let target = self.object_name()?;
        let alias = self.alias()?;
        self.expect_word("USING")?;
        let source = self.merge_source()?;
        self.expect_word("ON")?;
        let on = self.expr()?;
        let mut actions = Vec::new();
        while self.eat_word("WHEN") {
            let matched = if self.eat_word("MATCHED") {
                true
            } else {
                self.expect_words(&["NOT", "MATCHED"])?;
                false
            };
            let cond = if self.eat_word("AND") {
                Some(self.expr()?)
            } else {
                None
            };
            self.expect_word("THEN")?;
            let action = match (matched, self.eat_word("UPDATE"), self.eat_word("DELETE")) {
                (true, true, _) => {
                    self.expect_word("SET")?;
                    let mut set = vec![self.assignment()?];
                    while self.eat_sym(",") {
                        set.push(self.assignment()?);
                    }
                    MergeAction::Update { cond, set }
                }
                (true, false, true) => MergeAction::Delete { cond },
                (false, false, false) => {
                    self.expect_word("INSERT")?;
                    let columns = if self.is_sym("(") {
                        self.ident_list()?
                    } else {
                        Vec::new()
                    };
                    self.expect_word("VALUES")?;
                    let values = self.values_row()?;
                    MergeAction::Insert {
                        cond,
                        columns,
                        values,
                    }
                }
                _ => {
                    return Err(
                        self.expected("UPDATE or DELETE after MATCHED, INSERT after NOT MATCHED")
                    );
                }
            };
            actions.push(action);
        }
        if actions.is_empty() {
            return Err(self.expected("WHEN"));
        }
        self.eat_words(&["ELSE", "IGNORE"]);
        if self.eat_word("NOT") {
            self.expect_word("ATOMIC")?;
            self.expect_words(&["CONTINUE", "ON", "SQLEXCEPTION"])?;
        }
        Ok(Statement::Merge(Merge {
            target,
            alias,
            source,
            on,
            actions,
        }))
    }

    fn merge_source(&mut self) -> Result<crate::ast::TableRef> {
        if self.is_sym("(") {
            self.advance();
            let q = self.query()?;
            self.expect_sym(")")?;
            self.eat_word("AS");
            let alias = self.ident()?;
            let columns = if self.is_sym("(") {
                self.ident_list()?
            } else {
                Vec::new()
            };
            return Ok(crate::ast::TableRef::Subquery {
                query: Box::new(q),
                alias,
                columns,
            });
        }
        let name = self.object_name()?;
        let alias = self.alias()?;
        Ok(crate::ast::TableRef::Table {
            name,
            alias,
            columns: Vec::new(),
        })
    }
}

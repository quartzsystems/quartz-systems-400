// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Queries: `SELECT`, `VALUES`, `UNION`, `ORDER BY`, row limits, the
//! `FOR` and `WITH` clauses, and table references with joins.

use super::Parser;
use crate::ast::{
    Expr, Ident, JoinKind, OrderItem, Query, QueryBody, Select, SelectItem, TableRef,
};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    /// A full query with its clauses.
    pub(crate) fn query(&mut self) -> Result<Query> {
        if self.is_word("WITH") && !self.is_word_at(2, "AS") {
            // WITH NC and the like belong after the body; a common table
            // expression is WITH name AS (...).
        }
        if self.is_word("WITH") && (self.is_word_at(2, "AS") || self.is_sym_at(2, "(")) {
            // Common table expressions: each becomes a derived table
            // wherever the body names it.
            self.advance();
            let mut ctes = Vec::new();
            loop {
                let name = self.ident()?;
                let columns = self.optional_ident_list()?;
                self.expect_word("AS")?;
                self.expect_sym("(")?;
                let q = self.query()?;
                self.expect_sym(")")?;
                ctes.push((name, columns, q));
                if !self.eat_sym(",") {
                    break;
                }
            }
            let mut q = self.query()?;
            substitute_query(&mut q, &ctes);
            return Ok(q);
        }
        let body = self.union_body()?;
        let mut q = Query::of(body);
        if self.eat_words(&["ORDER", "BY"]) {
            q.order_by.push(self.order_item()?);
            while self.eat_sym(",") {
                q.order_by.push(self.order_item()?);
            }
        }
        loop {
            if self.eat_word("OFFSET") {
                q.offset = Some(self.additive_for_limit()?);
                if !self.eat_word("ROWS") {
                    self.eat_word("ROW");
                }
            } else if self.eat_word("FETCH") {
                if !self.eat_word("FIRST") {
                    self.expect_word("NEXT")?;
                }
                q.fetch_first = Some(if self.is_word("ROW") || self.is_word("ROWS") {
                    Expr::num("1")
                } else {
                    self.additive_for_limit()?
                });
                if !self.eat_word("ROWS") {
                    self.expect_word("ROW")?;
                }
                self.expect_word("ONLY")?;
            } else if self.eat_word("LIMIT") {
                let n = self.additive_for_limit()?;
                if self.eat_sym(",") {
                    q.offset = Some(n);
                    q.fetch_first = Some(self.additive_for_limit()?);
                } else {
                    q.fetch_first = Some(n);
                    if self.eat_word("OFFSET") {
                        q.offset = Some(self.additive_for_limit()?);
                    }
                }
            } else if self.eat_word("FOR") {
                if self.eat_word("UPDATE") {
                    let mut cols = Vec::new();
                    if self.eat_word("OF") {
                        cols.push(self.ident()?);
                        while self.eat_sym(",") {
                            cols.push(self.ident()?);
                        }
                    }
                    q.for_update = Some(cols);
                } else if self.eat_words(&["READ", "ONLY"]) || self.eat_words(&["FETCH", "ONLY"]) {
                    q.read_only = true;
                } else {
                    return Err(self.expected("UPDATE or READ ONLY"));
                }
            } else if self.eat_words(&["OPTIMIZE", "FOR"]) {
                self.additive_for_limit()?;
                if !self.eat_word("ROWS") {
                    self.expect_word("ROW")?;
                }
            } else if let Some(iso) = self.isolation() {
                q.isolation = Some(iso);
            } else {
                return Ok(q);
            }
        }
    }

    /// A row count: a number, a parameter or a host variable.
    fn additive_for_limit(&mut self) -> Result<Expr> {
        match self.tok().clone() {
            Tok::Num(n) => {
                self.advance();
                Ok(Expr::num(&n))
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
            _ => Err(self.expected("a row count")),
        }
    }

    fn order_item(&mut self) -> Result<OrderItem> {
        let expr = self.expr()?;
        let desc = if self.eat_word("DESC") {
            true
        } else {
            self.eat_word("ASC");
            false
        };
        let nulls_first = if self.eat_word("NULLS") {
            if self.eat_word("FIRST") {
                Some(true)
            } else {
                self.expect_word("LAST")?;
                Some(false)
            }
        } else {
            None
        };
        Ok(OrderItem {
            expr,
            desc,
            nulls_first,
        })
    }

    fn union_body(&mut self) -> Result<QueryBody> {
        let mut left = self.simple_body()?;
        loop {
            if self.eat_word("UNION") {
                let all = self.eat_word("ALL");
                if !all {
                    self.eat_word("DISTINCT");
                }
                let right = self.simple_body()?;
                left = QueryBody::Union {
                    left: Box::new(left),
                    right: Box::new(right),
                    all,
                };
            } else if self.is_word("EXCEPT") || self.is_word("INTERSECT") {
                return Err(Self::unsupported("EXCEPT and INTERSECT"));
            } else {
                return Ok(left);
            }
        }
    }

    fn simple_body(&mut self) -> Result<QueryBody> {
        if self.eat_sym("(") {
            let q = self.query()?;
            self.expect_sym(")")?;
            return Ok(QueryBody::Nested(Box::new(q)));
        }
        if self.eat_word("VALUES") {
            let mut rows = vec![self.values_row()?];
            while self.eat_sym(",") {
                rows.push(self.values_row()?);
            }
            return Ok(QueryBody::Values(rows));
        }
        self.expect_word("SELECT")?;
        Ok(QueryBody::Select(Box::new(self.select()?)))
    }

    /// `(a, b)` or a single value.
    pub(crate) fn values_row(&mut self) -> Result<Vec<Expr>> {
        if self.is_sym("(") && !self.starts_query() {
            self.advance();
            let mut row = vec![self.expr()?];
            while self.eat_sym(",") {
                row.push(self.expr()?);
            }
            self.expect_sym(")")?;
            return Ok(row);
        }
        Ok(vec![self.expr()?])
    }

    fn select(&mut self) -> Result<Select> {
        let distinct = if self.eat_word("DISTINCT") {
            true
        } else {
            self.eat_word("ALL");
            false
        };
        let mut items = vec![self.select_item()?];
        while self.eat_sym(",") {
            items.push(self.select_item()?);
        }
        let mut from = Vec::new();
        if self.eat_word("FROM") {
            from.push(self.table_ref()?);
            while self.eat_sym(",") {
                from.push(self.table_ref()?);
            }
        }
        let where_clause = if self.eat_word("WHERE") {
            Some(self.expr()?)
        } else {
            None
        };
        let mut group_by = Vec::new();
        if self.eat_words(&["GROUP", "BY"]) {
            group_by.push(self.expr()?);
            while self.eat_sym(",") {
                group_by.push(self.expr()?);
            }
        }
        let having = if self.eat_word("HAVING") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Select {
            distinct,
            items,
            from,
            where_clause,
            group_by,
            having,
        })
    }

    fn select_item(&mut self) -> Result<SelectItem> {
        if self.eat_sym("*") {
            return Ok(SelectItem::Star);
        }
        if matches!(self.tok(), Tok::Word(_) | Tok::Delimited(_))
            && matches!(self.tok_at(1), Tok::Sym("."))
            && matches!(self.tok_at(2), Tok::Sym("*"))
        {
            let t = self.ident()?;
            self.advance();
            self.advance();
            return Ok(SelectItem::TableStar(t));
        }
        let e = self.expr()?;
        let alias = self.alias()?;
        Ok(SelectItem::Expr(e, alias))
    }

    /// A table reference with any joins hanging off it.
    fn table_ref(&mut self) -> Result<TableRef> {
        let mut left = self.table_primary()?;
        loop {
            let kind = if self.eat_words(&["CROSS", "JOIN"]) {
                JoinKind::Cross
            } else if self.eat_word("JOIN") || self.eat_words(&["INNER", "JOIN"]) {
                JoinKind::Inner
            } else if self.eat_word("LEFT") {
                self.eat_word("OUTER");
                if self.eat_word("EXCEPTION") {
                    self.expect_word("JOIN")?;
                    JoinKind::LeftException
                } else {
                    self.expect_word("JOIN")?;
                    JoinKind::Left
                }
            } else if self.eat_word("RIGHT") {
                self.eat_word("OUTER");
                if self.eat_word("EXCEPTION") {
                    self.expect_word("JOIN")?;
                    JoinKind::RightException
                } else {
                    self.expect_word("JOIN")?;
                    JoinKind::Right
                }
            } else if self.eat_word("FULL") {
                self.eat_word("OUTER");
                self.expect_word("JOIN")?;
                JoinKind::Full
            } else if self.eat_words(&["EXCEPTION", "JOIN"]) {
                JoinKind::LeftException
            } else {
                return Ok(left);
            };
            let right = self.table_primary()?;
            let on = if kind == JoinKind::Cross {
                None
            } else {
                self.expect_word("ON")?;
                Some(self.expr()?)
            };
            left = TableRef::Join {
                left: Box::new(left),
                right: Box::new(right),
                kind,
                on,
            };
        }
    }

    fn table_primary(&mut self) -> Result<TableRef> {
        if self.is_sym("(") {
            if self.starts_query() {
                self.advance();
                let q = self.query()?;
                self.expect_sym(")")?;
                self.eat_word("AS");
                let alias = self.ident()?;
                let columns = self.optional_ident_list()?;
                return Ok(TableRef::Subquery {
                    query: Box::new(q),
                    alias,
                    columns,
                });
            }
            // A parenthesized join.
            self.advance();
            let inner = self.table_ref()?;
            self.expect_sym(")")?;
            return Ok(inner);
        }
        if self.eat_word("TABLE") || self.eat_word("LATERAL") {
            return Err(Self::unsupported("TABLE(...) and LATERAL table references"));
        }
        let name = self.object_name()?;
        let alias = self.alias()?;
        let columns = if alias.is_some() {
            self.optional_ident_list()?
        } else {
            Vec::new()
        };
        Ok(TableRef::Table {
            name,
            alias,
            columns,
        })
    }

    fn optional_ident_list(&mut self) -> Result<Vec<Ident>> {
        if self.is_sym("(") {
            self.ident_list()
        } else {
            Ok(Vec::new())
        }
    }
}

/// A common table expression: its name, column names and query.
type Cte = (Ident, Vec<Ident>, Query);

/// Every table reference of `q` that names a common table expression
/// becomes that expression as a derived table.
fn substitute_query(q: &mut Query, ctes: &[Cte]) {
    substitute_body(&mut q.body, ctes);
    for item in &mut q.order_by {
        substitute_expr(&mut item.expr, ctes);
    }
}

fn substitute_body(b: &mut QueryBody, ctes: &[Cte]) {
    match b {
        QueryBody::Select(sel) => {
            for t in &mut sel.from {
                substitute_table(t, ctes);
            }
            for item in &mut sel.items {
                if let SelectItem::Expr(e, _) = item {
                    substitute_expr(e, ctes);
                }
            }
            if let Some(w) = &mut sel.where_clause {
                substitute_expr(w, ctes);
            }
            for g in &mut sel.group_by {
                substitute_expr(g, ctes);
            }
            if let Some(h) = &mut sel.having {
                substitute_expr(h, ctes);
            }
        }
        QueryBody::Values(rows) => {
            for r in rows {
                for e in r {
                    substitute_expr(e, ctes);
                }
            }
        }
        QueryBody::Union { left, right, .. } => {
            substitute_body(left, ctes);
            substitute_body(right, ctes);
        }
        QueryBody::Nested(inner) => substitute_query(inner, ctes),
    }
}

fn substitute_table(t: &mut TableRef, ctes: &[Cte]) {
    match t {
        TableRef::Table {
            name,
            alias,
            columns,
        } if name.schema.is_none() => {
            if let Some((cname, ccols, q)) =
                ctes.iter().find(|(n, _, _)| n.value == name.name.value)
            {
                let alias = alias.clone().unwrap_or_else(|| cname.clone());
                let cols = if columns.is_empty() {
                    ccols.clone()
                } else {
                    columns.clone()
                };
                *t = TableRef::Subquery {
                    query: Box::new(q.clone()),
                    alias,
                    columns: cols,
                };
            }
        }
        TableRef::Table { .. } => {}
        TableRef::Subquery { query, .. } => substitute_query(query, ctes),
        TableRef::Join {
            left, right, on, ..
        } => {
            substitute_table(left, ctes);
            substitute_table(right, ctes);
            if let Some(e) = on {
                substitute_expr(e, ctes);
            }
        }
    }
}

fn substitute_expr(e: &mut Expr, ctes: &[Cte]) {
    match e {
        Expr::Neg(a)
        | Expr::Not(a)
        | Expr::IsNull(a, _)
        | Expr::Cast(a, _)
        | Expr::Duration(a, _) => {
            substitute_expr(a, ctes);
        }
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => {
            substitute_expr(a, ctes);
            substitute_expr(b, ctes);
        }
        Expr::Between(a, lo, hi, _) => {
            substitute_expr(a, ctes);
            substitute_expr(lo, ctes);
            substitute_expr(hi, ctes);
        }
        Expr::InList(a, list, _) => {
            substitute_expr(a, ctes);
            for x in list {
                substitute_expr(x, ctes);
            }
        }
        Expr::Like(a, p, esc, _) => {
            substitute_expr(a, ctes);
            substitute_expr(p, ctes);
            if let Some(x) = esc {
                substitute_expr(x, ctes);
            }
        }
        Expr::InSubquery(a, q, _) => {
            substitute_expr(a, ctes);
            substitute_query(q, ctes);
        }
        Expr::Exists(q, _) | Expr::Subquery(q) => substitute_query(q, ctes),
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            if let Some(o) = operand {
                substitute_expr(o, ctes);
            }
            for (w, t) in whens {
                substitute_expr(w, ctes);
                substitute_expr(t, ctes);
            }
            if let Some(o) = otherwise {
                substitute_expr(o, ctes);
            }
        }
        Expr::Function(f) => {
            for a in &mut f.args {
                substitute_expr(a, ctes);
            }
        }
        Expr::Row(items) => {
            for x in items {
                substitute_expr(x, ctes);
            }
        }
        _ => {}
    }
}

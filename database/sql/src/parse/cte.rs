// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Common table expressions: `WITH [RECURSIVE] name [(cols)] AS (...)`,
//! with `SEARCH` and `CYCLE`. A plain one becomes a derived table where
//! later expressions or the body name it; one that names itself is
//! recursive, its union split into the initial rows and the step, and
//! becomes a [`TableRef::Recursive`].

use super::Parser;
use crate::ast::{
    Cycle, Expr, Ident, Query, QueryBody, RecursiveCte, Search, SelectItem, TableRef,
};
use crate::error::{Result, SqlError};
use crate::lex::Tok;

/// A common table expression as the body sees it.
#[derive(Clone)]
enum Def {
    Plain(Box<Query>),
    Recursive(Box<RecursiveCte>),
}

type Cte = (Ident, Vec<Ident>, Def);

impl Parser {
    /// Whether a common table expression starts here (not `WITH NC`).
    pub(crate) fn starts_cte(&self) -> bool {
        self.is_word("WITH")
            && (self.is_word_at(2, "AS")
                || self.is_sym_at(2, "(")
                || (self.is_word_at(1, "RECURSIVE")
                    && (self.is_word_at(3, "AS") || self.is_sym_at(3, "("))))
    }

    /// `WITH ... query`.
    pub(crate) fn with_query(&mut self) -> Result<Query> {
        self.expect_word("WITH")?;
        if self.is_word("RECURSIVE") && !self.is_word_at(1, "AS") && !self.is_sym_at(1, "(") {
            self.advance();
        }
        let mut ctes: Vec<Cte> = Vec::new();
        loop {
            let name = self.ident()?;
            let columns = self.optional_ident_list_cte()?;
            self.expect_word("AS")?;
            self.expect_sym("(")?;
            let mut q = self.query()?;
            self.expect_sym(")")?;
            substitute_query(&mut q, &ctes);
            let search = self.search()?;
            let cycle = self.cycle()?;
            let def = if names_body(&q.body, &name) {
                Def::Recursive(Box::new(split(&name, &columns, &q, search, cycle)?))
            } else {
                if search.is_some() || cycle.is_some() {
                    return Err(SqlError::Unsupported {
                        what: "SEARCH or CYCLE on a common table expression that is not recursive"
                            .into(),
                    });
                }
                Def::Plain(Box::new(q))
            };
            ctes.push((name, columns, def));
            if !self.eat_sym(",") {
                break;
            }
        }
        let mut q = self.query()?;
        substitute_query(&mut q, &ctes);
        Ok(q)
    }

    fn optional_ident_list_cte(&mut self) -> Result<Vec<Ident>> {
        if self.is_sym("(") {
            self.ident_list()
        } else {
            Ok(Vec::new())
        }
    }

    fn search(&mut self) -> Result<Option<Search>> {
        if !self.eat_word("SEARCH") {
            return Ok(None);
        }
        let depth = if self.eat_word("DEPTH") {
            true
        } else {
            self.expect_word("BREADTH")?;
            false
        };
        self.expect_words(&["FIRST", "BY"])?;
        let mut by = vec![self.ident()?];
        while self.eat_sym(",") {
            by.push(self.ident()?);
        }
        self.expect_word("SET")?;
        Ok(Some(Search {
            depth,
            by,
            set: self.ident()?,
        }))
    }

    fn cycle(&mut self) -> Result<Option<Cycle>> {
        if !self.eat_word("CYCLE") {
            return Ok(None);
        }
        let mut columns = vec![self.ident()?];
        while self.eat_sym(",") {
            columns.push(self.ident()?);
        }
        self.expect_word("SET")?;
        let set = self.ident()?;
        self.expect_word("TO")?;
        let to = self.cycle_mark()?;
        self.expect_word("DEFAULT")?;
        let default = self.cycle_mark()?;
        let using = if self.eat_word("USING") {
            Some(self.ident()?)
        } else {
            None
        };
        Ok(Some(Cycle {
            columns,
            set,
            to,
            default,
            using,
        }))
    }

    fn cycle_mark(&mut self) -> Result<String> {
        match self.tok().clone() {
            Tok::Str(s) if s.chars().count() == 1 => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.expected("a one-character string")),
        }
    }
}

/// The union's sides, left to right; `false` when a side is a `UNION`
/// without `ALL`.
fn leaves(b: &QueryBody, out: &mut Vec<QueryBody>) -> bool {
    match b {
        QueryBody::Union {
            left,
            right,
            all,
            op,
        } => {
            let l = leaves(left, out);
            let r = leaves(right, out);
            l && r && *all && *op == crate::ast::SetOp::Union
        }
        other => {
            out.push(other.clone());
            true
        }
    }
}

fn split(
    name: &Ident,
    columns: &[Ident],
    q: &Query,
    search: Option<Search>,
    cycle: Option<Cycle>,
) -> Result<RecursiveCte> {
    if !q.order_by.is_empty() || q.fetch_first.is_some() || q.offset.is_some() {
        return Err(SqlError::Recursion {
            name: name.value.clone(),
            what: "ORDER BY or a row limit".into(),
            code: -342,
        });
    }
    let mut sides = Vec::new();
    if !leaves(&q.body, &mut sides) {
        return Err(SqlError::Recursion {
            name: name.value.clone(),
            what: "UNION without ALL".into(),
            code: -342,
        });
    }
    let (step, seed): (Vec<QueryBody>, Vec<QueryBody>) =
        sides.into_iter().partition(|b| names_body(b, name));
    if seed.is_empty() {
        return Err(SqlError::Recursion {
            name: name.value.clone(),
            what: "no initial rows: it names only itself".into(),
            code: -346,
        });
    }
    if columns.is_empty() {
        return Err(SqlError::Recursion {
            name: name.value.clone(),
            what: "no column list".into(),
            code: -343,
        });
    }
    Ok(RecursiveCte {
        name: name.clone(),
        columns: columns.to_vec(),
        seed,
        step,
        search,
        cycle,
    })
}

/// Whether a body names `name` as a table.
fn names_body(b: &QueryBody, name: &Ident) -> bool {
    match b {
        QueryBody::Select(s) => {
            s.from.iter().any(|t| names_table(t, name))
                || s.where_clause.as_ref().is_some_and(|w| names_expr(w, name))
                || s.items
                    .iter()
                    .any(|i| matches!(i, SelectItem::Expr(e, _) if names_expr(e, name)))
        }
        QueryBody::Values(_) => false,
        QueryBody::Union { left, right, .. } => names_body(left, name) || names_body(right, name),
        QueryBody::Nested(q) => names_body(&q.body, name),
    }
}

fn names_table(t: &TableRef, name: &Ident) -> bool {
    match t {
        TableRef::Table { name: n, .. } => n.schema.is_none() && n.name.value == name.value,
        TableRef::Subquery { query, .. } => names_body(&query.body, name),
        TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
            names_table(left, name) || names_table(right, name)
        }
        TableRef::Lateral(inner) => names_table(inner, name),
        _ => false,
    }
}

fn names_expr(e: &Expr, name: &Ident) -> bool {
    match e {
        Expr::Exists(q, _) | Expr::Subquery(q) => names_body(&q.body, name),
        Expr::InSubquery(a, q, _) => names_expr(a, name) || names_body(&q.body, name),
        e => crate::ast::visit::children(e)
            .into_iter()
            .any(|c| names_expr(c, name)),
    }
}

/// Every table reference of `q` that names a common table expression
/// becomes that expression.
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
            // The last definition of a name is the one in scope.
            if let Some((cname, ccols, def)) = ctes
                .iter()
                .rev()
                .find(|(n, _, _)| n.value == name.name.value)
            {
                let alias = alias.clone().unwrap_or_else(|| cname.clone());
                let cols = if columns.is_empty() {
                    ccols.clone()
                } else {
                    columns.clone()
                };
                *t = match def {
                    Def::Plain(q) => TableRef::Subquery {
                        query: q.clone(),
                        alias,
                        columns: cols,
                    },
                    Def::Recursive(r) => TableRef::Recursive {
                        cte: r.clone(),
                        alias,
                        columns: cols,
                    },
                };
            }
        }
        TableRef::Table { .. } | TableRef::DataChange { .. } | TableRef::Recursive { .. } => {}
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
        TableRef::JoinUsing { left, right, .. } => {
            substitute_table(left, ctes);
            substitute_table(right, ctes);
        }
        TableRef::Lateral(inner) => substitute_table(inner, ctes),
        TableRef::Function { args, .. } => {
            for a in args {
                substitute_expr(a, ctes);
            }
        }
    }
}

fn substitute_expr(e: &mut Expr, ctes: &[Cte]) {
    match e {
        Expr::InSubquery(a, q, _) => {
            substitute_expr(a, ctes);
            substitute_query(q, ctes);
        }
        Expr::Exists(q, _) | Expr::Subquery(q) => substitute_query(q, ctes),
        e => {
            for c in crate::ast::visit::children_mut(e) {
                substitute_expr(c, ctes);
            }
        }
    }
}

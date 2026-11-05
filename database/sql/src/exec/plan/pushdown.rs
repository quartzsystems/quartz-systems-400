// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Predicates pushed into derived tables: a conjunct of a `WHERE` that
//! names only the columns of a derived table in `FROM` (a nested table
//! expression, a view that runs its query, a common table expression)
//! becomes a conjunct of that table's own `WHERE`, its columns replaced
//! by the expressions the table's select list gives them. The table then
//! reads only the rows that pass, through an index where one serves.
//!
//! Only what filters the same rows either way is moved: the derived
//! query is a plain `SELECT` without `ORDER BY`, `FETCH FIRST`,
//! `OFFSET` or OLAP specifications; with `GROUP BY` only conditions on
//! grouping columns move (to `WHERE`), never on aggregates; the
//! conjunct and the expressions it takes have no subquery, sequence
//! reference or function outside a fixed set of deterministic built-ins.

use crate::ast::{ColumnRef, Expr, Query, QueryBody, Select, SelectItem, TableRef, columns_of, visit};

/// Built-in functions a pushed condition may call.
const PURE: &[&str] = &[
    "ABS", "CHAR", "COALESCE", "DATE", "DAY", "DEC", "DECIMAL", "DIGITS", "IFNULL", "INT", "INTEGER", "LCASE", "LEFT",
    "LENGTH", "LOWER", "LTRIM", "MOD", "MONTH", "RIGHT", "RTRIM", "STRIP", "SUBSTR", "SUBSTRING", "TRIM", "UCASE",
    "UPPER", "VALUE", "VARCHAR", "YEAR",
];

/// Whether `e` gives the same answer evaluated in either place.
fn movable(e: &Expr) -> bool {
    match e {
        Expr::Subquery(_)
        | Expr::Exists(..)
        | Expr::InSubquery(..)
        | Expr::NextValue(_)
        | Expr::PrevValue(_)
        | Expr::Over(_)
        | Expr::WithinGroup(_)
        | Expr::Default
        | Expr::Compare(_, _, _, Some(_)) => false,
        Expr::Function(f) if !PURE.contains(&f.name.as_str()) => false,
        e => visit::children(e).into_iter().all(movable),
    }
}

/// Split a condition into its `AND`ed parts.
fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::And(a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other.clone()),
    }
}

fn and_all(parts: Vec<Expr>) -> Option<Expr> {
    parts.into_iter().reduce(|a, b| Expr::And(Box::new(a), Box::new(b)))
}

/// The derived table's columns: name and the expression it stands for
/// inside; `None` when they cannot be told.
fn derived_columns(sel: &Select, names: &[crate::ast::Ident]) -> Option<Vec<(String, Expr)>> {
    let mut out = Vec::new();
    for (n, item) in sel.items.iter().enumerate() {
        match item {
            SelectItem::Expr(e, alias) => {
                let name = match (alias, e) {
                    (Some(a), _) => a.value.clone(),
                    (None, Expr::Column(c)) => c.column.value.clone(),
                    // An unnamed expression is named by its place.
                    _ => format!("{:05}", n + 1),
                };
                out.push((name, e.clone()));
            }
            // `*` of a single table: each column stands for itself.
            SelectItem::Star | SelectItem::TableStar(_) => return None,
        }
    }
    if !names.is_empty() {
        if names.len() != out.len() {
            return None;
        }
        for (o, n) in out.iter_mut().zip(names) {
            o.0.clone_from(&n.value);
        }
    }
    Some(out)
}

/// Whether the derived query may take conditions.
fn takes(q: &Query) -> Option<&Select> {
    if !q.order_by.is_empty() || q.offset.is_some() || q.fetch_first.is_some() || q.for_update.is_some() {
        return None;
    }
    let QueryBody::Select(sel) = &q.body else { return None };
    let olap = sel.items.iter().any(|i| matches!(i, SelectItem::Expr(e, _) if super::super::olap::has_over(e)));
    (!olap).then_some(&**sel)
}

/// `e` with the derived table's columns replaced; `None` when a column
/// is not one of them or its expression may not move.
fn substitute(e: &Expr, alias: &str, cols: &[(String, Expr)], grouped: &[Expr]) -> Option<Expr> {
    fn walk(e: &mut Expr, alias: &str, cols: &[(String, Expr)], grouped: &[Expr], ok: &mut bool) {
        if let Expr::Column(ColumnRef { table, column }) = e {
            if table.as_ref().is_some_and(|t| t.value != alias) {
                *ok = false;
                return;
            }
            match cols.iter().find(|(n, _)| *n == column.value) {
                Some((_, x)) if movable(x) && (grouped.is_empty() || grouped.contains(x)) && !super::super::eval::has_aggregate(x) => {
                    *e = x.clone();
                }
                _ => *ok = false,
            }
            return;
        }
        for c in visit::children_mut(e) {
            walk(c, alias, cols, grouped, ok);
        }
    }
    let mut out = e.clone();
    let mut ok = true;
    walk(&mut out, alias, cols, grouped, &mut ok);
    ok.then_some(out)
}

/// `sel` with the conjuncts of its `WHERE` that only its derived tables
/// need moved into them; `None` when none moves. `elsewhere` tells
/// whether an unqualified name is a column of another `FROM` item.
pub fn push(sel: &Select, elsewhere: &dyn Fn(&str) -> bool) -> Option<Select> {
    let w = sel.where_clause.as_ref()?;
    if !sel.from.iter().any(|t| matches!(t, TableRef::Subquery { .. })) {
        return None;
    }
    let mut parts = Vec::new();
    conjuncts(w, &mut parts);
    let mut out = sel.clone();
    let mut kept = Vec::new();
    let mut moved = false;
    for p in parts {
        let cols = columns_of(&p);
        if cols.is_empty() || !movable(&p) {
            kept.push(p);
            continue;
        }
        // The one derived table every column names.
        let mut target: Option<usize> = None;
        let mut single = true;
        for c in &cols {
            let hits: Vec<usize> = out
                .from
                .iter()
                .enumerate()
                .filter(|(_, t)| match t {
                    TableRef::Subquery { alias, query, columns } => match &c.table {
                        Some(t) => t.value == alias.value,
                        None => {
                            !elsewhere(&c.column.value)
                                && takes(query).and_then(|s| derived_columns(s, columns)).is_some_and(|d| d.iter().any(|(n, _)| *n == c.column.value))
                        }
                    },
                    _ => false,
                })
                .map(|(i, _)| i)
                .collect();
            // A name two tables have is left to the query to refuse.
            let hit = if hits.len() == 1 { Some(hits[0]) } else { None };
            match (hit, target) {
                (Some(h), None) => target = Some(h),
                (Some(h), Some(t)) if h == t => {}
                _ => single = false,
            }
        }
        let Some(ti) = target.filter(|_| single) else {
            kept.push(p);
            continue;
        };
        let TableRef::Subquery { alias, query, columns } = &mut out.from[ti] else { unreachable!("a derived table") };
        let Some(inner) = takes(query) else {
            kept.push(p);
            continue;
        };
        let Some(dcols) = derived_columns(inner, columns) else {
            kept.push(p);
            continue;
        };
        let grouped = inner.group_by.clone();
        // A query that aggregates without grouping gives one row whatever
        // it reads: nothing moves into it.
        let aggregates = grouped.is_empty() && inner.items.iter().any(|i| matches!(i, SelectItem::Expr(e, _) if super::super::eval::has_aggregate(e)));
        if aggregates || super::super::grouping::has_sets(&grouped) {
            kept.push(p);
            continue;
        }
        let Some(cond) = substitute(&p, &alias.value, &dcols, &grouped) else {
            kept.push(p);
            continue;
        };
        let QueryBody::Select(s) = &mut query.body else { unreachable!("checked") };
        s.where_clause = Some(match s.where_clause.take() {
            Some(w) => Expr::And(Box::new(w), Box::new(cond)),
            None => cond,
        });
        moved = true;
    }
    if !moved {
        return None;
    }
    out.where_clause = and_all(kept);
    Some(out)
}

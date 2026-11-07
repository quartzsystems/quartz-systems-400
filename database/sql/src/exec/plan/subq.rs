// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Subqueries run once rather than once per row. A subquery that read
//! no outer column the first time it ran answers the rest of the
//! statement from what it gave. One that did, and is correlated only by
//! equalities of its own columns with outer expressions, is decorrelated
//! the second time it runs: it runs once without those equalities,
//! grouped by its side of them where it aggregates, and its rows are
//! kept by the values of that side; each outer row then takes the rows
//! of its own values. An outer row whose values have no rows takes none,
//! or, for a subquery that aggregates without grouping, runs the
//! subquery itself (an aggregate over no rows still gives a row).
//!
//! Whatever cannot be decorrelated (other correlation, a derived table
//! inside, values of mixed kinds, an error in the single run) runs per
//! row as before, so the answers stay the same. Nothing is kept while a
//! recursive common table expression's step runs: its name stands for
//! other rows each time.

use std::collections::HashMap;
use std::rc::Rc;

use store::txn::Txn;

use super::super::eval::{Env, Evaluator, Row, has_aggregate};
use super::super::mem::Charge;
use super::super::query::{correlation, run_query};
use crate::ast::{Expr, Ident, Query, QueryBody, SelectItem};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// What is known about one subquery of the statement.
enum Entry {
    /// It reads no outer column: its rows.
    Fixed(Rc<Vec<Row>>),
    /// Correlated: how many times it ran.
    Runs(u32),
    /// Its rows by the values of its correlating expressions.
    Keyed(Keyed),
    /// Runs per row.
    PerRow,
}

struct Keyed {
    /// The outer side of each correlating equality.
    outer: Vec<Expr>,
    /// The kind of value each key position holds.
    kinds: Vec<Option<u8>>,
    rows: HashMap<String, Rc<Vec<Row>>>,
    /// It aggregates without grouping: values without rows run it.
    aggregates: bool,
    _charge: Charge,
}

/// The subqueries of the statement running.
#[derive(Default)]
pub struct Cache {
    entries: HashMap<usize, (Query, Entry)>,
    /// The outermost frame a column read since the current subquery
    /// started.
    reach: usize,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl Cache {
    /// A statement starts: nothing kept.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.reach = usize::MAX;
    }

    /// A column was read from frame `index`.
    pub fn touch(&mut self, index: usize) {
        self.reach = self.reach.min(index);
    }
}

/// Kinds whose `key()` is equal exactly when they compare equal.
fn class(v: &Value) -> Option<u8> {
    Some(match v {
        Value::Num(_) => 1,
        Value::Str(_) => 2,
        Value::Bin(_) => 3,
        Value::Bool(_) => 4,
        Value::Date(_) => 5,
        Value::Time(_) => 6,
        Value::Timestamp(_) => 7,
        _ => return None,
    })
}

fn key(q: &Query) -> usize {
    std::ptr::from_ref(q) as usize
}

/// Run `q` for the outer rows `env`, noting whether it read them.
fn run_noted(
    ev: &mut Evaluator<'_>,
    txn: &mut Txn<'_>,
    q: &Query,
    env: &Env,
) -> Result<(Rc<Vec<Row>>, bool)> {
    let saved = std::mem::replace(&mut ev.ctx.plan.subq.reach, usize::MAX);
    let r = run_query(txn, ev.ctx, ev.params, q, env);
    let reach = ev.ctx.plan.subq.reach;
    ev.ctx.plan.subq.reach = saved.min(reach);
    Ok((Rc::new(r?.rows), reach < env.frames.len()))
}

/// A subquery's rows for the outer rows `env`.
pub fn answer(
    ev: &mut Evaluator<'_>,
    txn: &mut Txn<'_>,
    q: &Query,
    env: &Env,
) -> Result<Rc<Vec<Row>>> {
    if !ev.ctx.ctes.is_empty() {
        return Ok(Rc::new(run_query(txn, ev.ctx, ev.params, q, env)?.rows));
    }
    let k = key(q);
    let known = ev
        .ctx
        .plan
        .subq
        .entries
        .get_mut(&k)
        .filter(|(kept, _)| kept == q)
        .map(|(_, e)| e);
    match known {
        Some(Entry::Fixed(rows)) => return Ok(Rc::clone(rows)),
        Some(Entry::PerRow) => return Ok(run_noted(ev, txn, q, env)?.0),
        Some(Entry::Runs(n)) => {
            *n += 1;
            if *n >= 2 {
                let built = decorrelate(ev, txn, q)?;
                let entry = built.map_or(Entry::PerRow, Entry::Keyed);
                ev.ctx.plan.subq.entries.insert(k, (q.clone(), entry));
                return answer(ev, txn, q, env);
            }
            return Ok(run_noted(ev, txn, q, env)?.0);
        }
        Some(Entry::Keyed(_)) => return keyed(ev, txn, q, env, k),
        None => {}
    }
    let (rows, correlated) = run_noted(ev, txn, q, env)?;
    let entry = if correlated {
        Entry::Runs(1)
    } else {
        Entry::Fixed(Rc::clone(&rows))
    };
    ev.ctx.plan.subq.entries.insert(k, (q.clone(), entry));
    Ok(rows)
}

/// The rows of a decorrelated subquery for `env`'s values.
fn keyed(
    ev: &mut Evaluator<'_>,
    txn: &mut Txn<'_>,
    q: &Query,
    env: &Env,
    k: usize,
) -> Result<Rc<Vec<Row>>> {
    let outer = match ev.ctx.plan.subq.entries.get(&k) {
        Some((_, Entry::Keyed(kd))) => kd.outer.clone(),
        _ => unreachable!("keyed"),
    };
    let mut values = Vec::with_capacity(outer.len());
    for e in &outer {
        values.push(ev.eval(txn, e, env)?);
    }
    // A routine the values called may have started a statement of its own.
    let Some((_, Entry::Keyed(kd))) = ev.ctx.plan.subq.entries.get(&k) else {
        return Ok(run_noted(ev, txn, q, env)?.0);
    };
    let aggregates = kd.aggregates;
    let mut text = String::new();
    let mut usable = true;
    let mut absent = false;
    for (v, kind) in values.iter().zip(&kd.kinds) {
        if v.is_null() {
            absent = true;
            break;
        }
        if kind.is_some() && class(v) != *kind {
            usable = false;
            break;
        }
        text.push_str(&v.key());
        text.push('\u{1}');
    }
    let found = if usable && !absent {
        kd.rows.get(&text).cloned()
    } else {
        None
    };
    match found {
        Some(rows) => Ok(rows),
        None if usable && !aggregates => Ok(Rc::new(Vec::new())),
        None => Ok(run_noted(ev, txn, q, env)?.0),
    }
}

/// Run `q` once without its correlation, its rows by key; `None` when
/// it cannot be.
fn decorrelate(ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, q: &Query) -> Result<Option<Keyed>> {
    if q.offset.is_some() || q.fetch_first.is_some() {
        return Ok(None);
    }
    let QueryBody::Select(sel) = &q.body else {
        return Ok(None);
    };
    let olap = sel
        .items
        .iter()
        .any(|i| matches!(i, SelectItem::Expr(e, _) if super::super::olap::has_over(e)));
    if olap || super::super::grouping::has_sets(&sel.group_by) {
        return Ok(None);
    }
    let Some((mut inner, pairs)) = correlation(txn, ev.ctx, sel)? else {
        return Ok(None);
    };
    let aggregates = sel.group_by.is_empty()
        && (sel
            .items
            .iter()
            .any(|i| matches!(i, SelectItem::Expr(e, _) if has_aggregate(e)))
            || sel.having.as_ref().is_some_and(has_aggregate));
    let grouped = !sel.group_by.is_empty() || aggregates;
    let n = pairs.len();
    for (i, (inside, _)) in pairs.iter().enumerate() {
        inner.items.push(SelectItem::Expr(
            inside.clone(),
            Some(Ident {
                value: format!("*K{i}"),
                delimited: true,
            }),
        ));
    }
    if grouped {
        let mut by: Vec<Expr> = pairs.iter().map(|(inside, _)| inside.clone()).collect();
        by.extend(inner.group_by.iter().cloned());
        inner.group_by = by;
    }
    let mut once = q.clone();
    once.body = QueryBody::Select(Box::new(inner));
    // Run with no outer rows: a column the split missed fails here, and
    // the subquery runs per row instead.
    let rs = match run_query(txn, ev.ctx, ev.params, &once, &Env::default()) {
        Ok(rs) => rs,
        Err(e @ SqlError::Cancelled) => return Err(e),
        Err(_) => return Ok(None),
    };
    let width = rs.columns.len().saturating_sub(n);
    let mut charge = ev.ctx.charge();
    let mut kinds: Vec<Option<u8>> = vec![None; n];
    let mut by: HashMap<String, Vec<Row>> = HashMap::new();
    for mut r in rs.rows {
        let keys = r.split_off(width);
        if keys.iter().any(Value::is_null) {
            continue;
        }
        let mut text = String::new();
        for (v, kind) in keys.iter().zip(&mut kinds) {
            let c = class(v);
            if c.is_none() || kind.is_some_and(|k| Some(k) != c) {
                return Ok(None);
            }
            *kind = c;
            text.push_str(&v.key());
            text.push('\u{1}');
        }
        if charge
            .add(text.len() + super::super::mem::row_bytes(&r))
            .is_err()
        {
            return Ok(None);
        }
        by.entry(text).or_default().push(r);
    }
    Ok(Some(Keyed {
        outer: pairs.into_iter().map(|(_, o)| o).collect(),
        kinds,
        rows: by.into_iter().map(|(k, v)| (k, Rc::new(v))).collect(),
        aggregates,
        _charge: charge,
    }))
}

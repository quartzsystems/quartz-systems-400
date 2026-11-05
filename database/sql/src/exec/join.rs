// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Joins as nested loops that keep only the rows that match: an
//! explicit `JOIN` of two `FROM` items, and the comma-separated items
//! walked depth first, each `WHERE` conjunct tested as soon as the
//! items it names are bound. Neither builds the cross product. Where a
//! conjunct is an equality between a column of the rows so far and a
//! column of the next item, that item's rows may be found by key (a hash
//! of the column's values, or a sort-merge of both sides) instead of
//! tried one by one, whichever costs least (`method`); every condition
//! is still evaluated on the rows found, so the answer and its order are
//! the nested loop's.

mod merge;
pub mod method;

use std::collections::HashMap;
use std::rc::Rc;

use store::txn::Txn;

use super::eval::{Env, Evaluator, Row, Schema};
use super::mem::Charge;
use crate::ast::{CmpOp, Expr, JoinKind};
use crate::error::Result;
use crate::value::Value;
pub use method::Method;

/// Rows of the next item by the key of one column.
struct Probe {
    /// The column of the rows so far.
    left: usize,
    /// Right rows by key, in their order.
    rows: HashMap<String, Vec<usize>>,
    /// The kind of every non-null right value; `None` when all are null.
    class: Option<u8>,
    /// What the index holds.
    _charge: Charge,
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

/// The column `e` names, when it is a column of `schema`.
fn column(schema: &Schema, e: &Expr) -> Option<usize> {
    let Expr::Column(c) = e else {
        return None;
    };
    schema
        .find(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value)
        .ok()
        .flatten()
}

/// The equalities of `preds` between a column of `ls` only and a
/// column of `rs` only: (left column, right column).
fn equalities(preds: &[&Expr], ls: &Schema, rs: &Schema) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for p in preds {
        let Expr::Compare(x, CmpOp::Eq, y, None) = p else {
            continue;
        };
        let at = |e: &Expr| (column(ls, e), column(rs, e));
        if let ((Some(left), None), (None, Some(col))) | ((None, Some(col)), (Some(left), None)) = (at(x), at(y)) {
            out.push((left, col));
        }
    }
    out
}

/// Whether `preds` hold an equality of a column of `ls` and one of `rs`.
pub(super) fn equality_in(preds: &[Expr], ls: &Schema, rs: &Schema) -> bool {
    let refs: Vec<&Expr> = preds.iter().collect();
    !equalities(&refs, ls, rs).is_empty()
}

/// What a hash table over `n` right rows holds, near enough.
fn hash_bytes(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX).saturating_mul(48)
}

/// How the rows a left row may match are found.
enum Finder {
    /// Every right row.
    All,
    Hash(Probe),
    Merge(merge::Runs),
}

impl Finder {
    /// Choose by cost and build. `left_sorted`/`right_sorted` are
    /// worked out only when a merge could win.
    #[allow(clippy::too_many_arguments)]
    fn build(
        ev: &mut Evaluator<'_>,
        preds: &[&Expr],
        ls: &Schema,
        rs: &Schema,
        left: &[Row],
        right: &[Row],
        goal: Option<u64>,
    ) -> Result<(Method, Self)> {
        let eqs = equalities(preds, ls, rs);
        let n = |x: usize| u64::try_from(x).unwrap_or(u64::MAX);
        let room = n(ev.ctx.charge().room());
        let mut inputs = method::Inputs {
            left: n(left.len()),
            right: n(right.len()),
            equality: !eqs.is_empty(),
            hash_bytes: hash_bytes(right.len()),
            room,
            goal,
            ..method::Inputs::default()
        };
        if let Some(&(lc, rc)) = eqs.first()
            && method::choose(&inputs) != Method::NestedLoop
        {
            inputs.left_sorted = merge::in_order(left.iter().map(|r| &r[lc]));
            inputs.right_sorted = merge::in_order(right.iter().map(|r| &r[rc]));
        }
        let m = method::choose(&inputs);
        match m {
            Method::NestedLoop => Ok((m, Self::All)),
            Method::Hash => match Probe::build(ev.ctx.charge(), preds, ls, rs, right)? {
                Some(p) => Ok((m, Self::Hash(p))),
                None => Ok((Method::NestedLoop, Self::All)),
            },
            Method::Merge => {
                let (lc, rc) = eqs[0];
                match merge::Runs::build(ev.ctx.charge(), left, lc, inputs.left_sorted, right, rc, inputs.right_sorted)? {
                    Some(r) => Ok((m, Self::Merge(r))),
                    None => Ok((Method::NestedLoop, Self::All)),
                }
            }
        }
    }

    fn lookup(&self, li: usize, l: &Row) -> Option<&[usize]> {
        match self {
            Self::All => None,
            Self::Hash(p) => p.lookup(l),
            Self::Merge(r) => r.lookup(li),
        }
    }
}

impl Probe {
    /// From the first equality of `preds` between a column of `ls` only
    /// and a column of `rs` only whose right values hash alike.
    fn build(
        charge: Charge,
        preds: &[&Expr],
        ls: &Schema,
        rs: &Schema,
        right: &[Row],
    ) -> Result<Option<Self>> {
        let mut charge = charge;
        for (left, col) in equalities(preds, ls, rs) {
            let mut rows: HashMap<String, Vec<usize>> = HashMap::new();
            let mut kind = None;
            let mut usable = true;
            for (i, row) in right.iter().enumerate() {
                let value = &row[col];
                if value.is_null() {
                    continue;
                }
                let this = class(value);
                if this.is_none() || kind.is_some_and(|k| Some(k) != this) {
                    usable = false;
                    break;
                }
                kind = this;
                let key = value.key();
                charge.add(key.len() + 3 * std::mem::size_of::<usize>())?;
                rows.entry(key).or_default().push(i);
            }
            if !usable {
                charge.clear();
                continue;
            }
            return Ok(Some(Self {
                left,
                rows,
                class: kind,
                _charge: charge,
            }));
        }
        Ok(None)
    }

    /// The right rows `l` may match, in order; `None` when any may.
    fn lookup(&self, l: &Row) -> Option<&[usize]> {
        let v = &l[self.left];
        if v.is_null() || self.class.is_none() {
            return Some(&[]);
        }
        if class(v) != self.class {
            return None;
        }
        Some(self.rows.get(&v.key()).map_or(&[], Vec::as_slice))
    }
}

/// Split a condition into its `AND`ed parts.
fn conjuncts<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    match e {
        Expr::And(a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other),
    }
}

/// Whether `row` passes every predicate; the row back.
fn passes(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    outer: &Env,
    schema: &Rc<Schema>,
    preds: &[Expr],
    row: Row,
) -> Result<(bool, Row)> {
    if preds.is_empty() {
        return Ok((true, row));
    }
    let mut env = outer.with(schema.clone(), row);
    let mut ok = true;
    for p in preds {
        if ev.truth(txn, p, &env)? != Some(true) {
            ok = false;
            break;
        }
    }
    Ok((ok, env.frames.pop().expect("pushed").1))
}

/// An explicit `JOIN` of two items' rows: the matches, left rows in
/// order, each with its right rows in order, then the unmatched ones an
/// outer join keeps.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn join(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    schema: &Rc<Schema>,
    ls: &Schema,
    rs: &Schema,
    left_rows: &[Row],
    right_rows: &[Row],
    kind: JoinKind,
    on: Option<&Expr>,
    outer: &Env,
) -> Result<Vec<Row>> {
    let mut parts = Vec::new();
    if let Some(e) = on {
        conjuncts(e, &mut parts);
    }
    let goal = ev.ctx.plan.goal;
    let (method, finder) = Finder::build(ev, &parts, ls, rs, left_rows, right_rows, goal)?;
    ev.ctx.plan.joined(method);
    let mut charge = ev.ctx.charge();
    let rnull = vec![Value::Null; rs.cols.len()];
    let keeps_match = matches!(
        kind,
        JoinKind::Inner | JoinKind::Cross | JoinKind::Left | JoinKind::Right | JoinKind::Full
    );
    let keeps_left = matches!(kind, JoinKind::Left | JoinKind::LeftException | JoinKind::Full);
    let keeps_right = matches!(kind, JoinKind::Right | JoinKind::RightException | JoinKind::Full);
    let mut right_matched = vec![false; right_rows.len()];
    let all: Vec<usize> = (0..right_rows.len()).collect();
    // Rows already read: nothing held from the store, a yield between
    // any two pairs.
    let mut out = Vec::new();
    for (li, l) in left_rows.iter().enumerate() {
        let mut matched = false;
        let cands = finder.lookup(li, l).unwrap_or(&all);
        for &ri in cands {
            ev.ctx.tick(txn)?;
            let mut combined = l.clone();
            combined.extend(right_rows[ri].iter().cloned());
            let (hit, combined) = match on {
                None => (true, combined),
                Some(e) => {
                    let mut env = outer.with(schema.clone(), combined);
                    let hit = ev.truth(txn, e, &env)? == Some(true);
                    (hit, env.frames.pop().expect("pushed").1)
                }
            };
            if hit {
                matched = true;
                right_matched[ri] = true;
                if keeps_match {
                    charge.row(&combined)?;
                    out.push(combined);
                }
            }
        }
        if !matched && keeps_left {
            let mut row = l.clone();
            row.extend(rnull.iter().cloned());
            charge.row(&row)?;
            out.push(row);
        }
        ev.ctx.tick(txn)?;
    }
    if keeps_right {
        let lnull = vec![Value::Null; ls.cols.len()];
        for (ri, r) in right_rows.iter().enumerate() {
            if !right_matched[ri] {
                let mut row = lnull.clone();
                row.extend(r.iter().cloned());
                charge.row(&row)?;
                out.push(row);
            }
        }
    }
    Ok(out)
}

/// One comma-separated `FROM` item: its schema and rows.
pub(super) struct Item {
    pub(super) schema: Rc<Schema>,
    pub(super) rows: Vec<Row>,
}

/// The items' rows joined in order, depth first: `preds[k]` is tested
/// once items `0..=k` are bound. The rows, and the charge for them.
pub(super) fn product(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    outer: &Env,
    mut items: Vec<Item>,
    preds: &[Vec<Expr>],
) -> Result<(Rc<Schema>, Vec<Row>, Charge)> {
    // The schema once items 0..=k are bound.
    let mut schemas: Vec<Rc<Schema>> = Vec::with_capacity(items.len());
    for it in &items {
        schemas.push(match schemas.last() {
            None => it.schema.clone(),
            Some(s) => {
                let mut cols = s.cols.clone();
                cols.extend(it.schema.cols.iter().cloned());
                Rc::new(Schema { cols })
            }
        });
    }
    let schema = schemas.last().cloned().unwrap_or_default();
    let mut charge = ev.ctx.charge();
    // An empty item: no rows, and nothing evaluated.
    if items.iter().any(|it| it.rows.is_empty()) {
        return Ok((schema, Vec::new(), charge));
    }
    let mut probes = vec![None];
    for k in 1..items.len() {
        let refs: Vec<&Expr> = preds[k].iter().collect();
        // The rows bound so far are made as the walk goes: the largest
        // item before stands for them. A merge needs them all at once,
        // so the walk weighs the loop against the hash join only.
        let n = |x: usize| u64::try_from(x).unwrap_or(u64::MAX);
        let before = items[..k].iter().map(|it| it.rows.len()).max().unwrap_or(0);
        let inputs = method::Inputs {
            left: n(before),
            right: n(items[k].rows.len()),
            equality: !equalities(&refs, &schemas[k - 1], &items[k].schema).is_empty(),
            hash_bytes: hash_bytes(items[k].rows.len()),
            room: n(ev.ctx.charge().room()),
            goal: ev.ctx.plan.goal,
            ..method::Inputs::default()
        };
        let probe = if method::choose(&inputs) == Method::NestedLoop {
            None
        } else {
            Probe::build(ev.ctx.charge(), &refs, &schemas[k - 1], &items[k].schema, &items[k].rows)?
        };
        ev.ctx.plan.joined(if probe.is_some() { Method::Hash } else { Method::NestedLoop });
        probes.push(probe);
    }
    let first = std::mem::take(&mut items[0].rows);
    let walk = Walk {
        outer,
        items: &items,
        schemas: &schemas,
        preds,
        probes: &probes,
    };
    let mut out = Vec::new();
    for row in first {
        ev.ctx.tick(txn)?;
        let (ok, row) = passes(txn, ev, outer, &schemas[0], &preds[0], row)?;
        if ok {
            walk.descend(txn, ev, 1, row, &mut out, &mut charge)?;
        }
    }
    Ok((schema, out, charge))
}

/// What the depth-first walk of [`product`] reads.
struct Walk<'a> {
    outer: &'a Env,
    items: &'a [Item],
    schemas: &'a [Rc<Schema>],
    preds: &'a [Vec<Expr>],
    probes: &'a [Option<Probe>],
}

impl Walk<'_> {
    /// Extend `row`, which binds items `0..k`, by item `k` onward.
    fn descend(
        &self,
        txn: &mut Txn<'_>,
        ev: &mut Evaluator<'_>,
        k: usize,
        row: Row,
        out: &mut Vec<Row>,
        charge: &mut Charge,
    ) -> Result<()> {
        if k == self.items.len() {
            charge.row(&row)?;
            out.push(row);
            return Ok(());
        }
        let rows = &self.items[k].rows;
        let found = self.probes[k].as_ref().and_then(|p| p.lookup(&row));
        let mut each = |txn: &mut Txn<'_>, ev: &mut Evaluator<'_>, r: &Row| -> Result<()> {
            ev.ctx.tick(txn)?;
            let mut next = Vec::with_capacity(row.len() + r.len());
            next.extend(row.iter().cloned());
            next.extend(r.iter().cloned());
            let (ok, next) = passes(txn, ev, self.outer, &self.schemas[k], &self.preds[k], next)?;
            if ok {
                self.descend(txn, ev, k + 1, next, out, charge)?;
            }
            Ok(())
        };
        match found {
            Some(idx) => {
                for &i in idx {
                    each(txn, ev, &rows[i])?;
                }
            }
            None => {
                for r in rows {
                    each(txn, ev, r)?;
                }
            }
        }
        Ok(())
    }
}

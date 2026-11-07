// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Several indexes at once: where no one path serves a table's
//! predicates, the relative record numbers each servable conjunct's
//! paths select are gathered (a disjunction or `IN` list takes the union
//! of its parts', several conjuncts the intersection of theirs) and the
//! records are read in that order, skipping the rest of the member. The
//! order is the arrival order a full scan would read in, and every
//! predicate is still tested on the rows read, so the answer is the full
//! scan's.

use std::collections::BTreeSet;
use std::ops::Bound;

use db::file;
use db::member::Member;
use store::txn::Txn;

use super::{Candidate, KEY_BATCH, simple_predicate};
use crate::ast::{CmpOp, Expr, columns_of};
use crate::error::{Result, SqlError};
use crate::exec::eval::{Env, Evaluator};

/// One key range over one path: the first key field's bounds.
struct Range {
    cand: usize,
    lo: Option<Expr>,
    hi: Option<Expr>,
}

/// The ranges of a simple predicate `col op value`, on the first path
/// whose first key is `col` (ascending).
fn range_of(cands: &[Candidate], e: &Expr) -> Option<Vec<Range>> {
    let one = |name: &str, lo: Option<Expr>, hi: Option<Expr>| {
        cands
            .iter()
            .position(|c| c.keys.first().is_some_and(|(k, d)| k == name && !d))
            .map(|cand| Range { cand, lo, hi })
    };
    match e {
        Expr::Or(left, right) => {
            let mut out = range_of(cands, left)?;
            out.extend(range_of(cands, right)?);
            Some(out)
        }
        Expr::InList(col, list, false) => {
            let Expr::Column(c) = &**col else { return None };
            if list
                .iter()
                .any(|x| !columns_of(x).is_empty() || super::has_subquery(x))
            {
                return None;
            }
            list.iter()
                .map(|v| one(&c.column.value, Some(v.clone()), Some(v.clone())))
                .collect()
        }
        Expr::Between(col, low, high, false) => {
            let Expr::Column(c) = &**col else { return None };
            if !columns_of(low).is_empty()
                || !columns_of(high).is_empty()
                || super::has_subquery(low)
                || super::has_subquery(high)
            {
                return None;
            }
            Some(vec![one(
                &c.column.value,
                Some((**low).clone()),
                Some((**high).clone()),
            )?])
        }
        e => {
            let (name, op, v) = simple_predicate(e)?;
            let r = match op {
                CmpOp::Eq => one(&name, Some(v.clone()), Some(v)),
                CmpOp::Gt | CmpOp::Ge => one(&name, Some(v), None),
                CmpOp::Lt | CmpOp::Le => one(&name, None, Some(v)),
                CmpOp::Ne => None,
            }?;
            Some(vec![r])
        }
    }
}

/// The ranges to read for `preds`: each conjunct that ranges can serve,
/// as one set; `None` unless a disjunction or `IN` list is among them or
/// two or more conjuncts are.
fn plan(cands: &[Candidate], preds: &[Expr]) -> Option<Vec<Vec<Range>>> {
    let mut sets = Vec::new();
    let mut union = false;
    for p in preds {
        if let Some(r) = range_of(cands, p) {
            union |= matches!(p, Expr::Or(..) | Expr::InList(..));
            sets.push(r);
        }
    }
    (union || sets.len() >= 2).then_some(sets)
}

/// The path entries' record numbers within one range.
fn rrns_of(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    c: &Candidate,
    r: &Range,
    outer: &Env,
    out: &mut BTreeSet<u64>,
) -> Result<()> {
    let key = |ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, e: &Expr| -> Result<Option<Vec<u8>>> {
        let v = ev.eval(txn, e, outer)?;
        if v.is_null() {
            return Ok(None);
        }
        let k = &c.fmt.format.keys[0];
        let dbv = v.to_field(&c.fmt.format.fields[k.field])?;
        let mut enc = db::path::values_key(&c.fmt, &[dbv]).map_err(|e| SqlError::Conversion {
            what: e.to_string(),
        })?;
        if let Some((number, _)) = c.cst {
            enc.splice(0..0, number.to_be_bytes());
        }
        Ok(Some(enc))
    };
    let lo = match &r.lo {
        Some(e) => match key(ev, txn, e)? {
            Some(k) => k,
            None => return Ok(()),
        },
        None => c
            .cst
            .map(|(n, _)| n.to_be_bytes().to_vec())
            .unwrap_or_default(),
    };
    let len = c.cst.map_or(usize::from(c.fmt.key_len), |(_, l)| l);
    let mut hi = match &r.hi {
        Some(e) => match key(ev, txn, e)? {
            Some(k) => k,
            None => return Ok(()),
        },
        None => c
            .cst
            .map(|(n, _)| n.to_be_bytes().to_vec())
            .unwrap_or_default(),
    };
    hi.resize(len, 0xFF);
    let desc = file::describe(txn, c.file)?;
    let mut pm = Member::open(txn, &desc, &c.member)?;
    pm.ensure_path(txn, &mut |_, _| {})?;
    let path = if c.cst.is_some() {
        pm.cst_tree()
    } else {
        pm.path()
    }
    .ok_or_else(|| SqlError::Database {
        what: format!("{} has no access path", c.label),
        msgid: None,
    })?;
    let mut batch = path.scan(
        txn,
        Bound::Included(lo.as_slice()),
        Bound::Included(hi.as_slice()),
        KEY_BATCH,
    )?;
    while let Some(last) = batch.last().cloned() {
        for k in &batch {
            if k.key.as_slice() > hi.as_slice() {
                return Ok(());
            }
            out.insert(k.rrn);
        }
        batch = path.scan_from(txn, Bound::Excluded(&last), KEY_BATCH)?;
    }
    Ok(())
}

/// The record numbers to read for `preds` in arrival order, with the
/// paths used; `None` when several paths do not serve them.
pub(super) fn rrns(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    cands: &[Candidate],
    preds: &[Expr],
    outer: &Env,
) -> Result<Option<(Vec<u64>, Vec<usize>)>> {
    let Some(sets) = plan(cands, preds) else {
        return Ok(None);
    };
    let mut used: Vec<usize> = Vec::new();
    let mut acc: Option<BTreeSet<u64>> = None;
    for set in &sets {
        let mut one = BTreeSet::new();
        for r in set {
            if !used.contains(&r.cand) {
                used.push(r.cand);
            }
            rrns_of(txn, ev, &cands[r.cand], r, outer, &mut one)?;
        }
        acc = Some(match acc {
            None => one,
            Some(a) => a.intersection(&one).copied().collect(),
        });
    }
    Ok(acc.map(|a| (a.into_iter().collect(), used)))
}

/// What the plan text says of it.
pub(super) fn describe(cands: &[Candidate], preds: &[Expr]) -> Option<String> {
    let sets = plan(cands, preds)?;
    let mut names: Vec<&str> = Vec::new();
    for r in sets.iter().flatten() {
        let n = cands[r.cand].label.as_str();
        if !names.contains(&n) {
            names.push(n);
        }
    }
    Some(format!(
        "bitmap over {} ranges of {}",
        sets.iter().map(Vec::len).sum::<usize>(),
        names.join(", ")
    ))
}

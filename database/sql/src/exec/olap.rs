// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! OLAP specifications: a function over a window of the rows a select
//! produces (after `WHERE`, grouping and `HAVING`). Each specification's
//! values are worked out for every row before the select list is
//! evaluated; the evaluator finds them by slot. Rows are partitioned by
//! `PARTITION BY`, ordered by the window's `ORDER BY` (stable), and the
//! frame (`ROWS` or `RANGE`, by default the rows up to the current row's
//! last peer) gives the rows an aggregate or value function sees.

#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use qdecimal::{DecFloat, DfFormat};
use store::txn::Txn;

use super::agg;
use crate::ast::visit::children;
use super::eval::{Env, Evaluator, binary, compare};
use super::sort::{KeySpec, cmp_keys};
use super::rtype::{Scope, SqlType, infer};
use crate::ast::{BinOp, CmpOp, Expr, FrameBound, Function, OrderItem, Over, Overflow};
use crate::error::{Result, SqlError};
use crate::value::{Value, decfloat_error};

/// The OLAP specifications in `e`, outermost first.
pub(crate) fn collect<'a>(e: &'a Expr, out: &mut Vec<&'a Over>) {
    if let Expr::Over(o) = e {
        out.push(o);
        return;
    }
    for c in children(e) {
        collect(c, out);
    }
}

/// Whether `e` holds an OLAP specification.
pub(crate) fn has_over(e: &Expr) -> bool {
    let mut v = Vec::new();
    collect(e, &mut v);
    !v.is_empty()
}

/// The function of a specification, with its ordering and overflow.
fn parts(o: &Over) -> Result<(&Function, &[OrderItem], Option<&Overflow>)> {
    match &o.func {
        Expr::Function(f) => Ok((f, &[], None)),
        Expr::WithinGroup(w) => Ok((&w.func, &w.order, w.overflow.as_ref())),
        _ => Err(SqlError::Unsupported { what: "OVER after an expression that is not a function".into() }),
    }
}

/// The result type of a specification.
pub(crate) fn result_type(o: &Over, scope: &Scope<'_>) -> SqlType {
    let Ok((f, _, _)) = parts(o) else { return SqlType::unknown() };
    let arg = f.args.first().map(|a| infer(a, scope));
    match f.name.as_str() {
        "ROW_NUMBER" | "RANK" | "DENSE_RANK" => SqlType::bigint(false),
        "NTILE" => SqlType::bigint(true),
        "PERCENT_RANK" | "RATIO_TO_REPORT" => SqlType::decfloat(true),
        "CUME_DIST" => SqlType::decfloat(false),
        "LAG" | "LEAD" | "FIRST_VALUE" | "LAST_VALUE" | "NTH_VALUE" => arg.unwrap_or_else(SqlType::unknown).nullable(true),
        _ => match infer(&o.func, scope) {
            t if f.name == "COUNT" || f.name == "COUNT_BIG" => t,
            t => t.nullable(true),
        },
    }
}

/// What one specification needs of each row.
struct RowData {
    part: String,
    keys: Vec<Value>,
    args: Vec<Value>,
    within: Vec<Value>,
}

/// The values of every specification for every row, by slot.
pub(crate) fn compute(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    envs: &[Env],
    overs: &[&Over],
) -> Result<Vec<HashMap<usize, Value>>> {
    let mut out: Vec<HashMap<usize, Value>> = vec![HashMap::new(); envs.len()];
    for o in overs {
        let vals = one(txn, ev, envs, o)?;
        for (i, v) in vals.into_iter().enumerate() {
            out[i].insert(o.slot, v);
        }
    }
    Ok(out)
}

fn row_key(vals: &[Value]) -> String {
    let mut s = String::new();
    for v in vals {
        s.push_str(&v.key());
        s.push('\u{1}');
    }
    s
}

#[allow(clippy::too_many_lines)]
fn one(txn: &mut Txn<'_>, ev: &mut Evaluator<'_>, envs: &[Env], o: &Over) -> Result<Vec<Value>> {
    let (f, within_order, overflow) = parts(o)?;
    let name = f.name.as_str();
    let aggregate = agg::is_aggregate(f);
    if !aggregate
        && !matches!(
            name,
            "ROW_NUMBER" | "RANK" | "DENSE_RANK" | "NTILE" | "PERCENT_RANK" | "CUME_DIST" | "LAG" | "LEAD" | "FIRST_VALUE"
                | "LAST_VALUE" | "NTH_VALUE" | "RATIO_TO_REPORT"
        )
    {
        return Err(SqlError::Unsupported { what: format!("{name} as an OLAP function") });
    }
    if matches!(name, "ROW_NUMBER" | "RANK" | "DENSE_RANK" | "PERCENT_RANK" | "CUME_DIST" | "NTILE")
        && o.window.frame.is_some()
    {
        return Err(SqlError::Misplaced { what: format!("a window frame with {name}") });
    }
    if matches!(name, "RANK" | "DENSE_RANK" | "PERCENT_RANK" | "CUME_DIST" | "NTILE" | "LAG" | "LEAD")
        && o.window.order.is_empty()
    {
        return Err(SqlError::Misplaced { what: format!("{name} without ORDER BY in its window") });
    }
    // Each row's partition, order keys and arguments.
    let mut data = Vec::with_capacity(envs.len());
    for env in envs {
        ev.ctx.tick(txn)?;
        let mut pv = Vec::with_capacity(o.window.partition.len());
        for p in &o.window.partition {
            pv.push(ev.eval(txn, p, env)?);
        }
        let mut keys = Vec::with_capacity(o.window.order.len());
        for i in &o.window.order {
            keys.push(ev.eval(txn, &i.expr, env)?);
        }
        let mut args = Vec::with_capacity(f.args.len());
        if !f.star {
            for a in &f.args {
                args.push(ev.eval(txn, a, env)?);
            }
        }
        let mut within = Vec::with_capacity(within_order.len());
        for i in within_order {
            within.push(ev.eval(txn, &i.expr, env)?);
        }
        data.push(RowData { part: row_key(&pv), keys, args, within });
    }
    // Partitions in the order they first appear, each sorted.
    let mut parts_of: Vec<Vec<usize>> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, d) in data.iter().enumerate() {
        let p = *index.entry(d.part.clone()).or_insert_with(|| {
            parts_of.push(Vec::new());
            parts_of.len() - 1
        });
        parts_of[p].push(i);
    }
    let specs: Vec<KeySpec> =
        o.window.order.iter().map(|i| KeySpec { desc: i.desc, nulls_first: i.nulls_first.unwrap_or(i.desc) }).collect();
    let mut failure = None;
    for p in &mut parts_of {
        p.sort_by(|a, b| match cmp_keys(&data[*a].keys, &data[*b].keys, &specs) {
            Ok(o) => o,
            Err(e) => {
                failure = Some(e);
                std::cmp::Ordering::Equal
            }
        });
    }
    if let Some(e) = failure {
        return Err(e);
    }
    let offset = |ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, e: &Expr| -> Result<Value> { ev.eval(txn, e, &Env::default()) };
    let bound_value = |b: &FrameBound| match b {
        FrameBound::Preceding(e) | FrameBound::Following(e) => Some(e.clone()),
        _ => None,
    };
    let (start_off, end_off) = match &o.window.frame {
        Some(fr) => {
            let s = match bound_value(&fr.start) {
                Some(e) => Some(offset(ev, txn, &e)?),
                None => None,
            };
            let e = match bound_value(&fr.end) {
                Some(e) => Some(offset(ev, txn, &e)?),
                None => None,
            };
            (s, e)
        }
        None => (None, None),
    };
    let mut out = vec![Value::Null; envs.len()];
    for p in &parts_of {
        let n = p.len();
        // Peers: rows whose order keys are all equal.
        let mut peer_start = vec![0; n];
        let mut peer_end = vec![0; n];
        let mut dense = vec![0i64; n];
        let mut s = 0;
        let mut groups = 0;
        while s < n {
            let mut e = s;
            while e + 1 < n && cmp_keys(&data[p[e + 1]].keys, &data[p[s]].keys, &specs)? == std::cmp::Ordering::Equal {
                e += 1;
            }
            groups += 1;
            for j in s..=e {
                peer_start[j] = s;
                peer_end[j] = e;
                dense[j] = groups;
            }
            s = e + 1;
        }
        let nn = i64::try_from(n).expect("fits");
        for j in 0..n {
            ev.ctx.tick(txn)?;
            let row = p[j];
            let jj = i64::try_from(j).expect("fits");
            let v = match name {
                "ROW_NUMBER" => Value::int(jj + 1),
                "RANK" => Value::int(i64::try_from(peer_start[j]).expect("fits") + 1),
                "DENSE_RANK" => Value::int(dense[j]),
                "PERCENT_RANK" => {
                    if n == 1 {
                        Value::DecFloat(DecFloat::from_i64(0, DfFormat::D34))
                    } else {
                        ratio(i64::try_from(peer_start[j]).expect("fits"), nn - 1)?
                    }
                }
                "CUME_DIST" => ratio(i64::try_from(peer_end[j]).expect("fits") + 1, nn)?,
                "NTILE" => {
                    let a = &data[row].args[0];
                    if a.is_null() {
                        Value::Null
                    } else {
                        let k = a.as_i64()?;
                        if k < 1 {
                            return Err(SqlError::Overflow { what: "NTILE needs a number of groups of 1 or more".into() });
                        }
                        Value::int(ntile(jj, nn, k))
                    }
                }
                "LAG" | "LEAD" => {
                    let a = &data[row].args;
                    let off = a.get(1).map_or(Ok(1), Value::as_i64)?;
                    let dflt = a.get(2).cloned().unwrap_or(Value::Null);
                    let forward = name == "LEAD";
                    shifted(p, &data, j, off, forward, o.ignore_nulls).unwrap_or(dflt)
                }
                "RATIO_TO_REPORT" => {
                    let mut sum = Value::Null;
                    for &r in p {
                        let x = &data[r].args[0];
                        if !x.is_null() {
                            sum = if sum.is_null() { x.clone() } else { binary(&sum, BinOp::Add, x)? };
                        }
                    }
                    let x = &data[row].args[0];
                    if x.is_null() || sum.is_null() {
                        Value::Null
                    } else {
                        let a = x.as_decfloat(DfFormat::D34)?;
                        let b = sum.as_decfloat(DfFormat::D34)?;
                        if b.is_zero() {
                            Value::Null
                        } else {
                            Value::DecFloat(qdecimal::decfloat::div(&a, &b).map_err(decfloat_error)?)
                        }
                    }
                }
                _ => {
                    let (lo, hi) = frame(o, p, &data, j, &peer_start, &peer_end, start_off.as_ref(), end_off.as_ref(), &specs)?;
                    let members: Vec<usize> = if lo <= hi { p[lo..=hi].to_vec() } else { Vec::new() };
                    match name {
                        "FIRST_VALUE" | "LAST_VALUE" | "NTH_VALUE" => {
                            let mut vals: Vec<&Value> = members.iter().map(|&r| &data[r].args[0]).collect();
                            if o.ignore_nulls {
                                vals.retain(|v| !v.is_null());
                            }
                            let pick = match name {
                                "FIRST_VALUE" => vals.first().copied(),
                                "LAST_VALUE" => vals.last().copied(),
                                _ => {
                                    let k = data[row].args.get(1).map_or(Ok(1), Value::as_i64)?;
                                    if k < 1 {
                                        return Err(SqlError::Overflow { what: "NTH_VALUE needs 1 or more".into() });
                                    }
                                    vals.get(usize::try_from(k - 1).unwrap_or(usize::MAX)).copied()
                                }
                            };
                            pick.cloned().unwrap_or(Value::Null)
                        }
                        _ => {
                            let inputs: Vec<agg::Input> = members
                                .iter()
                                .map(|&r| agg::Input { args: data[r].args.clone(), keys: data[r].within.clone() })
                                .collect();
                            let call = agg::Call { name, distinct: f.distinct, star: f.star, order: within_order, overflow };
                            let mut nulls = false;
                            let v = agg::finish(&call, inputs, &mut nulls)?;
                            if nulls {
                                ev.ctx.nulls_eliminated = true;
                            }
                            v
                        }
                    }
                }
            };
            out[row] = v;
        }
    }
    Ok(out)
}

/// `a / b` as a `DECFLOAT(34)`.
fn ratio(a: i64, b: i64) -> Result<Value> {
    let x = DecFloat::from_i64(a, DfFormat::D34);
    let y = DecFloat::from_i64(b, DfFormat::D34);
    Ok(Value::DecFloat(qdecimal::decfloat::div(&x, &y).map_err(decfloat_error)?))
}

/// The group (from 1) of row `j` of `n` in `k` groups: the first
/// `n mod k` groups have one row more.
fn ntile(j: i64, n: i64, k: i64) -> i64 {
    let size = n / k;
    let extra = n % k;
    let big = extra * (size + 1);
    if j < big { j / (size + 1) + 1 } else if size == 0 { j + 1 } else { extra + (j - big) / size + 1 }
}

/// The argument of the row `off` rows before (or after) `j`; with
/// `IGNORE NULLS` the rows with a null argument are not counted.
fn shifted(p: &[usize], data: &[RowData], j: usize, off: i64, forward: bool, ignore: bool) -> Option<Value> {
    if off == 0 {
        return Some(data[p[j]].args[0].clone());
    }
    let (forward, off) = if off < 0 { (!forward, -off) } else { (forward, off) };
    let mut left = off;
    let mut i = j;
    loop {
        if forward {
            i += 1;
            if i >= p.len() {
                return None;
            }
        } else {
            if i == 0 {
                return None;
            }
            i -= 1;
        }
        let v = &data[p[i]].args[0];
        if ignore && v.is_null() {
            continue;
        }
        left -= 1;
        if left == 0 {
            return Some(v.clone());
        }
    }
}

/// The frame of row `j`: the first and last positions in the sorted
/// partition (first above last when empty).
#[allow(clippy::too_many_arguments)]
fn frame(
    o: &Over,
    p: &[usize],
    data: &[RowData],
    j: usize,
    peer_start: &[usize],
    peer_end: &[usize],
    start_off: Option<&Value>,
    end_off: Option<&Value>,
    specs: &[KeySpec],
) -> Result<(usize, usize)> {
    let n = p.len();
    let Some(fr) = &o.window.frame else {
        return Ok(if o.window.order.is_empty() { (0, n - 1) } else { (0, peer_end[j]) });
    };
    let count = |v: Option<&Value>| -> Result<i64> {
        let x = v.map_or(Ok(0), Value::as_i64)?;
        if x < 0 {
            return Err(SqlError::Overflow { what: "a window frame offset below 0".into() });
        }
        Ok(x)
    };
    let ni = i64::try_from(n).expect("fits");
    let ji = i64::try_from(j).expect("fits");
    let clamp = |x: i64| usize::try_from(x.clamp(0, ni - 1)).expect("in range");
    if fr.rows {
        let lo = match &fr.start {
            FrameBound::UnboundedPreceding => 0,
            FrameBound::Preceding(_) => ji - count(start_off)?,
            FrameBound::CurrentRow => ji,
            FrameBound::Following(_) => ji + count(start_off)?,
            FrameBound::UnboundedFollowing => ni,
        };
        let hi = match &fr.end {
            FrameBound::UnboundedPreceding => NO_ROW,
            FrameBound::Preceding(_) => ji - count(end_off)?,
            FrameBound::CurrentRow => ji,
            FrameBound::Following(_) => ji + count(end_off)?,
            FrameBound::UnboundedFollowing => ni - 1,
        };
        if lo > hi || hi < 0 || lo >= ni {
            return Ok((1, 0));
        }
        return Ok((clamp(lo), clamp(hi)));
    }
    // RANGE: by the value of the one order key, in the sort's direction.
    let key = |i: usize| data[p[i]].keys.first().cloned().unwrap_or(Value::Null);
    let desc = specs.first().is_some_and(|s| s.desc);
    let me = key(j);
    // The key `off` before (`back`) or after the current one.
    let target = |off: Option<&Value>, back: bool| -> Result<Value> {
        let off = off.cloned().unwrap_or(Value::int(0));
        binary(&me, if back == desc { BinOp::Add } else { BinOp::Sub }, &off)
    };
    // Whether key i is at or past `t` in the sort's direction.
    let at_or_past = |i: usize, t: &Value| -> Result<bool> {
        let k = key(i);
        if k.is_null() {
            return Ok(false);
        }
        Ok(compare(&k, if desc { CmpOp::Le } else { CmpOp::Ge }, t)? == Some(true))
    };
    let at_or_before = |i: usize, t: &Value| -> Result<bool> {
        let k = key(i);
        if k.is_null() {
            return Ok(false);
        }
        Ok(compare(&k, if desc { CmpOp::Ge } else { CmpOp::Le }, t)? == Some(true))
    };
    // A null key's frame is its peers.
    if me.is_null() && (start_off.is_some() || end_off.is_some()) {
        return Ok((peer_start[j], peer_end[j]));
    }
    let first_from = |t: &Value| -> Result<usize> {
        for i in 0..n {
            if at_or_past(i, t)? {
                return Ok(i);
            }
        }
        Ok(n)
    };
    let last_to = |t: &Value| -> Result<i64> {
        let mut last = -1;
        for i in 0..n {
            if at_or_before(i, t)? {
                last = i64::try_from(i).expect("fits");
            }
        }
        Ok(last)
    };
    let lo = match &fr.start {
        FrameBound::UnboundedPreceding => 0,
        FrameBound::CurrentRow => peer_start[j],
        FrameBound::Preceding(_) => first_from(&target(start_off, true)?)?,
        FrameBound::Following(_) => first_from(&target(start_off, false)?)?,
        FrameBound::UnboundedFollowing => n,
    };
    let hi: i64 = match &fr.end {
        FrameBound::UnboundedFollowing => ni - 1,
        FrameBound::CurrentRow => i64::try_from(peer_end[j]).expect("fits"),
        FrameBound::Following(_) => last_to(&target(end_off, false)?)?,
        FrameBound::Preceding(_) => last_to(&target(end_off, true)?)?,
        FrameBound::UnboundedPreceding => NO_ROW,
    };
    let _ = (count(None)?, clamp(0));
    if hi < 0 || lo >= n || i64::try_from(lo).expect("fits") > hi {
        return Ok((1, 0));
    }
    Ok((lo, usize::try_from(hi).expect("positive")))
}

/// A frame end before the first row.
const NO_ROW: i64 = -1;

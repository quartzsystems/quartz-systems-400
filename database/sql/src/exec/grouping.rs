// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GROUP BY` with `ROLLUP`, `CUBE` and `GROUPING SETS`: the grouping
//! sets an element list stands for, and the select list of each set,
//! where a grouping expression the set leaves out is null and
//! `GROUPING(e)` is 1 for it, 0 otherwise.

#![allow(clippy::many_single_char_names)]

use crate::ast::{Expr, Function, Literal};
use crate::error::{Result, SqlError};
use crate::parse::GROUPING_SETS;

/// One grouping set.
#[derive(Clone, Debug)]
pub struct GroupingSet {
    /// The expressions the rows are grouped by.
    pub keys: Vec<Expr>,
    /// The grouping expressions of the other sets: null in this one.
    pub absent: Vec<Expr>,
}

/// The alternatives one element gives, each a list of expressions.
fn alternatives(e: &Expr, inside_sets: bool) -> Result<Vec<Vec<Expr>>> {
    Ok(match e {
        Expr::Function(Function { name, args, .. }) if name == "ROLLUP" => {
            let parts: Vec<Vec<Expr>> = args.iter().map(flat).collect();
            let mut out = Vec::new();
            for k in (0..=parts.len()).rev() {
                out.push(parts[..k].concat());
            }
            out
        }
        Expr::Function(Function { name, args, .. }) if name == "CUBE" => {
            let parts: Vec<Vec<Expr>> = args.iter().map(flat).collect();
            if parts.len() > 12 {
                return Err(SqlError::TooComplex);
            }
            let n = parts.len();
            let mut out = Vec::new();
            // Every subset, the full one first, in the order of a count
            // down of the bit pattern.
            for mask in (0..(1u32 << n)).rev() {
                let mut v = Vec::new();
                for (i, p) in parts.iter().enumerate() {
                    if mask & (1 << (n - 1 - i)) != 0 {
                        v.extend(p.iter().cloned());
                    }
                }
                out.push(v);
            }
            out
        }
        Expr::Function(Function { name, args, .. }) if name == GROUPING_SETS => {
            if inside_sets {
                return Err(SqlError::Unsupported {
                    what: "GROUPING SETS inside GROUPING SETS".into(),
                });
            }
            let mut out = Vec::new();
            for a in args {
                out.extend(alternatives(a, true)?);
            }
            out
        }
        Expr::Row(items) => vec![items.clone()],
        e => vec![vec![e.clone()]],
    })
}

fn flat(e: &Expr) -> Vec<Expr> {
    match e {
        Expr::Row(items) => items.clone(),
        e => vec![e.clone()],
    }
}

/// Whether `GROUP BY` uses `ROLLUP`, `CUBE` or `GROUPING SETS`.
#[must_use]
pub fn has_sets(group_by: &[Expr]) -> bool {
    group_by.iter().any(|e| matches!(e, Expr::Function(f) if matches!(f.name.as_str(), "ROLLUP" | "CUBE") || f.name == GROUPING_SETS))
}

/// The grouping sets of a `GROUP BY`: the product of its elements'
/// alternatives.
pub fn sets(group_by: &[Expr]) -> Result<Vec<GroupingSet>> {
    let mut combos: Vec<Vec<Expr>> = vec![Vec::new()];
    for e in group_by {
        let alts = alternatives(e, false)?;
        let mut next = Vec::with_capacity(combos.len() * alts.len());
        for c in &combos {
            for a in &alts {
                let mut v = c.clone();
                for x in a {
                    if !v.contains(x) {
                        v.push(x.clone());
                    }
                }
                next.push(v);
            }
        }
        if next.len() > 4096 {
            return Err(SqlError::TooComplex);
        }
        combos = next;
    }
    let mut all: Vec<Expr> = Vec::new();
    for c in &combos {
        for x in c {
            if !all.contains(x) {
                all.push(x.clone());
            }
        }
    }
    Ok(combos
        .into_iter()
        .map(|keys| {
            let absent = all.iter().filter(|x| !keys.contains(x)).cloned().collect();
            GroupingSet { keys, absent }
        })
        .collect())
}

/// `e` for one grouping set: absent grouping expressions null,
/// `GROUPING(x)` its 0 or 1.
#[must_use]
pub fn rewrite(e: &Expr, set: &GroupingSet) -> Expr {
    if set.absent.contains(e) {
        return Expr::Literal(Literal::Null);
    }
    if let Expr::Function(f) = e
        && f.name == "GROUPING"
        && f.args.len() == 1
    {
        return Expr::num(if set.absent.contains(&f.args[0]) {
            "1"
        } else {
            "0"
        });
    }
    // Inside an aggregate the columns are the rows' own.
    if super::agg::call_of(e).is_some() {
        return e.clone();
    }
    let mut out = e.clone();
    map_children(&mut out, &|c| rewrite(c, set));
    out
}

/// Replace each expression directly under `e` (subqueries left alone).
fn map_children(e: &mut Expr, f: &dyn Fn(&Expr) -> Expr) {
    let m = |x: &mut Box<Expr>| **x = f(x);
    match e {
        Expr::Neg(a)
        | Expr::Not(a)
        | Expr::IsNull(a, _)
        | Expr::Cast(a, _)
        | Expr::Duration(a, _)
        | Expr::InSubquery(a, _, _) => m(a),
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => {
            m(a);
            m(b);
        }
        Expr::Between(a, b, c, _) => {
            m(a);
            m(b);
            m(c);
        }
        Expr::InList(a, l, _) => {
            m(a);
            for x in l {
                *x = f(x);
            }
        }
        Expr::Like(a, p, esc, _) => {
            m(a);
            m(p);
            if let Some(x) = esc {
                m(x);
            }
        }
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            if let Some(o) = operand {
                m(o);
            }
            for (w, t) in whens {
                *w = f(w);
                *t = f(t);
            }
            if let Some(o) = otherwise {
                m(o);
            }
        }
        Expr::Function(func) => {
            for x in &mut func.args {
                *x = f(x);
            }
        }
        Expr::Row(items) => {
            for x in items {
                *x = f(x);
            }
        }
        Expr::Over(o) => {
            o.func = f(&o.func);
            for x in &mut o.window.partition {
                *x = f(x);
            }
            for i in &mut o.window.order {
                i.expr = f(&i.expr);
            }
        }
        Expr::WithinGroup(w) => {
            for x in &mut w.func.args {
                *x = f(x);
            }
        }
        _ => {}
    }
}

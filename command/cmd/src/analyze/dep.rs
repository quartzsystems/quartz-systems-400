// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DEP` statements: when the controlling condition holds, as many of
//! the parameter conditions as `NBRTRUE` says must hold too.

use qdecimal::Decimal;

use super::Bound;
use crate::def::{CmdDef, Dep, DepCtl, DepItem, NbrTrue, RelOp};
use crate::error::Diag;
use crate::value::Value;

/// Check every dependency; a failure is the statement's `MSGID` or
/// `CPD0061`.
pub fn check(def: &CmdDef, parms: &[Bound], diags: &mut Vec<Diag>) {
    for d in &def.deps {
        if !ctl_holds(d, parms) {
            continue;
        }
        let truths: Vec<bool> = d.parms.iter().map(|i| item_holds(i, parms)).collect();
        let n = truths.iter().filter(|t| **t).count();
        let ok = match d.nbrtrue {
            NbrTrue::All => n == truths.len(),
            NbrTrue::None => n == 0,
            NbrTrue::Count { op, n: want } => op.holds(n.cmp(&usize::from(want))),
        };
        if !ok {
            let names: Vec<&str> = d.parms.iter().map(item_kwd).collect();
            let first = names.first().copied().unwrap_or("");
            let id = d.msgid.as_deref().unwrap_or("CPD0061");
            diags.push(
                Diag::new(id)
                    .data(&[(first, 10), (&def.name, 10)])
                    .parm(first)
                    .detail(format!(
                        "the values of {} do not go together",
                        names.join(", ")
                    )),
            );
        }
    }
}

fn item_kwd(i: &DepItem) -> &str {
    match i {
        DepItem::Specified { kwd } | DepItem::Cond { kwd, .. } => kwd,
    }
}

fn ctl_holds(d: &Dep, parms: &[Bound]) -> bool {
    match &d.ctl {
        DepCtl::Always => true,
        DepCtl::Item { item } => item_holds(item, parms),
    }
}

fn item_holds(i: &DepItem, parms: &[Bound]) -> bool {
    match i {
        DepItem::Specified { kwd } => parms.iter().any(|b| b.kwd == *kwd && b.specified),
        DepItem::Cond { kwd, op, value } => parms
            .iter()
            .find(|b| b.kwd == *kwd)
            .and_then(|b| b.value.as_ref())
            .is_some_and(|v| compare(v, *op, value)),
    }
}

/// Compare a bound value with a condition's source text.
pub(crate) fn compare(v: &Value, op: RelOp, text: &str) -> bool {
    let scalar = first_scalar(v);
    match scalar {
        Some(Value::Num(d)) => match Decimal::parse(text) {
            Ok(t) => op.holds(d.cmp(&t)),
            Err(_) => false,
        },
        Some(Value::Char(s)) => {
            let t = text.trim_matches('\'');
            let t = if text.starts_with('\'') {
                t.to_string()
            } else {
                t.to_ascii_uppercase()
            };
            op.holds(ccsid::ebcdic_cmp_str(s.trim_end(), t.trim_end()))
        }
        Some(Value::Null) => text == "*NULL",
        _ => false,
    }
}

/// The scalar a value compares by: itself, or the first part of a
/// group, qualified name or list.
fn first_scalar(v: &Value) -> Option<&Value> {
    match v {
        Value::Qual(parts) => parts.first().and_then(first_scalar),
        Value::Elems(items) => items
            .first()
            .and_then(|i| i.as_ref())
            .and_then(first_scalar),
        Value::List(items) => items.first().and_then(first_scalar),
        other => Some(other),
    }
}

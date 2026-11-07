// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Array variables of SQL procedures. A procedure holds an array as a
//! 4-byte handle the database maps to the value (held locators, kept
//! past the unit of work), and its statement descriptors say which
//! inputs and outputs are arrays (`Desc::locs`):
//!
//! - `Ak`: input `k` is an array's handle; its value stands for it.
//! - `Bk~h~max~type`: output `k` goes to an array of `type` elements, at
//!   most `max`; input `h` holds the variable's old handle, freed once
//!   the new value has one.
//! - `Ek~j~h~max~type`: output `k` is an element assigned at the index in
//!   input `j` of the array whose handle is input `h`.
//!
//! The inputs `h` and `j` follow the statement's markers and are not
//! among them. A type's text has `/` for its commas.

use super::{Reply, Request, Session, Sqlca};
use crate::error::SqlError;
use crate::exec::array;
use crate::value::Value;

/// One array output of a statement.
struct Out {
    k: usize,
    index: Option<usize>,
    old: usize,
    max: u64,
    ty: crate::ast::TypeSpec,
}

/// Whether a descriptor's `locs` name arrays.
pub(super) fn has_arrays(locs: &str) -> bool {
    locs.split(',').any(|p| p.starts_with(['A', 'B', 'E']))
}

fn bad() -> SqlError {
    SqlError::Unsupported {
        what: "a statement descriptor with a bad array entry".into(),
    }
}

fn outs(locs: &str) -> Result<Vec<Out>, SqlError> {
    let mut v = Vec::new();
    for p in locs.split(',') {
        let elem = p.starts_with('E');
        if !elem && !p.starts_with('B') {
            continue;
        }
        let f: Vec<&str> = p[1..].split('~').collect();
        let num = |s: &str| s.parse::<usize>().map_err(|_| bad());
        let (k, index, rest) = if elem {
            let [k, j, rest @ ..] = f.as_slice() else {
                return Err(bad());
            };
            (num(k)?, Some(num(j)?), rest)
        } else {
            let [k, rest @ ..] = f.as_slice() else {
                return Err(bad());
            };
            (num(k)?, None, rest)
        };
        let [old, max, ty] = rest else {
            return Err(bad());
        };
        v.push(Out {
            k,
            index,
            old: num(old)?,
            max: max.parse().map_err(|_| bad())?,
            ty: crate::pl::types::parse_type(&ty.replace('/', ","))?,
        });
    }
    Ok(v)
}

fn handle(v: &Value) -> u32 {
    v.as_i64()
        .ok()
        .and_then(|h| u32::try_from(h).ok())
        .unwrap_or(0)
}

impl Session {
    /// A statement with array inputs or outputs.
    pub(super) fn with_arrays(&mut self, req: &Request) -> Reply {
        match self.arrays_inner(req) {
            Ok(r) => r,
            Err(e) => Reply::of(Sqlca::from_error(&e)),
        }
    }

    fn arrays_inner(&mut self, req: &Request) -> Result<Reply, SqlError> {
        let d = &req.desc;
        let outs = outs(&d.locs)?;
        let mut inputs = req.inputs.clone();
        for p in d.locs.split(',') {
            if let Some(k) = p.strip_prefix('A').and_then(|k| k.parse::<usize>().ok())
                && let Some(v) = inputs.get_mut(k)
                && !v.is_null()
            {
                *v = self.ctx.locators.get(handle(v))?.clone();
            }
        }
        // The hidden inputs: the old handles and the indexes.
        let hidden: Vec<usize> = outs
            .iter()
            .flat_map(|o| std::iter::once(o.old).chain(o.index))
            .collect();
        let marked = hidden
            .iter()
            .min()
            .copied()
            .unwrap_or(inputs.len())
            .min(inputs.len());
        let extra = inputs.split_off(marked);
        let hidden_at = |i: usize| extra.get(i - marked).cloned().unwrap_or(Value::Null);
        let sub = Request {
            scope: req.scope.clone(),
            desc: d.clone(),
            inputs,
        };
        let mut r = self.run_inner(&sub);
        if r.sqlca.is_error() {
            return Ok(r);
        }
        let mut freed = Vec::new();
        for row in &mut r.rows {
            for o in &outs {
                let Some(cell) = row.get_mut(o.k) else {
                    continue;
                };
                let old = handle(&hidden_at(o.old));
                let value = match o.index {
                    None => std::mem::replace(cell, Value::Null),
                    Some(j) => {
                        let at = array::index(&hidden_at(j), o.max)?;
                        let mut items = match self.ctx.locators.get(old) {
                            Ok(Value::Array(a)) => a.clone(),
                            _ => Vec::new(),
                        };
                        if items.len() <= at {
                            items.resize(at + 1, Value::Null);
                        }
                        items[at] = std::mem::replace(cell, Value::Null);
                        Value::Array(items)
                    }
                };
                if !value.is_null() && !matches!(value, Value::Array(_)) {
                    return Err(SqlError::Coded {
                        code: -408,
                        state: "42821",
                        what: format!("a {} value for an array variable", value.type_name()),
                    });
                }
                let value = array::to_type(&value, &o.ty, o.max)?;
                if value.is_null() {
                    *cell = Value::Null;
                } else {
                    let h = self.ctx.locators.assign(value);
                    self.ctx.locators.hold(h)?;
                    *cell = Value::int(i64::from(h));
                }
                if old != 0 {
                    freed.push(old);
                }
            }
        }
        for h in freed {
            let _ = self.ctx.locators.free(h);
        }
        Ok(r)
    }
}

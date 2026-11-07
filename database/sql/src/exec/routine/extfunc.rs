// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! External scalar functions (`CREATE FUNCTION ... EXTERNAL NAME`,
//! `PARAMETER STYLE SQL`). The function is a program or service program
//! procedure, and programs run in the job: each call is a plan of style
//! [`CallStyle::Function`] made in the middle of the statement
//! ([`super::midcall`]), the statement suspended until the job answers
//! with the result. `SCRATCHPAD` keeps its storage from call to call of
//! the statement, and `FINAL CALL` gives the call type (first, normal)
//! and a final call when the statement ends.

use std::collections::HashMap;

use db::sqlinf::routine::flag;
use store::txn::Txn;

use super::find::Found;
use super::{CallPlan, CallStyle, PlanParm};
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;
use crate::exec::eval::cast;
use crate::pl::types::{host_type, parse_type};
use crate::value::Value;

/// IBM's call types for `FINAL CALL`.
const FIRST: i64 = -1;
const NORMAL: i64 = 0;
const FINAL: i64 = 1;

/// The functions a statement has called that keep state: their plan
/// (for the final call) and scratchpad, by specific name.
#[derive(Debug, Default)]
pub struct State {
    calls: HashMap<String, (CallPlan, Option<Vec<u8>>)>,
}

fn unsupported(what: &str) -> SqlError {
    SqlError::Unsupported { what: what.into() }
}

/// Call external function `f` on `args`.
///
/// # Errors
///
/// The call cannot be made here, or the function failed (`SQL0443` with
/// the SQLSTATE it set).
pub fn call(txn: &mut Txn<'_>, ctx: &mut SqlContext, f: &Found, args: &[Value]) -> Result<Value> {
    let d = &f.routine.def;
    if matches!(d.style, 2 | 3) {
        // BLOCKED(rel): PARAMETER STYLE GENERAL gives the result as the
        // procedure's return value, which the runtime's by-reference call
        // does not read.
        return Err(unsupported("an external function of PARAMETER STYLE GENERAL"));
    }
    if d.has(flag::DBINFO) {
        return Err(unsupported("an external function with DBINFO"));
    }
    if d.has(flag::NULL_INPUT) && args.iter().any(Value::is_null) {
        return Ok(Value::Null);
    }
    let mut parms = Vec::new();
    for (p, a) in f.routine.parms.iter().filter(|p| p.role == 0).zip(args) {
        let ty = parse_type(&p.ty)?;
        let h = host_type(&ty)?;
        parms.push(PlanParm { mode: 1, desc: h.desc, extra: h.extra, value: cast(a, &ty)?, marker: None });
    }
    let Some(ret) = f.routine.parms.iter().find(|p| p.role == 2) else {
        // BLOCKED(rel): an external table function is called once per row
        // to fetch with open and close calls around.
        return Err(unsupported("an external table function"));
    };
    let ret_ty = parse_type(&ret.ty)?;
    let h = host_type(&ret_ty)?;
    parms.push(PlanParm { mode: 2, desc: h.desc, extra: h.extra, value: Value::Null, marker: None });
    let key = d.specific.clone();
    let kept = ctx.ext_functions.calls.get(&key).cloned();
    let first = kept.is_none();
    let pad = match &kept {
        Some((_, pad)) => pad.clone(),
        None if d.scratchpad > 0 => Some(vec![0; usize::try_from(d.scratchpad).unwrap_or(0)]),
        None => None,
    };
    let mut plan = CallPlan {
        program: d.program.clone(),
        library: d.program_library.clone(),
        entry: d.entry.clone(),
        style: CallStyle::Function,
        routine: format!("{}.{}", f.schema, d.name),
        specific: d.specific.clone(),
        parms,
        ..CallPlan::default()
    };
    // After the arguments and the result: the scratchpad, the call type.
    if let Some(p) = &pad {
        plan.parms.push(PlanParm { mode: 3, desc: 0, extra: 0, value: Value::Bin(p.clone()), marker: None });
    }
    if d.has(flag::FINAL_CALL) {
        plan.parms.push(PlanParm { mode: 1, desc: 0, extra: 1, value: Value::int(if first { FIRST } else { NORMAL }), marker: None });
    }
    let (value, pad) = make(txn, ctx, &plan, &ret_ty)?;
    if pad.is_some() || d.has(flag::FINAL_CALL) {
        ctx.ext_functions.calls.insert(key, (plan, pad));
    }
    Ok(value)
}

/// The statement ended: the final call of each `FINAL CALL` function it
/// called, its failures ignored as the statement's outcome stands.
pub fn finish(txn: &mut Txn<'_>, ctx: &mut SqlContext) {
    let calls = std::mem::take(&mut ctx.ext_functions.calls);
    for (_, (mut plan, pad)) in calls {
        let Some(last) = plan.parms.last_mut() else { continue };
        if last.extra != 1 {
            continue;
        }
        last.value = Value::int(FINAL);
        if let (Some(p), Some(slot)) = (pad, plan.parms.iter_mut().find(|p| matches!(p.value, Value::Bin(_)) && p.mode == 3)) {
            slot.value = Value::Bin(p);
        }
        let ty = crate::ast::TypeSpec { data_type: crate::ast::DataType::Integer, ccsid: None, bit_data: false, udt: None };
        let _ = make(txn, ctx, &plan, &ty);
    }
}

/// One call: the statement suspended, the job's answer read.
fn make(txn: &mut Txn<'_>, ctx: &mut SqlContext, plan: &CallPlan, ret: &crate::ast::TypeSpec) -> Result<(Value, Option<Vec<u8>>)> {
    let Some(mid) = ctx.midcall.clone() else {
        // BLOCKED(rel): a statement run where no program can be called
        // (the service console, a statement not on a fiber).
        return Err(unsupported(&format!("calling external function {} from this interface: it cannot run the program", plan.routine)));
    };
    if ctx.holds_open() {
        // BLOCKED(rel): a scan holding the store open cannot give it back.
        return Err(unsupported(&format!("external function {} where the statement reads a table's rows in place", plan.routine)));
    }
    txn.commit_part()?;
    if !txn.suspend() {
        return Err(unsupported(&format!("external function {} here: the statement cannot give the store back", plan.routine)));
    }
    let answer = mid(plan.clone());
    txn.resume()?;
    let Some(v) = answer else { return Err(SqlError::Cancelled) };
    let state = v.get(1).map(Value::as_text).unwrap_or_default();
    let message = v.get(2).map(Value::as_text).unwrap_or_default();
    if !state.starts_with("00") && !state.starts_with("01") {
        let code = v.get(4).and_then(|c| c.as_i64().ok()).and_then(|c| i32::try_from(c).ok()).filter(|c| *c < 0).unwrap_or(-443);
        let what = if message.trim().is_empty() { format!("function {} failed with SQLSTATE {state}", plan.routine) } else { message.trim().to_string() };
        return Err(SqlError::Signalled { code, state, what });
    }
    let value = match v.first() {
        Some(x) if !x.is_null() => cast(x, ret)?,
        _ => Value::Null,
    };
    let pad = match v.get(3) {
        Some(Value::Bin(b)) => Some(b.clone()),
        _ => None,
    };
    Ok((value, pad))
}

/// The answer to a [`CallStyle::Function`] plan, from the job: the
/// result (`Null` for a null one), the SQLSTATE, the message text, the
/// scratchpad as the function left it (`Null` for none), the SQLCODE.
#[must_use]
pub fn answer(result: Value, sqlstate: &str, message: &str, pad: Option<Vec<u8>>, sqlcode: i32) -> Vec<Value> {
    vec![result, Value::Str(sqlstate.into()), Value::Str(message.into()), pad.map_or(Value::Null, Value::Bin), Value::int(sqlcode.into())]
}

/// Whether a plan is one [`super::midcall`] makes (a trigger program or an
/// external function), answered by the job at once.
#[must_use]
pub fn is_mid(p: &CallPlan) -> bool {
    matches!(p.style, CallStyle::Trigger | CallStyle::Function)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CALL` from embedded SQL and SQL routines: the database resolves the
//! procedure and plans the call ([`sql::exec::routine::CallPlan`]); the
//! job makes it here, with the parameters in their types' storage and
//! the parameter style's extra arguments, then reads back the outputs
//! and tells the database how it went ([`sql::embed::Op::Called`]).

use std::cell::Cell;

use sql::exec::routine::{CallPlan, CallStyle, Called};
use sql::{Sqlca, Value};

use crate::value::{Desc, ebcdic};

thread_local! {
    /// Calls in progress: a call made while one is is nested.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// What the call came to.
pub struct Outcome {
    /// For [`Op::Called`](sql::embed::Op::Called).
    pub called: Called,
    /// The values for the statement's `OUT` and `INOUT` markers: the
    /// marker and its value.
    pub outs: Vec<(usize, Value)>,
    /// For a call in the middle of a statement (a trigger program, plan
    /// style `Trigger`): the answer that resumes it ([`answer`]).
    pub answer: Option<Vec<Value>>,
}

/// Make a call a statement asked for in its middle and answer it: the
/// statement goes on (`midcall::ANSWER`). `None` for a `CALL`'s plan.
pub fn answer(job: &mut mi::Job, plan: &CallPlan) -> Option<Outcome> {
    let answer = match plan.style {
        CallStyle::Trigger => mi::io::trigger::answer_plan(job, plan),
        CallStyle::Function => function_answer(job, plan),
        _ => return None,
    };
    Some(Outcome { called: Called::default(), outs: Vec::new(), answer: Some(answer) })
}

/// Make an external function's call (`PARAMETER STYLE SQL`): its
/// arguments, the result, their indicators, the SQLSTATE, the names, the
/// message, then the scratchpad and the call type when it has them; the
/// answer for the database (`extfunc::answer`).
pub fn function_answer(job: &mut mi::Job, plan: &CallPlan) -> Vec<Value> {
    use sql::exec::routine::extfunc;
    let fail = |code: i32, state: &str, text: String| extfunc::answer(Value::Null, state, &text, None, code);
    let mut bufs: Vec<Vec<u8>> = Vec::new();
    let mut inds: Vec<Vec<u8>> = Vec::new();
    let mut result = None;
    let mut pad: Option<Vec<u8>> = None;
    let mut call_type: Option<Vec<u8>> = None;
    for p in &plan.parms {
        match p.mode {
            3 => {
                let b = match &p.value {
                    Value::Bin(b) => b.clone(),
                    _ => Vec::new(),
                };
                let mut s = u32::try_from(b.len()).unwrap_or(0).to_be_bytes().to_vec();
                s.extend_from_slice(&b);
                pad = Some(s);
            }
            1 if p.extra == 1 && p.desc == 0 => {
                call_type = Some(i32::try_from(p.value.as_i64().unwrap_or(0)).unwrap_or(0).to_be_bytes().to_vec());
            }
            _ => {
                let d = Desc::of(p.desc);
                let (b, ind) = if p.value.is_null() {
                    (crate::sql::empty_image(&d, p.extra), -1i16)
                } else {
                    match crate::sql::value_image(&p.value, &d, p.extra) {
                        Ok(b) => (b, 0),
                        Err((code, state)) => return fail(code, state, "an argument does not fit its parameter".into()),
                    }
                };
                if p.mode == 2 {
                    result = Some((bufs.len(), d, p.extra));
                }
                bufs.push(b);
                inds.push(ind.to_be_bytes().to_vec());
            }
        }
    }
    let Some((at, rd, rextra)) = result else { return fail(-84, "42612", "a function call with no result".into()) };
    let mut state = ebcdic("00000");
    let mut fname = varchar(&plan.routine, 517);
    let mut sname = varchar(&plan.specific, 128);
    let mut diag = varchar("", 70);
    let mut args: Vec<u64> = bufs.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    args.extend(inds.iter_mut().map(|b| b.as_mut_ptr() as u64));
    args.push(state.as_mut_ptr() as u64);
    args.push(fname.as_mut_ptr() as u64);
    args.push(sname.as_mut_ptr() as u64);
    args.push(diag.as_mut_ptr() as u64);
    if let Some(s) = pad.as_mut() {
        args.push(s.as_mut_ptr() as u64);
    }
    if let Some(c) = call_type.as_mut() {
        args.push(c.as_mut_ptr() as u64);
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    let r = if plan.entry.is_empty() {
        job.call_program(&plan.program, &plan.library, &args).map_err(|c| (-444, format!("program {}/{} could not be called: {}", plan.library, plan.program, c.msgid())))
    } else {
        call_procedure(job, plan, &args).map_err(|(c, m)| (if c == -204 { -444 } else { c }, m))
    };
    DEPTH.with(|d| d.set(d.get() - 1));
    if let Err((code, text)) = r {
        return fail(code, if code == -444 { "42724" } else { "42612" }, text);
    }
    if let Some(e) = job.clear() {
        return fail(-443, "38501", format!("function {} ended with {}", plan.routine, e.msgid));
    }
    let st = ccsid::from_ebcdic(&state);
    let pad = pad.map(|s| s[4..].to_vec());
    if !st.starts_with("00") && !st.starts_with("01") {
        return extfunc::answer(Value::Null, &st, &read_varchar(&diag), pad, -443);
    }
    let null = i16::from_be_bytes([inds[at][0], inds[at][1]]) < 0;
    let v = if null { Value::Null } else { crate::sql::value_of(&bufs[at], &rd, rextra) };
    extfunc::answer(v, &st, &read_varchar(&diag), pad, 0)
}

fn varchar(s: &str, cap: usize) -> Vec<u8> {
    let mut e = ebcdic(s);
    e.truncate(cap);
    let mut out = u16::try_from(e.len()).unwrap_or(0).to_be_bytes().to_vec();
    out.extend_from_slice(&e);
    out.resize(2 + cap, 0x40);
    out
}

fn read_varchar(b: &[u8]) -> String {
    let n = usize::from(u16::from_be_bytes([b[0], b[1]])).min(b.len() - 2);
    ccsid::from_ebcdic(&b[2..2 + n]).trim_end().to_string()
}

/// A service program's procedure (`EXTERNAL NAME lib/srvpgm(proc)`),
/// its parameters by reference.
fn call_procedure(job: &mut mi::Job, plan: &CallPlan, args: &[u64]) -> Result<(), (i32, String)> {
    let table = job.table.clone();
    let what = format!("{}/{}({})", plan.library, plan.program, plan.entry);
    let p = job
        .act
        .load_program(&plan.program, &plan.library, ile::PgmKind::SrvPgm, &table)
        .map_err(|c| (-204, format!("service program procedure {what} could not be called: {}", c.msgid())))?;
    let addr = p
        .export_addr(&plan.entry)
        .or_else(|| p.export_addr(&plan.entry.to_ascii_uppercase()))
        .ok_or_else(|| (-204, format!("service program {}/{} exports no procedure {}", plan.library, plan.program, plan.entry)))?;
    if !mi::act::call_by_ref_many(job, addr, args) {
        return Err((-84, format!("CALL of {what} with more than {} parameters in all: not supported", mi::act::MAX_BY_REF)));
    }
    Ok(())
}

/// Make the call the plan describes.
#[allow(clippy::too_many_lines)] // the styles in order
pub fn perform(job: &mut mi::Job, plan: &CallPlan) -> Outcome {
    if let Some(a) = answer(job, plan) {
        return a;
    }
    let n = plan.parms.len();
    let mut called = Called {
        seq: plan.seq,
        max: plan.max_results,
        nested: DEPTH.with(Cell::get) > 0,
        sqlstate: "00000".into(),
        routine: plan.routine.clone(),
        ..Called::default()
    };
    let mut bufs: Vec<Vec<u8>> = Vec::with_capacity(n);
    let mut inds: Vec<i16> = Vec::with_capacity(n);
    for p in &plan.parms {
        let d = Desc::of(p.desc);
        let (b, ind) = if p.value.is_null() {
            (crate::sql::empty_image(&d, p.extra), -1)
        } else {
            match crate::sql::value_image(&p.value, &d, p.extra) {
                Ok(b) => (b, 0),
                Err((code, state)) => {
                    called.sqlcode = code;
                    called.sqlstate = state.into();
                    called.message = "an argument does not fit its parameter".into();
                    return Outcome { called, outs: Vec::new(), answer: None };
                }
            }
        };
        bufs.push(b);
        inds.push(ind);
    }
    let mut ind_bufs: Vec<Vec<u8>> = inds.iter().map(|i| i.to_be_bytes().to_vec()).collect();
    let mut ind_array: Vec<u8> = inds.iter().flat_map(|i| i.to_be_bytes()).collect();
    let mut state = ebcdic("00000");
    let mut rname = varchar(&plan.routine, 517);
    let mut sname = varchar(&plan.specific, 128);
    let mut diag = varchar("", 70);
    let mut ca = vec![0u8; 136];
    crate::sql::write_sqlca_at(&mut ca, &Sqlca::default());
    let mut args: Vec<u64> = bufs.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    match plan.style {
        CallStyle::General | CallStyle::Trigger | CallStyle::Function => {}
        CallStyle::GeneralNulls => args.push(ind_array.as_mut_ptr() as u64),
        CallStyle::Sql | CallStyle::SqlRoutine => {
            args.extend(ind_bufs.iter_mut().map(|b| b.as_mut_ptr() as u64));
            args.push(state.as_mut_ptr() as u64);
            args.push(rname.as_mut_ptr() as u64);
            args.push(sname.as_mut_ptr() as u64);
            args.push(diag.as_mut_ptr() as u64);
            if plan.style == CallStyle::SqlRoutine {
                args.push(ca.as_mut_ptr() as u64);
            }
        }
    }
    DEPTH.with(|d| d.set(d.get() + 1));
    let r = if plan.entry.is_empty() {
        job.call_program(&plan.program, &plan.library, &args).map_err(|c| (-204, format!("program {}/{} could not be called: {}", plan.library, plan.program, c.msgid())))
    } else {
        call_procedure(job, plan, &args)
    };
    DEPTH.with(|d| d.set(d.get() - 1));
    if let Err((code, message)) = r {
        called.sqlcode = code;
        called.sqlstate = if code == -204 { "42704" } else { "42612" }.into();
        called.message = message;
        return Outcome { called, outs: Vec::new(), answer: None };
    }
    if let Some(e) = job.clear() {
        // The program ended with an exception it did not handle.
        called.sqlcode = -443;
        called.sqlstate = "38501".into();
        called.message = format!("procedure {} ended with {}", plan.routine, e.msgid);
        return Outcome { called, outs: Vec::new(), answer: None };
    }
    match plan.style {
        CallStyle::SqlRoutine => {
            let ca = crate::sql::read_sqlca_at(&ca);
            called.sqlcode = ca.sqlcode;
            called.sqlstate = ca.sqlstate;
            called.message = ca.sqlerrmc;
            called.status = ca.sqlerrd[0];
        }
        CallStyle::Sql => {
            let st = ccsid::from_ebcdic(&state);
            if st != "00000" {
                called.sqlstate.clone_from(&st);
                called.message = read_varchar(&diag);
                called.sqlcode = if st.starts_with("01") { 462 } else { -443 };
            }
        }
        _ => {}
    }
    let mut outs = Vec::new();
    if called.sqlcode >= 0 {
        for (i, p) in plan.parms.iter().enumerate() {
            let Some(m) = p.marker else { continue };
            if p.mode == 1 {
                continue;
            }
            let null = match plan.style {
                CallStyle::General => false,
                CallStyle::GeneralNulls => i16::from_be_bytes([ind_array[2 * i], ind_array[2 * i + 1]]) < 0,
                _ => i16::from_be_bytes([ind_bufs[i][0], ind_bufs[i][1]]) < 0,
            };
            let v = if null { Value::Null } else { crate::sql::value_of(&bufs[i], &Desc::of(p.desc), p.extra) };
            outs.push((m, v));
        }
    }
    Outcome { called, outs, answer: None }
}

/// Run one statement for a requester in the job that has no host
/// variables of its own (a command line, a server): a `CALL` it plans is
/// made here, its `OUT` and `INOUT` values given back by marker.
pub fn run_request(job: &mut mi::Job, mut req: sql::embed::Request) -> (sql::embed::Reply, Vec<(usize, Value)>) {
    req.desc.calls = true;
    let Some(reply) = mi::io::trigger::sql_run(job, &req) else { return (sql::embed::Reply::default(), Vec::new()) };
    let Some(plan) = reply.call.clone() else { return (reply, Vec::new()) };
    let out = perform(job, &plan);
    let req2 = sql::embed::Request {
        scope: req.scope.clone(),
        desc: sql::embed::Desc { op: sql::embed::Op::Called, ..sql::embed::Desc::default() },
        inputs: out.called.inputs(),
    };
    let Some(r) = mi::io::trigger::sql_run(job, &req2) else { return (sql::embed::Reply::default(), Vec::new()) };
    let outs = if r.sqlca.is_error() { Vec::new() } else { out.outs };
    (r, outs)
}

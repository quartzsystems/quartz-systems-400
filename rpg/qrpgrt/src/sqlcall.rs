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

/// Make the call the plan describes.
#[allow(clippy::too_many_lines)] // the styles in order
pub fn perform(job: &mut mi::Job, plan: &CallPlan) -> Outcome {
    let n = plan.parms.len();
    let mut called = Called {
        seq: plan.seq,
        max: plan.max_results,
        nested: DEPTH.with(Cell::get) > 0,
        sqlstate: "00000".into(),
        routine: plan.routine.clone(),
        ..Called::default()
    };
    if !plan.entry.is_empty() {
        // BLOCKED(sql): a service program procedure takes at most six
        // parameters through the runtime's procedure-pointer call.
        called.sqlcode = -84;
        called.sqlstate = "42612".into();
        called.message = format!("CALL of service program procedure {}/{}({}): not supported", plan.library, plan.program, plan.entry);
        return Outcome { called, outs: Vec::new() };
    }
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
                    return Outcome { called, outs: Vec::new() };
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
        CallStyle::General => {}
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
    let r = job.call_program(&plan.program, &plan.library, &args);
    DEPTH.with(|d| d.set(d.get() - 1));
    if let Err(c) = r {
        called.sqlcode = -204;
        called.sqlstate = "42704".into();
        called.message = format!("program {}/{} could not be called: {}", plan.library, plan.program, c.msgid());
        return Outcome { called, outs: Vec::new() };
    }
    if let Some(e) = job.clear() {
        // The program ended with an exception it did not handle.
        called.sqlcode = -443;
        called.sqlstate = "38501".into();
        called.message = format!("procedure {} ended with {}", plan.routine, e.msgid);
        return Outcome { called, outs: Vec::new() };
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
    Outcome { called, outs }
}

/// Run one statement for a requester in the job that has no host
/// variables of its own (a command line, a server): a `CALL` it plans is
/// made here, its `OUT` and `INOUT` values given back by marker.
pub fn run_request(job: &mut mi::Job, mut req: sql::embed::Request) -> (sql::embed::Reply, Vec<(usize, Value)>) {
    req.desc.calls = true;
    let Some(s) = job.sql.as_mut() else { return (sql::embed::Reply::default(), Vec::new()) };
    let reply = s.run(&req);
    let Some(plan) = reply.call.clone() else { return (reply, Vec::new()) };
    let out = perform(job, &plan);
    let req2 = sql::embed::Request {
        scope: req.scope.clone(),
        desc: sql::embed::Desc { op: sql::embed::Op::Called, ..sql::embed::Desc::default() },
        inputs: out.called.inputs(),
    };
    let Some(s) = job.sql.as_mut() else { return (sql::embed::Reply::default(), Vec::new()) };
    let r = s.run(&req2);
    let outs = if r.sqlca.is_error() { Vec::new() } else { out.outs };
    (r, outs)
}

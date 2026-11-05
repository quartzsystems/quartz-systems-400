// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Trigger programs of record operations, called in the job. A file's
//! source defers them (`db::trg::Mode::Defer`): an operation with before
//! triggers answers [`TRIGGERS_DUE`] with their calls and changes
//! nothing; the job calls them in order, a before trigger with
//! `ALWREPCHG(*YES)` passing its new record to the next, and gives the
//! operation again with a [`TriggerResume`]. An operation done answers
//! the after calls for the job to make; one that fails gives status
//! 01024. `docs/STORAGE_SERVICE.md` "Trigger programs".
//!
//! An SQL statement makes its calls in its middle instead: the database
//! answers with a call plan and waits; [`sql_run`] makes the call and
//! gives the answer back until the statement ends.

use super::{IoRequest, IoResult};
use crate::Job;

/// The status of an operation whose before triggers are due.
pub const TRIGGERS_DUE: u16 = 0xFFFE;
/// A trigger program failed before the operation.
pub const FAILED_BEFORE: u16 = 1023;
/// A trigger program failed after the operation.
pub const FAILED_AFTER: u16 = 1024;

/// One call of a trigger program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggerCall {
    /// The trigger.
    pub trigger: String,
    /// The program's library.
    pub lib: String,
    /// The program.
    pub pgm: String,
    /// A before trigger.
    pub before: bool,
    /// `ALWREPCHG(*YES)`.
    pub alwrepchg: bool,
    /// The file.
    pub file: String,
    /// The trigger buffer.
    pub buffer: Vec<u8>,
}

/// Why a call failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggerFail {
    /// The trigger.
    pub trigger: String,
    /// The program's escape, or `CPF4160` when it was not found.
    pub msgid: String,
    /// Its text or data.
    pub text: String,
    /// Not found.
    pub not_found: bool,
}

/// The job's answer to before calls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggerResume {
    /// The buffer the last `ALWREPCHG(*YES)` trigger left.
    pub buffer: Option<Vec<u8>>,
    /// The call that failed.
    pub failed: Option<TriggerFail>,
}

/// Call one trigger program: the buffer and its length, `BINARY(4)`.
///
/// # Errors
///
/// The program could not be called or ended in an exception.
pub fn call(j: &mut Job, c: &mut TriggerCall) -> Result<(), TriggerFail> {
    let mut len = u32::try_from(c.buffer.len()).unwrap_or(u32::MAX).to_be_bytes();
    let addrs = [c.buffer.as_mut_ptr() as u64, len.as_mut_ptr() as u64];
    let fail = |msgid: String, text: String, not_found| TriggerFail { trigger: c.trigger.clone(), msgid, text, not_found };
    if let Err(cond) = j.call_program(&c.pgm, &c.lib, &addrs) {
        let _ = j.clear();
        let not_found = matches!(cond, crate::cond::Cond::Mch3401);
        let id = if not_found { "CPF4160".to_string() } else { cond.msgid() };
        return Err(fail(id, format!("{}/{} could not be called", c.lib, c.pgm), not_found));
    }
    if let Some(e) = j.clear() {
        return Err(fail(e.msgid.clone(), ccsid::decode_trimmed(&e.data), false));
    }
    Ok(())
}

/// Copy the new record and its null map of `from` into `to`: buffers of
/// one change have one layout.
fn pass_new(from: &[u8], to: &mut [u8]) {
    let at = |b: &[u8], i: usize| u32::from_be_bytes(b[i..i + 4].try_into().expect("4")) as usize;
    if from.len() < 96 || to.len() < 96 {
        return;
    }
    let (start, len) = (at(from, 64), at(from, 68) + at(from, 76));
    if let (Some(src), true) = (from.get(start..start + len), to.len() >= start + len) {
        to[start..start + len].copy_from_slice(src);
    }
}

/// The before calls, made in order.
pub fn call_before(j: &mut Job, calls: &mut [TriggerCall]) -> TriggerResume {
    let mut out = TriggerResume::default();
    for c in calls.iter_mut() {
        if let Some(b) = &out.buffer {
            pass_new(b, &mut c.buffer);
        }
        if let Err(f) = call(j, c) {
            out.failed = Some(f);
            return out;
        }
        if c.alwrepchg {
            out.buffer = Some(c.buffer.clone());
        }
    }
    out
}

/// One record operation with its trigger programs: `io` runs the
/// operation on the job's source (the same request, resumed once with the
/// before calls' answer); the after calls follow.
pub fn run(j: &mut Job, req: &IoRequest<'_>, io: &mut dyn FnMut(&mut Job, &IoRequest<'_>) -> IoResult) -> IoResult {
    let mut r = io(j, req);
    if r.status == TRIGGERS_DUE {
        let mut calls = std::mem::take(&mut r.triggers);
        let resume = call_before(j, &mut calls);
        let again = IoRequest { resume: Some(&resume), ..*req };
        r = io(j, &again);
    }
    if r.status == TRIGGERS_DUE {
        // Due again: the source did not take the answer.
        r.status = FAILED_BEFORE;
        r.msgid = Some("CPF502B".into());
        r.triggers.clear();
        return r;
    }
    for mut c in std::mem::take(&mut r.triggers) {
        if let Err(f) = call(j, &mut c) {
            tracing::debug!("trigger {} failed: {} {}", f.trigger, f.msgid, f.text);
            r.status = FAILED_AFTER;
            r.msgid = Some("CPF502B".into());
            break;
        }
    }
    r
}

/// Run one SQL statement through the job's session, making the calls
/// the database asks for in the middle of it (trigger programs, style
/// `Trigger`) and answering each, until it ends. Any other plan (a
/// `CALL`'s) is the caller's. `None` when the job has no SQL session.
pub fn sql_run(j: &mut Job, req: &sql::embed::Request) -> Option<sql::embed::Reply> {
    use sql::exec::routine::{CallStyle, midcall};
    let mut reply = j.sql.as_mut()?.run(req);
    while let Some(plan) = reply.call.take_if(|p| p.style == CallStyle::Trigger) {
        let mut c = TriggerCall {
            trigger: plan.routine.clone(),
            lib: plan.library.clone(),
            pgm: plan.program.clone(),
            file: plan.specific.clone(),
            buffer: midcall::plan_buffer(&plan),
            ..TriggerCall::default()
        };
        let failed = call(j, &mut c).err().map(|f| db::trg::Failure { msgid: f.msgid, text: f.text, not_found: f.not_found });
        let answer = sql::embed::Request {
            scope: req.scope.clone(),
            desc: sql::embed::Desc { op: sql::embed::Op::Called, name: midcall::ANSWER.into(), ..sql::embed::Desc::default() },
            inputs: midcall::trigger_inputs(&c.buffer, failed.as_ref()),
        };
        reply = j.sql.as_mut()?.run(&answer);
    }
    Some(reply)
}

impl TriggerCall {
    /// From the database's call.
    #[must_use]
    pub fn of(c: &db::trg::Call) -> Self {
        Self {
            trigger: c.trigger.clone(),
            lib: ccsid::decode_trimmed(c.program.0.as_bytes()),
            pgm: ccsid::decode_trimmed(c.program.1.as_bytes()),
            before: c.time != db::trg::Time::After,
            alwrepchg: c.alwrepchg,
            file: ccsid::decode_trimmed(c.file.as_bytes()),
            buffer: c.buffer.clone(),
        }
    }
}

impl TriggerResume {
    /// For the database.
    #[must_use]
    pub fn to_db(&self) -> db::trg::fire::Resume {
        db::trg::fire::Resume {
            buffer: self.buffer.clone(),
            failed: self.failed.as_ref().map(|f| {
                (f.trigger.clone(), db::trg::Failure { msgid: f.msgid.clone(), text: f.text.clone(), not_found: f.not_found })
            }),
        }
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls made in the middle of a statement. The database cannot run a
//! program, and a statement that needs one called before it can go on
//! (a trigger program, an external function) cannot end first as `CALL`
//! does. So it hands a [`CallPlan`] to a [`MidCaller`] the requester's
//! side set up: the storage service suspends the statement, answers the
//! job's request with the plan, and resumes the statement when the job's
//! [`crate::embed::Op::Called`] named [`ANSWER`] brings the outcome back
//! (`docs/STORAGE_SERVICE.md` "Trigger programs"). The job makes the
//! call and answers in `mi::io::trigger::sql_run`.
//!
//! [`Triggers`] is the trigger programs' use of it. Another use gives
//! its own plan style and reads its own answer; the suspension, the
//! store given back and the routing are the same.

use std::cell::RefCell;
use std::rc::Rc;

use db::trg::{Call, Caller, Failure};
use store::txn::Txn;

use super::{CallPlan, CallStyle, PlanParm};
use crate::value::Value;

/// The name an [`crate::embed::Op::Called`] carries when it answers a
/// call made in the middle of a statement, not a `CALL`.
pub const ANSWER: &str = "*MIDCALL";

/// Make one call in the middle of a statement: the plan out, the
/// requester's answer (the inputs of its `Op::Called`) back; `None` when
/// the statement is to end instead (the service closes, the job went).
/// The store is given back while it runs.
pub type MidCaller = Rc<dyn Fn(CallPlan) -> Option<Vec<Value>>>;

/// The plan of one trigger program call: the buffer is its one argument.
#[must_use]
pub fn trigger_plan(c: &Call) -> CallPlan {
    CallPlan {
        program: ccsid::decode_trimmed(c.program.1.as_bytes()),
        library: ccsid::decode_trimmed(c.program.0.as_bytes()),
        style: CallStyle::Trigger,
        routine: c.trigger.clone(),
        specific: ccsid::decode_trimmed(c.file.as_bytes()),
        parms: vec![PlanParm { mode: 3, desc: 0, extra: 0, value: Value::Bin(c.buffer.clone()), marker: None }],
        ..CallPlan::default()
    }
}

/// The trigger buffer of a plan of style [`CallStyle::Trigger`].
#[must_use]
pub fn plan_buffer(p: &CallPlan) -> Vec<u8> {
    match p.parms.first().map(|p| &p.value) {
        Some(Value::Bin(b)) => b.clone(),
        _ => Vec::new(),
    }
}

/// The job's answer to a trigger call: the buffer as the program left
/// it, and why it failed.
#[must_use]
pub fn trigger_inputs(buffer: &[u8], failed: Option<&Failure>) -> Vec<Value> {
    let f = failed.cloned().unwrap_or_default();
    vec![
        Value::Bin(buffer.to_vec()),
        Value::Str(f.msgid),
        Value::Str(f.text),
        Value::int(i64::from(failed.is_some_and(|f| f.not_found))),
    ]
}

/// [`trigger_inputs`] read back.
fn trigger_answer(v: &[Value]) -> Option<(Vec<u8>, Option<Failure>)> {
    let Some(Value::Bin(buf)) = v.first() else { return None };
    let msgid = v.get(1)?.as_text();
    let failure = (!msgid.trim().is_empty()).then(|| Failure {
        msgid: msgid.trim().to_string(),
        text: v.get(2).map(Value::as_text).unwrap_or_default(),
        not_found: v.get(3).and_then(|x| x.as_i64().ok()) == Some(1),
    });
    Some((buf.clone(), failure))
}

/// The calls of `MODE DB2SQL` after triggers a statement has made so
/// far, made when it has changed every row ([`fire_deferred`]).
pub type Deferred = Rc<RefCell<Vec<Call>>>;

/// Trigger programs called through a [`MidCaller`].
pub struct Triggers {
    mid: MidCaller,
    later: Deferred,
}

impl Triggers {
    /// A shared caller for an open data path's trigger mode; deferred
    /// calls go to `later`.
    #[must_use]
    pub fn shared(mid: MidCaller, later: Deferred) -> db::trg::fire::CallerRef {
        Rc::new(RefCell::new(Self { mid, later }))
    }

    fn now(&self, txn: &mut Txn<'_>, call: &mut Call) -> std::result::Result<(), Failure> {
        // What the statement wrote so far is committed and the store
        // given back: the program's own requests are others'.
        txn.commit_part().map_err(|e| refused(call, &e.to_string()))?;
        if !txn.suspend() {
            return Err(refused(call, "the statement cannot give the store back"));
        }
        let answer = (self.mid)(trigger_plan(call));
        txn.resume().map_err(|e| refused(call, &e.to_string()))?;
        let Some(v) = answer else {
            return Err(refused(call, "the statement ended while it ran"));
        };
        let (buf, failure) = trigger_answer(&v).ok_or_else(|| refused(call, "its answer was not understood"))?;
        if let Some(f) = failure {
            return Err(f);
        }
        if buf.len() == call.buffer.len() {
            call.buffer = buf;
        }
        Ok(())
    }
}

/// A statement has changed every row: the `MODE DB2SQL` after triggers it
/// deferred, called in the order the rows changed.
///
/// # Errors
///
/// A call failed (`SQL0723`, `SQL0443`, `SQL0204`): the later ones are
/// not made.
pub fn fire_deferred(txn: &mut Txn<'_>, mid: Option<&MidCaller>, later: &Deferred) -> crate::error::Result<()> {
    let calls = std::mem::take(&mut *later.borrow_mut());
    let Some(mid) = mid else { return Ok(()) };
    let t = Triggers { mid: Rc::clone(mid), later: Rc::default() };
    for mut c in calls {
        t.now(txn, &mut c).map_err(|f| crate::error::SqlError::from(db::trg::fire::failed(c.file, &c.trigger, false, f)))?;
    }
    Ok(())
}

fn refused(c: &Call, why: &str) -> Failure {
    Failure { msgid: "CPF502B".into(), text: format!("trigger {} was not called: {why}", c.trigger), not_found: false }
}

impl Caller for Triggers {
    fn call(&mut self, txn: &mut Txn<'_>, call: &mut Call) -> std::result::Result<(), Failure> {
        if call.deferred {
            self.later.borrow_mut().push(call.clone());
            return Ok(());
        }
        self.now(txn, call)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsname::Name;

    #[test]
    fn a_trigger_plan_crosses_and_its_answer_comes_back() {
        let n = |s: &str| Name::parse(s).unwrap();
        let c = Call {
            trigger: "T1".into(),
            program: (n("QSTEST"), n("TRGPGM")),
            time: db::trg::Time::Before,
            event: db::trg::Event::Insert,
            alwrepchg: true,
            file: n("TRGT"),
            library: n("QSTEST"),
            buffer: vec![0x40, 0, 0xF1],
            deferred: false,
        };
        let p = CallPlan::decode(&trigger_plan(&c).encode()).unwrap();
        assert_eq!((p.style, p.program.as_str(), p.library.as_str(), p.routine.as_str()), (CallStyle::Trigger, "TRGPGM", "QSTEST", "T1"));
        assert_eq!(plan_buffer(&p), c.buffer);
        let ok = trigger_answer(&trigger_inputs(&[1, 2], None)).unwrap();
        assert_eq!(ok, (vec![1, 2], None));
        let f = Failure { msgid: "CPF4160".into(), text: "gone".into(), not_found: true };
        assert_eq!(trigger_answer(&trigger_inputs(&[], Some(&f))).unwrap().1, Some(f));
    }
}

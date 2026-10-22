// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message queues in the service: send, receive, reply, remove, and
//! break delivery. A queue in `*BREAK` delivery is allocated to the job
//! that set it; a message that arrives on it at or above its severity
//! waits in that job's break list until the job asks, at its next wait
//! point.

use store::obj::{ObjectId, types};
use wm::obj::msgq::{MsgKind, MsgQ, QMsg};
use wm::obj::params::qualified;

use crate::ipl::system::System;

/// Microseconds since the epoch.
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

/// `LIB/NAME` of a queue, `*LIBL` when unqualified. `*SYSOPR` is
/// `QSYS/QSYSOPR`.
pub fn queue_name(q: &str) -> (String, String) {
    match q.to_ascii_uppercase().as_str() {
        "*SYSOPR" | "QSYSOPR" => ("QSYS".into(), "QSYSOPR".into()),
        "*HSTLOG" | "QHST" => ("QSYS".into(), "QHST".into()),
        other => qualified(other),
    }
}

/// Run `f` on a queue, `op` checked first; written back when `f` says
/// it changed it.
pub fn with_queue<T>(
    system: &mut System,
    op: &'static str,
    lib: &str,
    name: &str,
    f: impl FnOnce(&mut MsgQ, ObjectId) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let id = wm::obj::find(txn, qsys, &names, lib, name, types::MSGQ)
            .map_err(|_| format!("CPF2403 message queue {name} in {lib} not found"))?;
        store::obj::guard::check_id(txn, op, id).map_err(|e| e.to_string())?;
        let mut q =
            wm::obj::msgq::MsgQ::decode(&wm::obj::read(txn, id).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let (r, changed) = f(&mut q, id)?;
        if changed {
            wm::obj::write(txn, id, &q.encode()).map_err(|e| e.to_string())?;
        }
        Ok(r)
    })
}

/// The sender as a job triple.
pub fn sender(system: &System, number: u32) -> (String, String, u32) {
    system.jobs.get(number).map_or_else(
        || ("*N".into(), "*N".into(), number),
        |j| (j.name.clone(), j.user.clone(), j.number),
    )
}

/// Send to `lib/name`; the key. A queue in break delivery gets the
/// message to its job's break list.
pub fn send(system: &mut System, lib: &str, name: &str, mut m: QMsg) -> Result<u32, String> {
    if m.time == 0 {
        m.time = now_micros();
    }
    if lib == "QSYS" && name == "QHST" {
        // What is sent to QHST goes into the history log.
        let kind = match m.kind {
            MsgKind::Comp => "01",
            MsgKind::Diag => "02",
            MsgKind::Inq => "05",
            MsgKind::Rpy => "21",
            MsgKind::Notify => "14",
            MsgKind::Escape => "15",
            MsgKind::Info => "04",
        };
        let h = super::hist::Hist {
            time: m.time / 1_000_000,
            msgid: m.msgid.clone(),
            severity: m.severity,
            kind,
            job: m.job.clone(),
            msgf: m.msgf.clone(),
            data: m.data.clone(),
            text: m.text.clone(),
        };
        super::hist::log(system, &h)?;
        return Ok(0);
    }
    let sent = m.clone();
    let (key, id, breaks, brkpgm) = with_queue(system, "msgq.send", lib, name, |q, id| {
        let key = q
            .send(m)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2460")))?;
        let breaks = q.delivery == "*BREAK" && sent.severity >= q.severity;
        Ok(((key, id, breaks, q.brkpgm.clone()), true))
    })?;
    if breaks && let Some(&job) = system.break_alloc.get(&id) {
        let mut b = sent;
        b.key = key;
        system
            .break_pending
            .entry(job)
            .or_default()
            .push((format!("{lib}/{name}"), brkpgm, b));
    }
    Ok(key)
}

/// A message from `sender_job` with immediate text.
pub fn immediate(system: &System, sender_job: u32, kind: MsgKind, text: &str) -> QMsg {
    QMsg {
        kind,
        text: text.to_string(),
        job: sender(system, sender_job),
        from_pgm: "QSVCJOB".into(),
        ..QMsg::default()
    }
}

/// Put `lib/name` in break delivery for `job`: `CPF2451` when another
/// active job has it.
pub fn allocate(system: &mut System, lib: &str, name: &str, job: u32) -> Result<(), String> {
    let id = with_queue(system, "msgq.allocate", lib, name, |_, id| Ok((id, false)))?;
    if let Some(&holder) = system.break_alloc.get(&id)
        && holder != job
        && system.jobs.in_use(holder)
    {
        return Err(format!(
            "CPF2451 message queue {name} is allocated to job {holder:06}"
        ));
    }
    system.break_alloc.insert(id, job);
    Ok(())
}

/// A job ended: its allocations and its waiting breaks go.
pub fn release(system: &mut System, job: u32) {
    system.break_alloc.retain(|_, j| *j != job);
    system.break_pending.remove(&job);
}

/// Take the breaks waiting for `job`.
pub fn take_breaks(system: &mut System, job: u32) -> Vec<Break> {
    system.break_pending.remove(&job).unwrap_or_default()
}

/// A break waiting: the queue, its break program and the message.
pub type Break = (String, (String, String), QMsg);

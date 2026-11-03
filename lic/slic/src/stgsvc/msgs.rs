// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message queues in the service: send, receive, reply, remove, and
//! break delivery. A queue in `*BREAK` delivery is allocated to the job
//! that set it; a message that arrives on it at or above its severity
//! waits in that job's break list until the job asks, at its next wait
//! point. An inquiry whose reply goes to another queue leaves a sender's
//! copy there, which its reply answers too.

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
    let upper = q.to_ascii_uppercase();
    // The analyzer spells an unqualified name `*LIBL/NAME`.
    let bare = upper.strip_prefix("*LIBL/").unwrap_or(&upper);
    match bare {
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
            MsgKind::Copy => "06",
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
    if lib == "QSYS" && name == "QSYSOPR" {
        let text = if m.text.is_empty() { m.msgid.clone() } else { m.text.clone() };
        super::prblog::message_sent(system, &m.msgid, m.severity, &text);
    }
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

/// Message `msgid`'s description in `msgf` (`*LIBL` when no library),
/// from the store or the built-in sources behind it.
pub fn description(system: &mut System, msgid: &str, msgf: &(String, String)) -> Option<msgf::MsgDesc> {
    let (file, lib) = (msgf.0.trim(), msgf.1.trim());
    let found = system.storage.as_mut().and_then(|s| {
        let qsys = s.qsys?;
        crate::svc::db::with_txn(&mut s.store, |txn| {
            let names = crate::svc::ile::name_context(txn, qsys);
            let f = qsname::Name::parse(file).map_err(|e| e.to_string())?;
            let spec = match lib {
                "*LIBL" | "" => qsname::LibSpec::Libl,
                l => qsname::LibSpec::Name(qsname::Name::parse(l).map_err(|e| e.to_string())?),
            };
            msgf::store::rtvmsg(txn, qsys, &names, msgid, f, spec).map_err(|e| e.to_string())
        })
        .ok()
        .flatten()
    });
    found.or_else(|| msgf::MsgLookup::describe(&mut msgf::BuiltinMsgs::new(), msgid, file, if lib.is_empty() { "*LIBL" } else { lib }))
}

/// A message-queue function refused: the message and its values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The message identifier.
    pub msgid: String,
    /// Its substitution values, in order.
    pub data: Vec<String>,
    /// The text the service's console shows.
    pub text: String,
}

impl Refused {
    /// From a function's `CPFnnnn text` error.
    #[must_use]
    pub fn of(e: &str) -> Self {
        let (id, text) = e.split_once(' ').unwrap_or((e, ""));
        Self {
            msgid: id.to_string(),
            data: Vec::new(),
            text: text.to_string(),
        }
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.msgid, self.text)
    }
}

/// Answer the inquiry `key` on `lib/name` as `from`, the reply checked
/// against the message's valid replies (`CPF2420`); the reply's key.
/// An empty reply is the default one. When the reply goes to another
/// queue, the sender's copy there is answered too.
pub fn reply(system: &mut System, lib: &str, name: &str, key: u32, reply: &str, from: (String, String, u32)) -> Result<u32, Refused> {
    let (inq, text) = with_queue(system, "msgq.reply", lib, name, |q, _| {
        let i = q.inquiry(key).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2422")))?;
        Ok(((q.msgs[i].clone(), q.reply_text(i, reply)), false))
    })
    .map_err(|e| Refused::of(&e))?;
    if !inq.msgid.is_empty()
        && let Some(d) = description(system, &inq.msgid, &inq.msgf)
        && !d.reply_ok(&text)
    {
        return Err(Refused {
            msgid: "CPF2420".into(),
            data: vec![text.clone(), inq.msgid.clone()],
            text: format!("reply {text} is not valid for message {}", inq.msgid),
        });
    }
    let time = now_micros();
    let rkey = with_queue(system, "msgq.reply", lib, name, |q, _| {
        let k = q
            .reply(key, &text, from.clone(), time)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2422")))?;
        Ok((k, true))
    })
    .map_err(|e| Refused::of(&e))?;
    let to = (inq.reply_to.1.clone(), inq.reply_to.0.clone());
    if !to.1.is_empty() && (to.0.as_str(), to.1.as_str()) != (lib, name) {
        // The sender's copy, when the sender left one: the system's
        // doing, whoever replied.
        let of = (name.to_string(), lib.to_string());
        let _ = sec::guard::as_system(|| {
            with_queue(system, "obj.check", &to.0, &to.1, |q, _| match q.copy_of(&of, key) {
                Some(c) => {
                    q.answer_copy(c, &text, from, time).map_err(|e| e.to_string())?;
                    Ok(((), true))
                }
                None => Ok(((), false)),
            })
        });
    }
    Ok(rkey)
}

/// Send the inquiry `m` to `lib/name`, its reply going to `reply_to`
/// (library, name): when that is another queue, a sender's copy goes
/// there. The key the reply will answer: the copy's, else the inquiry's.
pub fn send_inquiry(system: &mut System, lib: &str, name: &str, mut m: QMsg, reply_to: (&str, &str)) -> Result<u32, String> {
    m.kind = MsgKind::Inq;
    m.reply_to = (reply_to.1.to_string(), reply_to.0.to_string());
    // The copy is sent when the inquiry is.
    if m.time == 0 {
        m.time = now_micros();
    }
    let copy = QMsg {
        kind: MsgKind::Copy,
        reply_to: (name.to_string(), lib.to_string()),
        reply: String::new(),
        ..m.clone()
    };
    let key = send(system, lib, name, m)?;
    if reply_to == (lib, name) {
        return Ok(key);
    }
    let copy = QMsg { reply_key: key, ..copy };
    match with_queue(system, "msgq.send", reply_to.0, reply_to.1, |q, _| {
        let k = q.send(copy).map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2460")))?;
        Ok((k, true))
    }) {
        Ok(k) => Ok(k),
        Err(e) => {
            // No copy, no inquiry: its reply would go nowhere.
            let _ = sec::guard::as_system(|| with_queue(system, "obj.check", lib, name, |q, _| Ok((q.remove(key).is_ok(), true))));
            Err(e)
        }
    }
}

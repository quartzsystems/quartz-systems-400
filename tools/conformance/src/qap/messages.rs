// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 93: the `Messages` service and the spool watches (version 6)
//! through the real connection jobs, as `QAPTEST` and `QAPTEST2`: a
//! watch of `QAPTEST`'s own queue sees a message the console sends; an
//! inquiry to `QAPTEST2` leaves a sender's copy whose key the reply
//! answers; a second reply is refused (`CPF2422`); a queue not there
//! is `NotFound`; removal is all or none; lists are strictly newest
//! first; a watch of `QAPTEST`'s spooled files sees a batch job's log
//! arrive and its deletion; an unwatch stops a watch once. Then a batch
//! job's program fails unmonitored: its `CEE9901` inquiry arrives on a
//! watch of `QSYSOPR`, a reply its description does not take is `Error`
//! (`CPF2420`), `C` is taken and the job goes on to its end.

use std::time::Duration;

use qap_proto::ops::event;
use qap_proto::ops::messages::{
    Key, ListMessages, MessageArrived, MessageType, Messages, Queue, QueueRef, Recipient,
    RemoveMessages, ReplyMessage, SendMessage, Sent,
};
use qap_proto::ops::spool::{
    SpoolChange, SpoolTarget, SpooledFileChanged, SpooledFileRef, WatchSpool,
};
use qap_proto::types::ObjectName;
use qap_proto::{Envelope, Op, Service, Status};

use super::client::{Conn, brief, msgid};
use super::{USER, expect};
use crate::qemu::session::Session;
use crate::steps::CMD;

/// The second profile.
pub const USER2: &str = "QAPTEST2";
/// Its password.
pub const PASSWORD2: &str = "Qap4Two2";
/// How long an event may take: a watch looks every 250 ms.
const EVENT: Duration = Duration::from_secs(15);

fn status(r: &Envelope, want: Status, what: &str) -> Result<(), String> {
    if r.status == want {
        Ok(())
    } else {
        Err(format!("{what}: {} where {want:?} was due", brief(r)))
    }
}

fn list(c: &mut Conn, queue: Queue, newest_first: bool) -> Result<Messages, String> {
    c.ok(
        Op::ListMessages,
        &ListMessages {
            queue,
            newest_first,
            start: 0,
            max: 0,
        },
    )
}

/// The next event that `want` takes, within [`EVENT`]; others that
/// come first are counted.
fn wait<T>(
    c: &mut Conn,
    what: &str,
    skipped: &mut usize,
    want: impl Fn(&Envelope) -> Option<T>,
) -> Result<T, String> {
    let end = std::time::Instant::now() + EVENT;
    loop {
        let left = end.saturating_duration_since(std::time::Instant::now());
        let e = c
            .event(left)
            .map_err(|e| format!("waiting for {what}: {e}"))?;
        if let Some(t) = want(&e) {
            return Ok(t);
        }
        *skipped += 1;
    }
}

fn arrived(e: &Envelope) -> Option<MessageArrived> {
    (e.service == Service::Messages && e.op == event::MESSAGE_ARRIVED)
        .then(|| e.body_as().ok())
        .flatten()
}

fn spooled(e: &Envelope) -> Option<SpooledFileChanged> {
    (e.service == Service::Spool && e.op == event::SPOOLED_FILE_CHANGED)
        .then(|| e.body_as().ok())
        .flatten()
}

/// Run the step on `a` (`USER`) and `b` (`USER2`), both signed on.
#[allow(clippy::too_many_lines)]
pub fn run(s: &mut Session, a: &mut Conn, b: &mut Conn) -> Result<String, String> {
    let mut skipped = 0;
    status(
        &a.call(Op::WatchQueue, &QueueRef { queue: Queue::Own })?,
        Status::Ok,
        "WatchQueue",
    )?;
    // A message from the console arrives as an event.
    expect(
        s,
        &format!("SNDMSG MSG('Step 93 from the console') TOUSR({USER})"),
        &[""],
    )?;
    let console = wait(a, "the console's message", &mut skipped, |e| {
        arrived(e).filter(|m| m.message.text.contains("Step 93"))
    })?;
    if console.queue != Queue::Own || console.message.kind != MessageType::Informational {
        return Err(format!("the console's message: {console:?}"));
    }
    // An inquiry to USER2: a sender's copy under the key returned.
    let sent: Sent = a.ok(
        Op::SendMessage,
        &SendMessage {
            to: Recipient::User {
                profile: USER2.into(),
            },
            text: "Ready for step 93?".into(),
            inquiry: true,
        },
    )?;
    let key: Key = sent.key.ok_or("SendMessage gave an inquiry no key")?;
    let copy = wait(a, "the sender's copy", &mut skipped, |e| {
        arrived(e).filter(|m| m.message.key == key)
    })?;
    if copy.message.kind != MessageType::SenderCopy || copy.message.needs_reply {
        return Err(format!("the sender's copy: {copy:?}"));
    }
    let theirs = list(b, Queue::Own, true)?;
    let inq = theirs
        .messages
        .iter()
        .find(|m| m.text == "Ready for step 93?")
        .ok_or_else(|| format!("{USER2}'s queue has no inquiry: {:?}", theirs.messages))?;
    if !inq.needs_reply || inq.kind != MessageType::Inquiry || inq.from_user != USER {
        return Err(format!("{USER2}'s inquiry: {inq:?}"));
    }
    let answer = |c: &mut Conn, text: &str| {
        c.call(
            Op::ReplyMessage,
            &ReplyMessage {
                queue: Queue::Own,
                key: inq.key,
                reply: text.into(),
            },
        )
    };
    status(&answer(b, "YES")?, Status::Ok, "ReplyMessage")?;
    let again = answer(b, "NO")?;
    if again.status != Status::InvalidRequest || msgid(&again) != "CPF2422" {
        return Err(format!("a second reply: {}", brief(&again)));
    }
    let rpy = wait(a, "the reply", &mut skipped, |e| {
        arrived(e).filter(|m| m.message.kind == MessageType::Reply)
    })?;
    if rpy.message.answers != Some(key)
        || rpy.message.text != "YES"
        || rpy.message.from_user != USER2
    {
        return Err(format!("the reply on {USER}'s queue: {rpy:?}"));
    }
    // Strictly newest first, the copy answered.
    let own = list(a, Queue::Own, true)?;
    if own.messages.windows(2).any(|w| w[0].sent < w[1].sent)
        || own.messages.first().map(|m| m.key) != Some(rpy.message.key)
    {
        return Err(format!(
            "newest first: {:?}",
            own.messages
                .iter()
                .map(|m| (m.key, m.sent))
                .collect::<Vec<_>>()
        ));
    }
    let answered = own
        .messages
        .iter()
        .find(|m| m.key == key)
        .ok_or("the copy left the queue")?;
    if answered.reply.as_deref() != Some("YES") {
        return Err(format!("the copy's reply: {answered:?}"));
    }
    // A queue not there; removal all or none.
    let nope = a.call(
        Op::ListMessages,
        &ListMessages {
            queue: Queue::Named {
                queue: ObjectName {
                    library: "QGPL".into(),
                    name: "NOSUCHQ".into(),
                },
            },
            ..ListMessages::default()
        },
    )?;
    if nope.status != Status::NotFound || msgid(&nope) != "CPF2403" {
        return Err(format!("a queue not there: {}", brief(&nope)));
    }
    let rm = |c: &mut Conn, keys: Vec<Key>| {
        c.call(
            Op::RemoveMessages,
            &RemoveMessages {
                queue: Queue::Own,
                keys,
            },
        )
    };
    let partial = rm(a, vec![key, [0xFF; 4]])?;
    if partial.status != Status::NotFound || msgid(&partial) != "CPF2410" {
        return Err(format!("removing a key not there: {}", brief(&partial)));
    }
    status(&rm(a, vec![key])?, Status::Ok, "RemoveMessages")?;
    if list(a, Queue::Own, false)?
        .messages
        .iter()
        .any(|m| m.key == key)
    {
        return Err("the copy stayed after RemoveMessages".into());
    }
    let spool = spool_watch(s, a, &mut skipped)?;
    let batch = batch_inquiry(s, a, &mut skipped)?;
    status(
        &a.call(Op::UnwatchQueue, &QueueRef { queue: Queue::Own })?,
        Status::Ok,
        "UnwatchQueue",
    )?;
    let twice = a.call(Op::UnwatchQueue, &QueueRef { queue: Queue::Own })?;
    if twice.status != Status::NotFound || msgid(&twice) != "TCP1A5B" {
        return Err(format!("a second UnwatchQueue: {}", brief(&twice)));
    }
    Ok(format!(
        "a console message arrived as an event; an inquiry to {USER2} found its reply through its sender's copy (key {:08X}), a second reply CPF2422; CPF2403, CPF2410 all or none; newest first over {} messages; {spool}; {batch}; {skipped} other events passed by",
        u32::from_be_bytes(key),
        own.messages.len()
    ))
}

/// A watch of `USER`'s spooled files sees a batch job's log arrive and
/// go when deleted.
fn spool_watch(s: &mut Session, a: &mut Conn, skipped: &mut usize) -> Result<String, String> {
    let me = SpoolTarget::User {
        profile: "*CURRENT".into(),
    };
    status(
        &a.call(Op::WatchSpool, &WatchSpool { target: me.clone() })?,
        Status::Ok,
        "WatchSpool",
    )?;
    s.command("STRSBS QSYS/QBATCH", CMD)?;
    let sent = s.command(
        &format!("SBMJOB CMD(CRTLIB LIB(QGPL)) JOB(QAP93) USER({USER}) LOG(4 00 *SECLVL)"),
        CMD,
    )?;
    let added = wait(a, "the batch job's log", skipped, |e| {
        spooled(e).filter(|c| c.change == SpoolChange::Added && c.id.job.name == "QAP93")
    })
    .map_err(|e| format!("{e}\nSBMJOB: {sent}"))?;
    let file = added.file.ok_or("Added without its file")?;
    if added.target != me || file.user != USER {
        return Err(format!("the job log's event: {:?} {file:?}", added.target));
    }
    status(
        &a.call(
            Op::DeleteSpooledFile,
            &SpooledFileRef {
                id: added.id.clone(),
            },
        )?,
        Status::Ok,
        "DeleteSpooledFile",
    )?;
    let gone = wait(a, "the job log's deletion", skipped, |e| {
        spooled(e).filter(|c| c.id == added.id && c.change != SpoolChange::Added)
    })?;
    if gone.change != SpoolChange::Removed || gone.file.is_some() {
        return Err(format!("the deletion's event: {gone:?}"));
    }
    status(
        &a.call(Op::UnwatchSpool, &WatchSpool { target: me })?,
        Status::Ok,
        "UnwatchSpool",
    )?;
    Ok(format!(
        "{} {} arrived and was removed under a spool watch",
        added.id.file, added.id.number
    ))
}

/// A program that fails unmonitored: a decimal divide by zero.
const FAILING: &str = "\
module QAP93F
static $z: packed(3,0)
proc QAP93F() entry {
L0:
  dec.div $z, 1, 0 prec(3,0)
  ret
}
";

/// A batch job's `CEE9901` on `QSYSOPR`: seen on a watch, a reply it
/// does not take refused with the system's message, `C` taken, and the
/// job no longer waiting.
fn batch_inquiry(s: &mut Session, a: &mut Conn, skipped: &mut usize) -> Result<String, String> {
    crate::phase5::build(s, "QAP93F", FAILING)?;
    let w = a.call(
        Op::WatchQueue,
        &QueueRef {
            queue: Queue::SystemOperator,
        },
    )?;
    status(&w, Status::Ok, "WatchQueue QSYSOPR")?;
    s.command("STRSBS QSYS/QBATCH", CMD)?;
    let sent = s.command(
        "SBMJOB CMD('call QGPL/QAP93F') JOB(QAP93F) JOBQ(QGPL/QBATCH) INQMSGRPY(*RQD)",
        CMD,
    )?;
    let inq = wait(a, "the batch job's CEE9901", skipped, |e| {
        arrived(e).filter(|m| {
            m.queue == Queue::SystemOperator && m.message.id == "CEE9901" && m.message.needs_reply
        })
    })
    .map_err(|e| format!("{e}\nSBMJOB: {sent}"))?;
    if !inq.message.text.contains("QAP93F") || inq.message.help.is_empty() {
        return Err(format!("CEE9901's text: {:?}", inq.message));
    }
    let answer = |c: &mut Conn, text: &str| {
        c.call(
            Op::ReplyMessage,
            &ReplyMessage {
                queue: Queue::SystemOperator,
                key: inq.message.key,
                reply: text.into(),
            },
        )
    };
    let bad = answer(a, "X")?;
    if bad.status != Status::Error || msgid(&bad) != "CPF2420" {
        return Err(format!("reply X to CEE9901: {}", brief(&bad)));
    }
    status(&answer(a, "C")?, Status::Ok, "reply C to CEE9901")?;
    // The job takes the reply and ends; no longer in MSGW.
    let mut left = String::new();
    for _ in 0..80 {
        left = s.command("WRKACTJOB", CMD)?;
        if !left
            .lines()
            .any(|l| l.contains("QAP93F") && l.contains("MSGW"))
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if left
        .lines()
        .any(|l| l.contains("QAP93F") && l.contains("MSGW"))
    {
        return Err(format!("QAP93F still waits after the reply:\n{left}"));
    }
    let after = list(a, Queue::SystemOperator, true)?;
    let m = after
        .messages
        .iter()
        .find(|m| m.key == inq.message.key)
        .ok_or("CEE9901 left QSYSOPR")?;
    if m.reply.as_deref() != Some("C") || m.needs_reply {
        return Err(format!("CEE9901 after the reply: {m:?}"));
    }
    status(
        &a.call(
            Op::UnwatchQueue,
            &QueueRef {
                queue: Queue::SystemOperator,
            },
        )?,
        Status::Ok,
        "UnwatchQueue QSYSOPR",
    )?;
    Ok(
        "a batch job's CEE9901 arrived on QSYSOPR, X was refused (CPF2420), C let the job go on"
            .into(),
    )
}

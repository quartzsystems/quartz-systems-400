// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's message queues, under the rules the message
//! commands follow on QS/400 (docs/QAP.md, `Messages`):
//!
//! - keys go up as messages arrive and are never one a message on the
//!   queue has; lists are in arrival order;
//! - a queue with `users` is only theirs (`CPF9802`); `CPF2403` for a
//!   queue not there, `CPF2204` for a profile not there;
//! - a reply is checked against the inquiry's valid replies (`CPF2420`);
//!   an empty one is its default; `CPF2422` for a message that is not an
//!   inquiry waiting, `CPF2410` for a key not on the queue;
//! - an inquiry sent to another queue leaves a sender's copy on the
//!   sender's own queue; its reply answers the copy there too.
//!
//! The fixtures ([`install`]):
//!
//! - `QSYS/QSYSOPR`: every type of message, an inquiry `CPA0701`
//!   whose valid replies are `C`, `D`, `I` and `R` (default `C`), an
//!   impromptu inquiry from `ANN` (default `NO`), an answered one, and
//!   [`OPERATOR_FILLER`] more, so it fills several pages;
//! - `QUSRSYS/BOB`: every type, a sender's copy answered, an inquiry
//!   `CPA5305` taking `Y` or `N` (default `N`);
//! - `QUSRSYS/ANN` and `QUSRSYS/WARN`, the other profiles' own;
//! - `QGPL/SECRETQ`, which only `ANN` may use;
//! - `QGPL/TEAMQ`, held in break mode by job [`BREAK_JOB`]: watches
//!   pass it by.

use qap_proto::Status;
use qap_proto::ops::messages::{
    Key, ListMessages, MessageType, Queue, Recipient, RemoveMessages, ReplyMessage, SendMessage,
};
use qap_proto::types::{JobId, ObjectName};

use super::EPOCH;
use super::spool::No;

/// Informational messages on `QSYSOPR` past the fixtures' own.
pub const OPERATOR_FILLER: usize = 140;
/// The job that holds `QGPL/TEAMQ` in break mode.
pub const BREAK_JOB: u32 = 4242;

/// A message on a queue.
#[derive(Clone, Debug, Default)]
pub struct Msg {
    /// Its key.
    pub key: u32,
    /// Its identifier; empty for an impromptu message.
    pub id: String,
    /// The values its identifier's text takes.
    pub data: Vec<String>,
    /// Its type.
    pub kind: MessageType,
    /// Its severity.
    pub severity: u8,
    /// An impromptu message's text.
    pub text: String,
    /// An impromptu message's second-level text.
    pub help: String,
    /// When sent, milliseconds since 1970.
    pub sent: i64,
    /// The sending job.
    pub from: JobId,
    /// An inquiry or a sender's copy waiting for its reply.
    pub waiting: bool,
    /// Its reply, once it has one.
    pub reply: Option<String>,
    /// A reply's: the key it answers.
    pub answers: Option<u32>,
    /// An inquiry's valid replies; any when empty.
    pub valid: Vec<String>,
    /// An inquiry's default reply.
    pub dft: String,
    /// An inquiry's sender's copy: its queue and key there.
    pub copy: Option<(ObjectName, u32)>,
}

/// A message queue.
#[derive(Clone, Debug, Default)]
pub struct MsgQueue {
    /// Its library and name.
    pub queue: ObjectName,
    /// Who may use it; anyone when empty.
    pub users: Vec<String>,
    /// Its messages, in arrival order.
    pub msgs: Vec<Msg>,
    /// The next key.
    next_key: u32,
    /// The job holding it in break mode.
    pub break_job: Option<u32>,
}

impl MsgQueue {
    /// Put `m` on the queue; its key.
    pub fn put(&mut self, mut m: Msg) -> u32 {
        let mut k = self.next_key.max(1);
        while self.msgs.iter().any(|x| x.key == k) {
            k = k.wrapping_add(1).max(1);
        }
        self.next_key = k.wrapping_add(1).max(1);
        m.key = k;
        if matches!(m.kind, MessageType::Inquiry | MessageType::SenderCopy) && m.reply.is_none() {
            m.waiting = true;
        }
        self.msgs.push(m);
        k
    }

    fn at(&self, key: u32) -> Option<usize> {
        self.msgs.iter().position(|m| m.key == key)
    }
}

/// Every message queue.
#[derive(Clone, Debug, Default)]
pub struct Queues {
    /// The queues.
    pub queues: Vec<MsgQueue>,
    /// Profiles that exist.
    pub profiles: Vec<String>,
}

fn no(status: Status, id: &'static str, data: &[&str]) -> No {
    (status, id, data.iter().map(ToString::to_string).collect())
}

fn name(lib: &str, name: &str) -> ObjectName {
    ObjectName {
        library: lib.into(),
        name: name.into(),
    }
}

fn hex(k: u32) -> String {
    format!("{k:08X}")
}

/// A key as 4 bytes.
#[must_use]
pub fn key(k: u32) -> Key {
    k.to_be_bytes()
}

/// Libraries `*LIBL` searches.
const LIBL: [&str; 3] = ["QGPL", "QUSRSYS", "QSYS"];

impl Queues {
    /// Add a queue.
    pub fn add(&mut self, lib: &str, n: &str, users: &[&str]) -> &mut MsgQueue {
        self.queues.push(MsgQueue {
            queue: name(lib, n),
            users: users.iter().map(ToString::to_string).collect(),
            ..MsgQueue::default()
        });
        self.queues.last_mut().expect("just added")
    }

    /// The queue `lib/name`, `*LIBL` searched.
    #[must_use]
    pub fn find(&self, lib: &str, n: &str) -> Option<usize> {
        let (lib, n) = (
            lib.trim().to_ascii_uppercase(),
            n.trim().to_ascii_uppercase(),
        );
        let at = |l: &str| {
            self.queues
                .iter()
                .position(|q| q.queue.library == l && q.queue.name == n)
        };
        match lib.as_str() {
            "" | "*LIBL" | "*CURLIB" => LIBL.iter().find_map(|l| at(l)),
            l => at(l),
        }
    }

    /// Queue `queue` as `me` names it, for `me` to use.
    fn resolve(&self, me: &str, queue: &Queue) -> Result<usize, No> {
        let (lib, n) = match queue {
            Queue::Own => ("QUSRSYS".to_string(), me.to_string()),
            Queue::SystemOperator => ("QSYS".into(), "QSYSOPR".into()),
            Queue::Named { queue } => (queue.library.clone(), queue.name.clone()),
        };
        let i = self
            .find(&lib, &n)
            .ok_or_else(|| no(Status::NotFound, "CPF2403", &[n.trim(), lib.trim()]))?;
        let q = &self.queues[i];
        if !q.users.is_empty() && !q.users.iter().any(|u| u == me) {
            let what = format!("{}/{}", q.queue.library, q.queue.name);
            return Err(no(Status::NotAuthorized, "CPF9802", &[&what]));
        }
        Ok(i)
    }

    /// `ListMessages` for `me`: the messages, rendered by the caller.
    ///
    /// # Errors
    /// `NotFound`, `NotAuthorized`.
    pub fn list(&self, me: &str, req: &ListMessages) -> Result<(u32, Vec<Msg>), No> {
        let q = &self.queues[self.resolve(me, &req.queue)?];
        let mut all = q.msgs.clone();
        if req.newest_first {
            all.reverse();
        }
        let start = usize::try_from(req.start).unwrap_or(usize::MAX);
        let max = if req.max == 0 {
            usize::MAX
        } else {
            usize::try_from(req.max).unwrap_or(usize::MAX)
        };
        let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
        Ok((total, all.into_iter().skip(start).take(max).collect()))
    }

    /// `ReplyMessage` from `me` in job `from`.
    ///
    /// # Errors
    /// `NotFound` (queue or key), `NotAuthorized`, `InvalidRequest` (not
    /// an inquiry waiting), `Error` (`CPF2420`, a reply it does not take).
    pub fn reply(
        &mut self,
        me: &str,
        from: &JobId,
        now: i64,
        req: &ReplyMessage,
    ) -> Result<(), No> {
        let qi = self.resolve(me, &req.queue)?;
        let k = u32::from_be_bytes(req.key);
        let q = &mut self.queues[qi];
        let i = q
            .at(k)
            .ok_or_else(|| no(Status::NotFound, "CPF2410", &[&hex(k)]))?;
        let m = &q.msgs[i];
        if m.kind != MessageType::Inquiry || !m.waiting {
            return Err(no(Status::InvalidRequest, "CPF2422", &[&hex(k)]));
        }
        let text = if req.reply.trim().is_empty() {
            m.dft.clone()
        } else {
            req.reply.trim().to_string()
        };
        if !m.valid.is_empty() && !m.valid.iter().any(|v| v.eq_ignore_ascii_case(&text)) {
            return Err(no(Status::Error, "CPF2420", &[&text, &m.id]));
        }
        let copy = m.copy.clone();
        let m = &mut q.msgs[i];
        m.waiting = false;
        m.reply = Some(text.clone());
        let reply = |answers: u32| Msg {
            kind: MessageType::Reply,
            text: text.clone(),
            sent: now,
            from: from.clone(),
            answers: Some(answers),
            ..Msg::default()
        };
        q.put(reply(k));
        if let Some((cq, ck)) = copy
            && let Some(ci) = self.find(&cq.library, &cq.name)
        {
            let cq = &mut self.queues[ci];
            if let Some(c) = cq.at(ck) {
                cq.msgs[c].waiting = false;
                cq.msgs[c].reply = Some(text.clone());
                cq.put(reply(ck));
            }
        }
        Ok(())
    }

    /// `RemoveMessages` for `me`: all, or none when a key is not there.
    ///
    /// # Errors
    /// `NotFound` (queue or key), `NotAuthorized`.
    pub fn remove(&mut self, me: &str, req: &RemoveMessages) -> Result<(), No> {
        let qi = self.resolve(me, &req.queue)?;
        let q = &mut self.queues[qi];
        if let Some(k) = req
            .keys
            .iter()
            .map(|k| u32::from_be_bytes(*k))
            .find(|k| q.at(*k).is_none())
        {
            return Err(no(Status::NotFound, "CPF2410", &[&hex(k)]));
        }
        let keys: Vec<u32> = req.keys.iter().map(|k| u32::from_be_bytes(*k)).collect();
        q.msgs.retain(|m| !keys.contains(&m.key));
        Ok(())
    }

    /// `SendMessage` from `me` in job `from`: the key an inquiry's reply
    /// will answer.
    ///
    /// # Errors
    /// `NotFound` (queue or profile), `NotAuthorized`, `InvalidRequest`
    /// (no text).
    pub fn send(
        &mut self,
        me: &str,
        from: &JobId,
        now: i64,
        req: &SendMessage,
    ) -> Result<Option<u32>, No> {
        if req.text.trim().is_empty() {
            return Err(no(Status::InvalidRequest, "CPD0071", &["MSG"]));
        }
        let to = match &req.to {
            Recipient::User { profile } => {
                let p = profile.trim().to_ascii_uppercase();
                if !self.profiles.contains(&p) {
                    return Err(no(Status::NotFound, "CPF2204", &[]));
                }
                self.find("QUSRSYS", &p)
                    .ok_or_else(|| no(Status::NotFound, "CPF2403", &[&p, "QUSRSYS"]))?
            }
            Recipient::Queue { queue } => self.resolve(me, queue)?,
        };
        let m = Msg {
            kind: if req.inquiry {
                MessageType::Inquiry
            } else {
                MessageType::Informational
            },
            severity: if req.inquiry { 99 } else { 0 },
            text: req.text.clone(),
            sent: now,
            from: from.clone(),
            ..Msg::default()
        };
        if !req.inquiry {
            self.queues[to].put(m);
            return Ok(None);
        }
        let own = self.resolve(me, &Queue::Own)?;
        if own == to {
            return Ok(Some(self.queues[to].put(m)));
        }
        let ck = self.queues[own].put(Msg {
            kind: MessageType::SenderCopy,
            ..m.clone()
        });
        let own_name = self.queues[own].queue.clone();
        self.queues[to].put(Msg {
            copy: Some((own_name, ck)),
            ..m
        });
        Ok(Some(ck))
    }
}

fn job(name: &str, user: &str, number: u32) -> JobId {
    JobId {
        name: name.into(),
        user: user.into(),
        number: format!("{number:06}"),
    }
}

/// One message of each kind but inquiries and their replies, sent by
/// `from` from `at`.
fn every_kind(q: &mut MsgQueue, from: &JobId, at: &mut i64) {
    let mut put = |kind: MessageType, id: &str, data: &[&str], sev: u8, text: &str| {
        *at += 1000;
        q.put(Msg {
            id: id.into(),
            data: data.iter().map(ToString::to_string).collect(),
            kind,
            severity: sev,
            text: text.into(),
            help: if id.is_empty() {
                String::new()
            } else {
                format!("Help for {id}.")
            },
            sent: *at,
            from: from.clone(),
            ..Msg::default()
        });
    };
    put(
        MessageType::Informational,
        "",
        &[],
        0,
        "Backups start at 22:00 tonight.",
    );
    put(MessageType::Completion, "CPC2102", &["HLTEST"], 0, "");
    put(MessageType::Diagnostic, "CPD0071", &["MSG"], 30, "");
    put(MessageType::Escape, "CPF2103", &["HLTEST"], 40, "");
    put(MessageType::Notify, "CPF5026", &["QGPL"], 30, "");
}

/// The shipped queues and messages.
pub fn install(qs: &mut Queues) {
    qs.profiles = vec!["BOB".into(), "WARN".into(), "ANN".into()];
    let mut at = i64::try_from(EPOCH).unwrap_or(0) * 1000;
    let sysarb = job("QSYSARB", "QSYS", 12);
    let ann = job("QPADEV0002", "ANN", 1235);
    let bob = job("QPADEV0001", "BOB", 1234);
    let op = qs.add("QSYS", "QSYSOPR", &[]);
    every_kind(op, &sysarb, &mut at);
    at += 1000;
    op.put(Msg {
        id: "CPA0701".into(),
        data: vec!["QPADEV0003".into()],
        kind: MessageType::Inquiry,
        severity: 99,
        sent: at,
        from: sysarb.clone(),
        valid: vec!["C".into(), "D".into(), "I".into(), "R".into()],
        dft: "C".into(),
        ..Msg::default()
    });
    at += 1000;
    op.put(Msg {
        kind: MessageType::Inquiry,
        severity: 99,
        text: "Is the forms printer loaded?".into(),
        sent: at,
        from: ann.clone(),
        dft: "NO".into(),
        ..Msg::default()
    });
    at += 1000;
    let answered = op.put(Msg {
        kind: MessageType::Inquiry,
        severity: 99,
        text: "May the night jobs start?".into(),
        sent: at,
        from: ann.clone(),
        reply: Some("YES".into()),
        dft: "YES".into(),
        ..Msg::default()
    });
    at += 1000;
    op.put(Msg {
        kind: MessageType::Reply,
        text: "YES".into(),
        sent: at,
        from: bob.clone(),
        answers: Some(answered),
        ..Msg::default()
    });
    for i in 0..OPERATOR_FILLER {
        at += 1000;
        op.put(Msg {
            kind: MessageType::Informational,
            text: format!("Job {:06}/QUSER/QZDASOINIT ended normally.", 2000 + i),
            sent: at,
            from: sysarb.clone(),
            ..Msg::default()
        });
    }
    let own = qs.add("QUSRSYS", "BOB", &[]);
    every_kind(own, &bob, &mut at);
    at += 1000;
    own.put(Msg {
        id: "CPA5305".into(),
        data: vec!["PRT01".into()],
        kind: MessageType::Inquiry,
        severity: 99,
        sent: at,
        from: sysarb.clone(),
        valid: vec!["Y".into(), "N".into()],
        dft: "N".into(),
        ..Msg::default()
    });
    at += 1000;
    let copy = own.put(Msg {
        kind: MessageType::SenderCopy,
        severity: 99,
        text: "Can you cover the help desk at noon?".into(),
        sent: at,
        from: bob.clone(),
        reply: Some("OK".into()),
        ..Msg::default()
    });
    at += 1000;
    own.put(Msg {
        kind: MessageType::Reply,
        text: "OK".into(),
        sent: at,
        from: ann,
        answers: Some(copy),
        ..Msg::default()
    });
    qs.add("QUSRSYS", "ANN", &["ANN"]);
    qs.add("QUSRSYS", "WARN", &["WARN"]);
    qs.add("QGPL", "SECRETQ", &["ANN"]);
    qs.add("QGPL", "TEAMQ", &[]).break_job = Some(BREAK_JOB);
}

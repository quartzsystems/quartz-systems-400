// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*MSGQ`: a message queue and its messages. The attributes come
//! first, then the messages oldest first, each a fixed part and its
//! data, text, second-level text and reply. A message keeps its text
//! as substituted when it was sent.
//!
//! Which job a queue is allocated to for break delivery is not kept
//! here: an allocation ends with the job, so the storage service keeps
//! it in memory.

use ipc::wire_struct;

use crate::rec::{Layout, Rec, field, text_bytes, text_of};
use crate::{Error, Result};

/// The attributes.
pub const LAYOUT: Layout = Layout {
    name: "MessageQueue",
    size: Some(128),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Severity", 10, 1),
        field("Reserved1", 11, 1),
        field("Delivery", 12, 10),
        field("Full", 22, 10),
        field("BrkPgmName", 32, 10),
        field("BrkPgmLib", 42, 10),
        field("SizeInitial", 52, 4),
        field("SizeIncrement", 56, 4),
        field("SizeMax", 60, 4),
        field("NextKey", 64, 4),
        field("Count", 68, 4),
        field("Text", 72, 50),
        field("Reserved2", 122, 6),
    ],
};

/// One message's fixed part.
pub const ENTRY_LAYOUT: Layout = Layout {
    name: "MessageEntry",
    size: Some(112),
    fields: &[
        field("Key", 0, 4),
        field("ReplyKey", 4, 4),
        field("Type", 8, 1),
        field("Severity", 9, 1),
        field("Status", 10, 1),
        field("Reserved", 11, 1),
        field("Time", 12, 8),
        field("MsgId", 20, 7),
        field("Pad", 27, 1),
        field("MsgfName", 28, 10),
        field("MsgfLib", 38, 10),
        field("JobName", 48, 10),
        field("JobUser", 58, 10),
        field("JobNumber", 68, 4),
        field("FromPgm", 72, 10),
        field("ReplyTo", 82, 10),
        field("ReplyToLib", 92, 10),
        field("DataLen", 102, 2),
        field("TextLen", 104, 2),
        field("HelpLen", 106, 2),
        field("ReplyLen", 108, 2),
        field("DftLen", 110, 2),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMMSGQ";
const VERSION: u64 = 1;

/// A message type, as the queue keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MsgKind {
    /// `*INFO`.
    #[default]
    Info,
    /// `*INQ`: waits for a reply.
    Inq,
    /// `*RPY`: the reply to an inquiry.
    Rpy,
    /// `*COMP`.
    Comp,
    /// `*DIAG`.
    Diag,
    /// `*NOTIFY`.
    Notify,
    /// `*ESCAPE`.
    Escape,
}

impl MsgKind {
    fn code(self) -> u8 {
        match self {
            Self::Info => 1,
            Self::Inq => 2,
            Self::Rpy => 3,
            Self::Comp => 4,
            Self::Diag => 5,
            Self::Notify => 6,
            Self::Escape => 7,
        }
    }

    fn of(c: u8) -> Self {
        match c {
            2 => Self::Inq,
            3 => Self::Rpy,
            4 => Self::Comp,
            5 => Self::Diag,
            6 => Self::Notify,
            7 => Self::Escape,
            _ => Self::Info,
        }
    }

    /// `*INFO`, `*INQ` ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Info => "*INFO",
            Self::Inq => "*INQ",
            Self::Rpy => "*RPY",
            Self::Comp => "*COMP",
            Self::Diag => "*DIAG",
            Self::Notify => "*NOTIFY",
            Self::Escape => "*ESCAPE",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "*INFO" => Self::Info,
            "*INQ" => Self::Inq,
            "*RPY" => Self::Rpy,
            "*COMP" => Self::Comp,
            "*DIAG" => Self::Diag,
            "*NOTIFY" => Self::Notify,
            "*ESCAPE" => Self::Escape,
            _ => return None,
        })
    }
}

impl ipc::Wire for MsgKind {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(self.code());
    }
    fn get(d: &mut ipc::Dec<'_>) -> std::result::Result<Self, ipc::WireError> {
        Ok(Self::of(u8::get(d)?))
    }
}

/// Where a message stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Not received yet.
    #[default]
    New,
    /// Received (`*OLD`).
    Old,
    /// An inquiry waiting for its reply.
    Unanswered,
    /// An inquiry that has its reply.
    Answered,
}

impl Status {
    fn code(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Old => 1,
            Self::Unanswered => 2,
            Self::Answered => 3,
        }
    }

    fn of(c: u8) -> Self {
        match c {
            1 => Self::Old,
            2 => Self::Unanswered,
            3 => Self::Answered,
            _ => Self::New,
        }
    }
}

impl ipc::Wire for Status {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(self.code());
    }
    fn get(d: &mut ipc::Dec<'_>) -> std::result::Result<Self, ipc::WireError> {
        Ok(Self::of(u8::get(d)?))
    }
}

/// A message on a queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QMsg {
    /// Its key on the queue.
    pub key: u32,
    /// For an inquiry, its reply's key; for a reply, the inquiry's.
    pub reply_key: u32,
    /// Type.
    pub kind: MsgKind,
    /// Severity.
    pub severity: u8,
    /// Where it stands.
    pub status: Status,
    /// When it was sent, microseconds since the epoch.
    pub time: u64,
    /// Identifier, blank for an immediate message.
    pub msgid: String,
    /// Message file and library.
    pub msgf: (String, String),
    /// The sending job: name, user, number.
    pub job: (String, String, u32),
    /// The sending program.
    pub from_pgm: String,
    /// Where the reply goes: a queue and its library.
    pub reply_to: (String, String),
    /// Message data.
    pub data: Vec<u8>,
    /// First-level text.
    pub text: String,
    /// Second-level text.
    pub help: String,
    /// The reply, once answered.
    pub reply: String,
    /// The default reply, for an inquiry.
    pub dft: String,
}

wire_struct!(QMsg {
    key,
    reply_key,
    kind,
    severity,
    status,
    time,
    msgid,
    msgf,
    job,
    from_pgm,
    reply_to,
    data,
    text,
    help,
    reply,
    dft
});

impl QMsg {
    /// About how many bytes it takes.
    #[must_use]
    pub fn size(&self) -> usize {
        ENTRY_LAYOUT.size.unwrap_or(0)
            + self.data.len()
            + self.text.len()
            + self.help.len()
            + self.reply.len()
            + self.dft.len()
    }
}

/// `MSGQFULL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Full {
    /// `*SNDMSG`: the send fails.
    #[default]
    NoWrap,
    /// `*WRAP`: the oldest messages that need no reply go.
    Wrap,
}

/// A queue's attributes and messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgQ {
    /// `DLVRY`: `*HOLD`, `*BREAK`, `*NOTIFY`, `*DFT`.
    pub delivery: String,
    /// `SEV`: the lowest severity break and notify deliver.
    pub severity: u8,
    /// `BRKPGM`: name and library, `*DSPMSG` for the system's.
    pub brkpgm: (String, String),
    /// `SIZE`: initial, increment and maximum in kilobytes; maximum 0
    /// is `*NOMAX`.
    pub size: (u32, u32, u32),
    /// `MSGQFULL`.
    pub full: Full,
    /// `TEXT`.
    pub text: String,
    /// The next key handed out.
    pub next_key: u32,
    /// Messages, oldest first.
    pub msgs: Vec<QMsg>,
}

impl Default for MsgQ {
    /// `CRTMSGQ`'s defaults.
    fn default() -> Self {
        Self {
            delivery: "*HOLD".into(),
            severity: 0,
            brkpgm: ("*DSPMSG".into(), String::new()),
            size: (3, 1, 0),
            full: Full::NoWrap,
            text: String::new(),
            next_key: 1,
            msgs: Vec::new(),
        }
    }
}

/// The bytes a text field takes, CP037.
fn put_text(out: &mut Vec<u8>, s: &str) -> usize {
    let b = text_bytes(s, s.chars().count());
    out.extend_from_slice(&b);
    b.len()
}

fn len16(n: usize) -> u64 {
    u64::from(u16::try_from(n).unwrap_or(u16::MAX))
}

impl MsgQ {
    /// The payload.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // record, entry, lengths
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("Severity", u64::from(self.severity));
        r.set_text("Delivery", &self.delivery);
        r.set_text(
            "Full",
            match self.full {
                Full::NoWrap => "*SNDMSG",
                Full::Wrap => "*WRAP",
            },
        );
        r.set_text("BrkPgmName", &self.brkpgm.0);
        r.set_text("BrkPgmLib", &self.brkpgm.1);
        r.set_uint("SizeInitial", u64::from(self.size.0));
        r.set_uint("SizeIncrement", u64::from(self.size.1));
        r.set_uint("SizeMax", u64::from(self.size.2));
        r.set_uint("NextKey", u64::from(self.next_key));
        r.set_uint("Count", self.msgs.len() as u64);
        r.set_text("Text", &self.text);
        let mut out = r.buf;
        for m in &self.msgs {
            let mut e = Rec::new(&ENTRY_LAYOUT);
            e.set_uint("Key", u64::from(m.key));
            e.set_uint("ReplyKey", u64::from(m.reply_key));
            e.set_uint("Type", u64::from(m.kind.code()));
            e.set_uint("Severity", u64::from(m.severity));
            e.set_uint("Status", u64::from(m.status.code()));
            e.set_uint("Time", m.time);
            e.set_text("MsgId", &m.msgid);
            e.set_text("MsgfName", &m.msgf.0);
            e.set_text("MsgfLib", &m.msgf.1);
            e.set_text("JobName", &m.job.0);
            e.set_text("JobUser", &m.job.1);
            e.set_uint("JobNumber", u64::from(m.job.2));
            e.set_text("FromPgm", &m.from_pgm);
            e.set_text("ReplyTo", &m.reply_to.0);
            e.set_text("ReplyToLib", &m.reply_to.1);
            let mut tail = Vec::new();
            tail.extend_from_slice(&m.data);
            let t = put_text(&mut tail, &m.text);
            let h = put_text(&mut tail, &m.help);
            let rp = put_text(&mut tail, &m.reply);
            let d = put_text(&mut tail, &m.dft);
            e.set_uint("DataLen", len16(m.data.len()));
            e.set_uint("TextLen", len16(t));
            e.set_uint("HelpLen", len16(h));
            e.set_uint("ReplyLen", len16(rp));
            e.set_uint("DftLen", len16(d));
            out.extend(e.buf);
            out.extend(tail);
        }
        out
    }

    /// Read a payload back.
    #[allow(clippy::many_single_char_names)] // record, entry, lengths
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a message queue".into()));
        }
        let small = |n: &str| u32::try_from(r.uint(n)).unwrap_or(u32::MAX);
        let mut q = Self {
            delivery: r.text("Delivery"),
            severity: u8::try_from(r.uint("Severity")).unwrap_or(99),
            brkpgm: (r.text("BrkPgmName"), r.text("BrkPgmLib")),
            size: (
                small("SizeInitial"),
                small("SizeIncrement"),
                small("SizeMax"),
            ),
            full: if r.text("Full") == "*WRAP" {
                Full::Wrap
            } else {
                Full::NoWrap
            },
            text: r.text("Text"),
            next_key: small("NextKey"),
            msgs: Vec::new(),
        };
        let mut rest = r.tail();
        for _ in 0..r.uint("Count") {
            let e = Rec::read(&ENTRY_LAYOUT, rest)?;
            let n = |f: &str| usize::try_from(e.uint(f)).unwrap_or(usize::MAX);
            let (dl, tl, hl, rl, fl) = (
                n("DataLen"),
                n("TextLen"),
                n("HelpLen"),
                n("ReplyLen"),
                n("DftLen"),
            );
            let tail = e.tail();
            let total = dl + tl + hl + rl + fl;
            if tail.len() < total {
                return Err(Error::Corrupt("message entry truncated".into()));
            }
            let mut at = 0;
            let mut take = |k: usize| {
                let s = &tail[at..at + k];
                at += k;
                s
            };
            let data = take(dl).to_vec();
            let text = text_of(take(tl));
            let help = text_of(take(hl));
            let reply = text_of(take(rl));
            let dft = text_of(take(fl));
            q.msgs.push(QMsg {
                key: u32::try_from(e.uint("Key")).unwrap_or(0),
                reply_key: u32::try_from(e.uint("ReplyKey")).unwrap_or(0),
                kind: MsgKind::of(u8::try_from(e.uint("Type")).unwrap_or(0)),
                severity: u8::try_from(e.uint("Severity")).unwrap_or(0),
                status: Status::of(u8::try_from(e.uint("Status")).unwrap_or(0)),
                time: e.uint("Time"),
                msgid: e.text("MsgId"),
                msgf: (e.text("MsgfName"), e.text("MsgfLib")),
                job: (
                    e.text("JobName"),
                    e.text("JobUser"),
                    u32::try_from(e.uint("JobNumber")).unwrap_or(0),
                ),
                from_pgm: e.text("FromPgm"),
                reply_to: (e.text("ReplyTo"), e.text("ReplyToLib")),
                data,
                text,
                help,
                reply,
                dft,
            });
            let used = ENTRY_LAYOUT.size.unwrap_or(0) + total;
            rest = &rest[used..];
        }
        Ok(q)
    }

    /// Bytes the messages take.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.msgs.iter().map(QMsg::size).sum()
    }

    /// Put a message on the queue; its key. A full queue fails with
    /// `CPF2460` under `*SNDMSG`, or loses its oldest messages that need
    /// no reply under `*WRAP`.
    pub fn send(&mut self, mut m: QMsg) -> Result<u32> {
        let max = usize::try_from(self.size.2).unwrap_or(0) * 1024;
        if max > 0 && self.bytes() + m.size() > max {
            if self.full == Full::NoWrap {
                return Err(Error::value("CPF2460", "the message queue is full"));
            }
            while self.bytes() + m.size() > max {
                let Some(i) = self
                    .msgs
                    .iter()
                    .position(|x| x.status != Status::Unanswered)
                else {
                    return Err(Error::value("CPF2460", "the message queue is full"));
                };
                self.msgs.remove(i);
            }
        }
        m.key = self.next_key;
        self.next_key = self.next_key.wrapping_add(1).max(1);
        if m.kind == MsgKind::Inq {
            m.status = Status::Unanswered;
        }
        let key = m.key;
        self.msgs.push(m);
        Ok(key)
    }

    /// Answer the inquiry `key`: the reply goes on the queue after it;
    /// the reply's key. `CPF2422` when it is not an inquiry waiting.
    pub fn reply(
        &mut self,
        key: u32,
        reply: &str,
        from: (String, String, u32),
        time: u64,
    ) -> Result<u32> {
        let i = self
            .msgs
            .iter()
            .position(|m| m.key == key && m.kind == MsgKind::Inq)
            .ok_or_else(|| {
                Error::value("CPF2422", format!("message {key:08X} is not an inquiry"))
            })?;
        if self.msgs[i].status != Status::Unanswered {
            return Err(Error::value(
                "CPF2422",
                format!("message {key:08X} has its reply already"),
            ));
        }
        let reply = if reply.trim().is_empty() {
            self.msgs[i].dft.clone()
        } else {
            reply.trim().to_string()
        };
        let rkey = self.next_key;
        self.next_key = self.next_key.wrapping_add(1).max(1);
        let m = &mut self.msgs[i];
        m.status = Status::Answered;
        m.reply.clone_from(&reply);
        m.reply_key = rkey;
        let rpy = QMsg {
            key: rkey,
            reply_key: key,
            kind: MsgKind::Rpy,
            status: Status::New,
            time,
            job: from,
            text: reply,
            ..QMsg::default()
        };
        self.msgs.insert(i + 1, rpy);
        Ok(rkey)
    }

    /// Receive: the oldest new message of a kind (any when `None`), or
    /// the keyed one; marked old, or removed when `remove`.
    pub fn receive(
        &mut self,
        kind: Option<MsgKind>,
        key: Option<u32>,
        remove: bool,
    ) -> Option<QMsg> {
        let i = self.msgs.iter().position(|m| match key {
            Some(k) => m.key == k,
            None => m.status == Status::New && kind.is_none_or(|k| m.kind == k),
        })?;
        if remove {
            return Some(self.msgs.remove(i));
        }
        let m = &mut self.msgs[i];
        if m.status == Status::New {
            m.status = Status::Old;
        }
        Some(m.clone())
    }

    /// Remove by key; `CPF2410` when there is no such message.
    pub fn remove(&mut self, key: u32) -> Result<()> {
        let before = self.msgs.len();
        self.msgs.retain(|m| m.key != key);
        if before == self.msgs.len() {
            return Err(Error::value(
                "CPF2410",
                format!("message key {key:08X} not found"),
            ));
        }
        Ok(())
    }

    /// `CLRMSGQ CLEAR(*ALL|*KEEPUNANS)`, `RMVMSG CLEAR(*OLD|*NEW)`; how
    /// many went.
    pub fn clear(&mut self, which: &str) -> usize {
        let before = self.msgs.len();
        self.msgs.retain(|m| match which {
            "*KEEPUNANS" => m.status == Status::Unanswered,
            "*OLD" => m.status == Status::New || m.status == Status::Unanswered,
            "*NEW" => m.status != Status::New,
            _ => false,
        });
        before - self.msgs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(text: &str) -> QMsg {
        QMsg {
            kind: MsgKind::Info,
            text: text.into(),
            job: ("QSVCJOB".into(), "QSYS".into(), 3),
            ..QMsg::default()
        }
    }

    #[test]
    fn round_trip() {
        let mut q = MsgQ {
            delivery: "*BREAK".into(),
            text: "Operator".into(),
            ..MsgQ::default()
        };
        q.send(info("Hello")).unwrap();
        let k = q
            .send(QMsg {
                kind: MsgKind::Inq,
                msgid: "CEE9901".into(),
                msgf: ("QCEEMSG".into(), "QSYS".into()),
                data: vec![1, 2, 3],
                text: "Reply C or G.".into(),
                help: "Help.".into(),
                dft: "C".into(),
                ..info("")
            })
            .unwrap();
        q.reply(k, "", ("QSECOFR".into(), "QSECOFR".into(), 9), 5)
            .unwrap();
        let back = MsgQ::decode(&q.encode()).unwrap();
        assert_eq!(back, q);
        assert_eq!(back.msgs[1].reply, "C");
        assert_eq!(back.msgs[2].kind, MsgKind::Rpy);
        assert!(q.reply(k, "G", ("A".into(), "B".into(), 1), 6).is_err());
    }

    #[test]
    fn receive_remove_full() {
        let mut q = MsgQ {
            size: (1, 1, 1),
            ..MsgQ::default()
        };
        let a = q.send(info(&"a".repeat(300))).unwrap();
        q.send(info(&"b".repeat(300))).unwrap();
        assert!(q.send(info(&"c".repeat(600))).is_err());
        q.full = Full::Wrap;
        q.send(info(&"c".repeat(600))).unwrap();
        assert!(q.msgs.iter().all(|m| m.key != a));
        let got = q.receive(None, None, false).unwrap();
        assert_eq!(got.status, Status::Old);
        assert_eq!(q.clear("*OLD"), 1);
        assert!(q.remove(999).is_err());
    }
}

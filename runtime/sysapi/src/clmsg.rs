// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CL compiler's message programs, for what the `sndpgmmsg` and
//! `rcvmsg` instructions do not reach: `QCLSNDM` sends `SNDPGMMSG`'s
//! message to named message queues (`TOMSGQ`, `TOUSR`), `QCLRCVM`
//! receives as `RCVMSG` does from a call stack entry's queue or a named
//! queue (`MSGQ`), waiting (`WAIT`) and filling every return variable
//! (`RTNTYPE`, `MSGLEN`, `SECLVL` ...). An inquiry sent to a named
//! queue with `RPYMSGQ(*PGMQ)` is answered on the program's queue: its
//! key there stands for the inquiry, and its reply is received by that
//! key as IBM's is.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use mi::{Job, Message, QueueRef};
use qir::MsgType;

use crate::param::{Args, type_code, type_named};
use crate::provider::{ApiFail, Provider, QueueMsg};

/// The programs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[("QCLSNDM", qclsndm), ("QCLRCVM", qclrcvm)];

/// Keys standing for inquiries sent to named queues: above every key
/// the job hands out itself.
const INQ_KEYS: u32 = 0x8000_0000;

/// How often a named queue is asked again while a receive waits.
const POLL: Duration = Duration::from_millis(250);

thread_local! {
    /// Inquiries sent to named queues whose replies come to the
    /// program's queue: the key handed out, the queue and the key there.
    static INQUIRIES: RefCell<HashMap<u32, (String, String, u32)>> = RefCell::new(HashMap::new());
}

/// An escape to the CL program.
fn escape(j: &mut Job, f: &ApiFail) {
    let _ = j.send_message(
        &f.msgid,
        crate::param::file_of(&f.msgid),
        "QSYS",
        f.data.clone(),
        MsgType::Escape,
        QueueRef::Same,
    );
}

/// The `*MSGQ` type name of a message type.
fn kind_name(t: MsgType) -> &'static str {
    match t {
        MsgType::Inq => "*INQ",
        MsgType::Rpy => "*RPY",
        MsgType::Comp => "*COMP",
        MsgType::Diag => "*DIAG",
        MsgType::Notify => "*NOTIFY",
        MsgType::Escape => "*ESCAPE",
        _ => "*INFO",
    }
}

/// `RTNTYPE` of a named queue's message.
fn kind_code(kind: &str, msgid: &str) -> &'static str {
    match kind {
        "*COMP" => "01",
        "*DIAG" => "02",
        "*INQ" => "05",
        "*COPY" => "06",
        "*RPY" if msgid.is_empty() => "21",
        "*RPY" => "22",
        "*NOTIFY" => "14",
        "*ESCAPE" => "15",
        _ => "04",
    }
}

/// `QCLSNDM(msgid, msgf, data, data length, type, queues, count,
/// reply queue, key)`: the message to each `(name, library)` of
/// `queues`; the key of the last, or for an inquiry answered on the
/// program's queue the key that stands for it.
fn qclsndm(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let msgid = a.char(0, 7);
    let (msgf, msgf_lib) = a.qual(1);
    let n = usize::try_from(a.packed(3, 15, 0)).unwrap_or(0).min(3000);
    let data = a.bytes(2, n);
    let ty = a.char(4, 10);
    let kind = type_named(&ty).map_or("*INFO", kind_name);
    let (text, help, severity) = j.describe_message(&msgid, &msgf, &msgf_lib, &data);
    let count = usize::try_from(a.packed(6, 15, 0)).unwrap_or(0).clamp(1, 50);
    let queues = a.bytes(5, 20 * count);
    let (rpy, _) = a.qual(7);
    let mut last = 0;
    for q in queues.chunks(20) {
        let name = ccsid::decode_trimmed(&q[..10]);
        let lib = ccsid::decode_trimmed(&q[10..]);
        let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
        let m = QueueMsg {
            kind: kind.into(),
            severity: u8::try_from(severity).unwrap_or(99),
            msgid: msgid.clone(),
            msgf: (msgf.clone(), msgf_lib.clone()),
            data: data.clone(),
            text: text.clone(),
            help: help.clone(),
            ..QueueMsg::default()
        };
        match p.msgq_send(&lib, &name, m) {
            Ok(k) if kind == "*INQ" && (rpy.is_empty() || rpy == "*PGMQ") => {
                last = INQUIRIES.with(|m| {
                    let mut m = m.borrow_mut();
                    let key = INQ_KEYS + u32::try_from(m.len()).unwrap_or(0) + 1;
                    m.insert(key, (lib.clone(), name.clone(), k));
                    key
                });
            }
            Ok(k) => last = k,
            Err(f) => {
                escape(j, &f);
                return;
            }
        }
    }
    a.put_bytes(8, &last.to_be_bytes());
}

/// What a receive found, in `RCVMSG`'s terms.
#[derive(Default)]
struct Got {
    rtntype: String,
    msgid: String,
    severity: i64,
    key: u32,
    msgf: (String, String),
    text: String,
    help: String,
    data: Vec<u8>,
}

impl Got {
    fn of_job(m: &Message) -> Self {
        Self {
            rtntype: type_code(m.msg_type).into(),
            msgid: m.msgid.clone(),
            severity: i64::from(m.severity),
            key: m.key,
            msgf: (m.msgf.clone(), m.msgf_lib.clone()),
            text: m.text.clone(),
            help: m.help.clone(),
            data: m.data.clone(),
        }
    }

    fn of_queue(m: &QueueMsg) -> Self {
        Self {
            rtntype: kind_code(&m.kind, &m.msgid).into(),
            msgid: m.msgid.clone(),
            severity: i64::from(m.severity),
            key: m.key,
            msgf: m.msgf.clone(),
            text: m.text.clone(),
            help: m.help.clone(),
            data: m.data.clone(),
        }
    }
}

/// One look at a call stack entry's queue: `MSGTYPE` and `MSGKEY` as
/// `RCVMSG` takes them.
fn look_job(j: &mut Job, q: QueueRef, ty: &str, key: Option<u32>, rmv: bool) -> Result<Option<Got>, ApiFail> {
    let sel = match (ty, key) {
        ("*NEXT" | "*PRV", _) => {
            let Some(queue) = j.queue(q) else {
                return Ok(None);
            };
            let i = key.and_then(|k| queue.msgs.iter().position(|m| m.key == k));
            let m = match (ty, i) {
                ("*NEXT", Some(i)) => queue.msgs.get(i + 1),
                ("*NEXT", None) => queue.msgs.first(),
                ("*PRV", Some(i)) if i > 0 => queue.msgs.get(i - 1),
                ("*PRV", None) => queue.msgs.last(),
                _ => None,
            };
            match m {
                Some(m) => (None, Some(m.key)),
                None => return Ok(None),
            }
        }
        ("*FIRST" | "*LAST", _) => {
            let Some(queue) = j.queue(q) else {
                return Ok(None);
            };
            let m = if ty == "*FIRST" { queue.msgs.first() } else { queue.msgs.last() };
            match m {
                Some(m) => (None, Some(m.key)),
                None => return Ok(None),
            }
        }
        ("*EXCP", _) => (Some(MsgType::Escape), key),
        _ => (Some(type_named(ty).ok_or_else(|| ApiFail::id("CPF24B3").text(ty, 10))?), key),
    };
    Ok(j.receive_message(q, sel.0, sel.1, rmv).as_ref().map(Got::of_job))
}

/// The reply to an inquiry sent to a named queue, once it has one.
fn look_reply(p: &mut dyn Provider, key: u32, rmv: bool) -> Result<Option<Got>, ApiFail> {
    let Some((lib, name, k)) = INQUIRIES.with(|m| m.borrow().get(&key).cloned()) else {
        return Ok(None);
    };
    let msgs = p.msgq_list(&lib, &name)?;
    let Some(inq) = msgs.into_iter().find(|m| m.key == k && !m.reply.is_empty()) else {
        return Ok(None);
    };
    if rmv {
        INQUIRIES.with(|m| m.borrow_mut().remove(&key));
    }
    let text = inq.reply.clone();
    Ok(Some(Got {
        rtntype: kind_code("*RPY", &inq.msgid).into(),
        key,
        data: ccsid::to_ebcdic(&text).unwrap_or_default(),
        text,
        ..Got::default()
    }))
}

/// One look at a named queue.
fn look_queue(p: &mut dyn Provider, lib: &str, name: &str, ty: &str, key: Option<u32>, rmv: bool) -> Result<Option<Got>, ApiFail> {
    let kind = match ty {
        "*ANY" | "*NEXT" | "*FIRST" | "*LAST" | "*PRV" => None,
        "*EXCP" => Some("*ESCAPE"),
        t => Some(t),
    };
    let key = match (ty, key) {
        ("*NEXT" | "*PRV" | "*FIRST" | "*LAST", k) => {
            let msgs = p.msgq_list(lib, name)?;
            let i = k.and_then(|k| msgs.iter().position(|m| m.key == k));
            let m = match (ty, i) {
                ("*NEXT", Some(i)) => msgs.get(i + 1),
                ("*NEXT" | "*FIRST", None) | ("*FIRST", Some(_)) => msgs.first(),
                ("*PRV", Some(i)) if i > 0 => msgs.get(i - 1),
                ("*PRV" | "*LAST", None) | ("*LAST", Some(_)) => msgs.last(),
                _ => None,
            };
            match m {
                Some(m) => Some(m.key),
                None => return Ok(None),
            }
        }
        (_, k) => k,
    };
    Ok(p.msgq_receive(lib, name, kind, key, rmv)?.as_ref().map(Got::of_queue))
}

/// Store a character field, blank padded.
fn put_text(a: &Args<'_>, i: usize, n: usize, b: &[u8]) {
    if let Some(out) = a.raw(i, n) {
        let k = b.len().min(n);
        out[..k].copy_from_slice(&b[..k]);
        out[k..].fill(ccsid::SPACE);
    }
}

/// `QCLRCVM(relationship, queue, type, key, wait, remove, found,
/// rtntype, msgid, sev, keyvar, msgf, msgflib, msglen, seclvllen,
/// msgdtalen, msg, seclvl, msgdta)`: a relationship of `*MSGQ` receives
/// from `queue`, else from that call stack entry's queue. Nothing found
/// leaves blanks and zeros, as IBM's `RCVMSG` does.
fn qclrcvm(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let rel = a.char(0, 10);
    let (qname, qlib) = a.qual(1);
    let qlib = if qlib.is_empty() { "*LIBL".to_string() } else { qlib };
    let ty = a.char(2, 10);
    let ty = if ty.is_empty() { "*ANY".to_string() } else { ty };
    let key = a.raw(3, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let key = key.filter(|k| *k != 0x4040_4040);
    let wait = a.packed(4, 15, 0);
    let rmv = a.char(5, 10);
    let rmv = rmv != "*NO";
    let started = Instant::now();
    let got = loop {
        let r = if rel == "*MSGQ" {
            look_queue(p, &qlib, &qname, &ty, key, rmv)
        } else if key.is_some_and(|k| k >= INQ_KEYS) && ty == "*RPY" {
            look_reply(p, key.unwrap_or(0), rmv)
        } else {
            let q = match rel.as_str() {
                "*PRV" => QueueRef::Prv,
                "*EXT" => QueueRef::Ext,
                "*CTLBDY" => QueueRef::CtlBdy,
                _ => QueueRef::Same,
            };
            look_job(j, q, &ty, key, rmv)
        };
        match r {
            Ok(Some(g)) => break Some(g),
            Ok(None) => {}
            Err(f) => {
                escape(j, &f);
                return;
            }
        }
        let waited = started.elapsed();
        let limit = u64::try_from(wait).map(Duration::from_secs);
        match limit {
            // `*MAX` (-1) waits as long as it takes.
            Err(_) => std::thread::sleep(POLL),
            Ok(l) if waited < l => std::thread::sleep(POLL.min(l.saturating_sub(waited))),
            Ok(_) => break None,
        }
    };
    let g = got.unwrap_or_default();
    let text = ccsid::to_ebcdic(&g.text).unwrap_or_default();
    let help = ccsid::to_ebcdic(&g.help).unwrap_or_default();
    let len = |n: usize| i64::try_from(n).unwrap_or(0);
    put_text(&a, 6, 1, if g.rtntype.is_empty() { b"\xF0" } else { b"\xF1" });
    a.put_char(7, 2, &g.rtntype);
    a.put_char(8, 7, &g.msgid);
    a.put_packed(9, 15, 0, g.severity);
    a.put_bytes(10, &if g.rtntype.is_empty() { [ccsid::SPACE; 4] } else { g.key.to_be_bytes() });
    a.put_char(11, 10, &g.msgf.0);
    a.put_char(12, 10, &g.msgf.1);
    a.put_packed(13, 15, 0, len(text.len()));
    a.put_packed(14, 15, 0, len(help.len()));
    a.put_packed(15, 15, 0, len(g.data.len()));
    put_text(&a, 16, 3000, &text);
    put_text(&a, 17, 3000, &help);
    put_text(&a, 18, 3000, &g.data);
}

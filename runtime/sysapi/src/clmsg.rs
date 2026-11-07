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

use crate::param::{Args, entry_queue, type_code, type_named};
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
    let count = usize::try_from(a.packed(6, 15, 0))
        .unwrap_or(0)
        .clamp(1, 50);
    let queues = a.bytes(5, 20 * count);
    let (rpy, _) = a.qual(7);
    let mut last = 0;
    let mut targets = Vec::new();
    for q in queues.chunks(20) {
        let name = ccsid::decode_trimmed(&q[..10]);
        let lib = ccsid::decode_trimmed(&q[10..]);
        match name.as_str() {
            // TOUSR(*REQUESTER): an interactive job's own external
            // queue; a batch job has no requester but the operator.
            "*REQUESTER" if j.display.is_some() => {
                let kind = type_named(&ty).unwrap_or(MsgType::Info);
                match j.send_message(&msgid, &msgf, &msgf_lib, data.clone(), kind, QueueRef::Ext) {
                    Ok(k) => last = k,
                    Err(c) => {
                        escape(j, &ApiFail::id(&c.msgid()));
                        return;
                    }
                }
            }
            "*REQUESTER" => targets.push(("QSYS".to_string(), "QSYSOPR".to_string())),
            // TOUSR(*ALLACT): each signed-on user's queue.
            "*ALLACT" => match p.signed_on_users() {
                Ok(users) => targets.extend(users.into_iter().map(|u| ("QUSRSYS".to_string(), u))),
                Err(f) => {
                    escape(j, &f);
                    return;
                }
            },
            _ => targets.push((
                if lib.is_empty() {
                    "*LIBL".to_string()
                } else {
                    lib
                },
                name,
            )),
        }
    }
    for (lib, name) in targets {
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
    /// The sending job, for a named queue's message: name, user, number.
    job: Option<(String, String, u32)>,
    from_pgm: String,
    sent_us: u64,
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
            job: None,
            from_pgm: m.from_pgm.clone(),
            sent_us: m.sent_us,
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
            job: Some(m.job.clone()),
            from_pgm: m.from_pgm.clone(),
            sent_us: m.sent_us,
        }
    }

    /// `ALROPT`, `TXTCCSID` and `DTACCSID` (the primary, V7R1M0): a
    /// message from a message file `*NO`, 65535 and 65535 (QS/400 keeps
    /// no alert options); an immediate one blank, the job's 37 and 0.
    fn ccsids(&self) -> (&'static str, i64, i64) {
        if self.msgid.is_empty() {
            ("", 37, 0)
        } else {
            ("*NO", 65535, 65535)
        }
    }

    /// `SENDER` as the primary fills it for a program message
    /// (`tools/conformance/oracle/cl/rcvmsg/rcvmsg.txt`): 1-26 the
    /// sending job (blank for one in this job), 27-38 the sending
    /// program, 39-42 its instruction, 43-49 the date `CYYMMDD`, 50-55
    /// the time, 56-65 the receiving program, 66-69 its instruction, 70
    /// and 71 the sending and receiving types (`1` ILE), 72-77 the
    /// microseconds, 78-87 the sending user; the rest blank.
    fn sender(&self, receiver: &str, user: &str, offset_min: i32) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut s = String::with_capacity(720);
        match &self.job {
            Some((name, user, number)) => {
                let _ = write!(s, "{name:<10.10}{user:<10.10}{number:06}");
            }
            None => s.push_str(&" ".repeat(26)),
        }
        let secs = self.sent_us / 1_000_000;
        let local = u64::try_from(i64::try_from(secs).unwrap_or(0) + i64::from(offset_min) * 60)
            .unwrap_or(0);
        let (y, mo, d, h, mi, se) = crate::job::civil(local);
        let c = i32::from(y >= 2000);
        let from_user = self.job.as_ref().map_or(user, |j| j.1.as_str());
        let _ = write!(
            s,
            "{:<12.12}0000{c}{:02}{mo:02}{d:02}{h:02}{mi:02}{se:02}{receiver:<10.10}0000{}1{:06}{from_user:<10.10}",
            self.from_pgm,
            y % 100,
            if self.from_pgm.is_empty() { " " } else { "1" },
            self.sent_us % 1_000_000
        );
        let mut b = ccsid::to_ebcdic(&s).unwrap_or_default();
        b.resize(720, ccsid::SPACE);
        b
    }
}

/// One look at a call stack entry's queue: `MSGTYPE` and `MSGKEY` as
/// `RCVMSG` takes them.
fn look_job(
    j: &mut Job,
    q: QueueRef,
    ty: &str,
    key: Option<u32>,
    rmv: bool,
) -> Result<Option<Got>, ApiFail> {
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
            let m = if ty == "*FIRST" {
                queue.msgs.first()
            } else {
                queue.msgs.last()
            };
            match m {
                Some(m) => (None, Some(m.key)),
                None => return Ok(None),
            }
        }
        ("*EXCP", _) => (Some(MsgType::Escape), key),
        _ => (
            Some(type_named(ty).ok_or_else(|| ApiFail::id("CPF24B3").text(ty, 10))?),
            key,
        ),
    };
    Ok(j.receive_message(q, sel.0, sel.1, rmv)
        .as_ref()
        .map(Got::of_job))
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
fn look_queue(
    p: &mut dyn Provider,
    lib: &str,
    name: &str,
    ty: &str,
    key: Option<u32>,
    rmv: bool,
) -> Result<Option<Got>, ApiFail> {
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
    Ok(p.msgq_receive(lib, name, kind, key, rmv)?
        .as_ref()
        .map(Got::of_queue))
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
/// msgdtalen, msg, seclvl, msgdta, alropt, txtccsid, dtaccsid, sender,
/// entry)`: a relationship of `*MSGQ` receives from `queue`, else from
/// the queue of the call stack entry `entry` names (name, module, bound
/// program; blank for the program itself) or its caller's. Nothing found
/// leaves blanks and zeros, as IBM's `RCVMSG` does.
#[allow(clippy::too_many_lines)]
fn qclrcvm(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let rel = a.char(0, 10);
    let (qname, qlib) = a.qual(1);
    let qlib = if qlib.is_empty() {
        "*LIBL".to_string()
    } else {
        qlib
    };
    let ty = a.char(2, 10);
    let ty = if ty.is_empty() {
        "*ANY".to_string()
    } else {
        ty
    };
    let key = a
        .raw(3, 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let key = key.filter(|k| *k != 0x4040_4040);
    let wait = a.packed(4, 15, 0);
    let rmv = a.char(5, 10);
    let rmv = rmv != "*NO";
    // PGMQ's call stack entry: name, module, bound program.
    let spec = a.bytes(23, 30);
    let part = |i: usize| {
        spec.get(i * 10..i * 10 + 10)
            .map(ccsid::decode_trimmed)
            .unwrap_or_default()
    };
    let (e, module, bndpgm) = (part(0), part(1), part(2));
    let entry = (!e.is_empty() && e != "*").then_some(e);
    let started = Instant::now();
    let got = loop {
        let r = if rel == "*MSGQ" {
            look_queue(p, &qlib, &qname, &ty, key, rmv)
        } else if key.is_some_and(|k| k >= INQ_KEYS) && ty == "*RPY" {
            look_reply(p, key.unwrap_or(0), rmv)
        } else {
            let q = match (rel.as_str(), entry.as_deref()) {
                ("*EXT", _) => QueueRef::Ext,
                ("*CTLBDY", _) => QueueRef::CtlBdy,
                (_, Some(e)) => match entry_of(j, e, &module, &bndpgm, rel == "*PRV") {
                    Some(q) => q,
                    None => {
                        escape(j, &ApiFail::id("CPF2479"));
                        return;
                    }
                },
                ("*PRV", None) => QueueRef::Prv,
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
    put_text(
        &a,
        6,
        1,
        if g.rtntype.is_empty() {
            b"\xF0"
        } else {
            b"\xF1"
        },
    );
    a.put_char(7, 2, &g.rtntype);
    a.put_char(8, 7, &g.msgid);
    a.put_packed(9, 15, 0, g.severity);
    a.put_bytes(
        10,
        &if g.rtntype.is_empty() {
            [ccsid::SPACE; 4]
        } else {
            g.key.to_be_bytes()
        },
    );
    a.put_char(11, 10, &g.msgf.0);
    a.put_char(12, 10, &g.msgf.1);
    a.put_packed(13, 15, 0, len(text.len()));
    a.put_packed(14, 15, 0, len(help.len()));
    a.put_packed(15, 15, 0, len(g.data.len()));
    put_text(&a, 16, 3000, &text);
    put_text(&a, 17, 3000, &help);
    put_text(&a, 18, 3000, &g.data);
    // ALROPT, TXTCCSID, DTACCSID and SENDER: blanks and zeros when
    // nothing came.
    let found = !g.rtntype.is_empty();
    let (alropt, txt, dta) = if found { g.ccsids() } else { ("", 0, 0) };
    a.put_char(19, 9, alropt);
    a.put_packed(20, 15, 0, txt);
    a.put_packed(21, 15, 0, dta);
    if found {
        let receiver = j
            .act
            .frames
            .last()
            .and_then(|f| f.program.as_ref())
            .map(|p| p.object.name.clone())
            .unwrap_or_default();
        let user = j.ident.user.clone();
        let offset = p.now().offset_min;
        put_text(&a, 22, 720, &g.sender(&receiver, &user, offset));
    } else {
        put_text(&a, 22, 720, &[]);
    }
}

/// The queue of call stack entry `name` (a program or procedure, `*PGMBDY`,
/// `*CTLBDY`, or `*PGMNAME` with `bndpgm`), limited to `module` and
/// `bndpgm` when they are given; its caller's with `prv`.
fn entry_of(j: &Job, name: &str, module: &str, bndpgm: &str, prv: bool) -> Option<QueueRef> {
    let given = |s: &str| !s.is_empty() && s != "*NONE";
    let counter = i32::from(prv);
    if !given(module) && !given(bndpgm) {
        return entry_queue(j, name, counter);
    }
    let frames = &j.act.frames;
    let top = frames.len();
    let base = frames.iter().rposition(|f| {
        f.program.as_ref().is_some_and(|p| {
            let in_pgm = !given(bndpgm) || p.object.name == bndpgm;
            let in_mod = !given(module)
                || p.proc_of(f.code)
                    .and_then(|(m, _)| p.object.modules.get(usize::try_from(m).ok()?))
                    .is_some_and(|m| m.module.name == module);
            let named = name == "*PGMNAME"
                || p.proc_name(f.code).as_deref() == Some(name)
                || p.object.name == name;
            in_pgm && in_mod && named
        })
    })?;
    let up = usize::try_from(counter).ok()?;
    let target = base.checked_sub(up)?;
    Some(QueueRef::Up(u32::try_from(top - 1 - target).ok()?))
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message queue commands of the service console: create, change,
//! delete, clear, send, reply, receive, remove and display.

use std::fmt::Write as _;

use store::obj::{JobHandle, ops, types};
use wm::cmd::Params;
use wm::obj::msgq::{Full, MsgKind, MsgQ, Status};

use crate::ipl::system::{System, now_unix};
use crate::stgsvc::msgs::{self, queue_name, with_queue};

/// Help for these commands.
pub const HELP: &str = "\
crtmsgq LIB/Q [DLVRY(*HOLD|*BREAK|*NOTIFY) SEV() SIZE(i n m) MSGQFULL(*SNDMSG|*WRAP) TEXT()]
chgmsgq Q [DLVRY() SEV() BRKPGM() TEXT()] | dltmsgq Q | clrmsgq Q [CLEAR(*ALL|*KEEPUNANS)]
sndmsg MSG('..') TOMSGQ(Q|*SYSOPR) [MSGTYPE(*INFO|*INQ) RPYMSGQ(Q)] | sndbrkmsg MSG('..') TOMSGQ(Q)
sndrpymsg MSGKEY(hex) MSGQ(Q) RPY('..') | rcvmsg MSGQ(Q) [MSGTYPE() MSGKEY() RMV(*NO)]
rmvmsg MSGQ(Q) MSGKEY(hex)|CLEAR(*ALL|*OLD|*NEW|*KEEPUNANS) | dspmsg [Q|*SYSOPR]
";

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "crtmsgq" => crtmsgq(system, &p),
        "chgmsgq" => chgmsgq(system, &p),
        "dltmsgq" => dltmsgq(system, &p),
        "clrmsgq" => clrmsgq(system, &p),
        "sndmsg" => sndmsg(system, &p, false),
        "sndbrkmsg" => sndmsg(system, &p, true),
        "sndrpymsg" => sndrpymsg(system, &p),
        "rcvmsg" => rcvmsg(system, &p),
        "rmvmsg" => rmvmsg(system, &p),
        "dspmsg" => dspmsg(system, &p),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

fn text_kw(p: &Params, kw: &str) -> String {
    p.kw(kw)
        .map(|t| t.trim().trim_matches('\'').replace("''", "'"))
        .unwrap_or_default()
}

/// Apply `CRTMSGQ`/`CHGMSGQ` keywords.
#[allow(clippy::many_single_char_names)] // queue, params, SIZE values
fn apply(q: &mut MsgQ, p: &Params) -> Result<(), String> {
    for (kw, v) in &p.kw {
        let up = v.trim().to_ascii_uppercase();
        match kw.as_str() {
            "MSGQ" => {}
            "DLVRY" if matches!(up.as_str(), "*HOLD" | "*BREAK" | "*NOTIFY" | "*DFT") => {
                q.delivery = up;
            }
            "SEV" => {
                q.severity = up
                    .parse()
                    .ok()
                    .filter(|s| *s <= 99)
                    .ok_or_else(|| format!("CPD0085 SEV({v}) is 0 through 99"))?;
            }
            "BRKPGM" => {
                q.brkpgm = if up.starts_with('*') {
                    (up, String::new())
                } else {
                    let (l, n) = wm::obj::params::qualified(&up);
                    (n, l)
                };
            }
            "SIZE" => {
                let w: Vec<u32> = up
                    .split_whitespace()
                    .map(|x| if x == "*NOMAX" { Ok(0) } else { x.parse() })
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("CPD0085 SIZE({v})"))?;
                let [i, n, m] = w.as_slice() else {
                    return Err(format!("CPD0085 SIZE({v}) needs three values"));
                };
                q.size = (*i, *n, *m);
            }
            "MSGQFULL" => {
                q.full = match up.as_str() {
                    "*WRAP" => Full::Wrap,
                    "*SNDMSG" => Full::NoWrap,
                    _ => return Err(format!("CPD0084 MSGQFULL({v})")),
                };
            }
            "TEXT" => q.text = text_kw(p, "TEXT"),
            _ => {
                return Err(format!(
                    "CPD0043 keyword {kw}({v}) not valid for this command"
                ));
            }
        }
    }
    Ok(())
}

/// `CRTMSGQ`.
#[allow(clippy::many_single_char_names)] // params, queue, store, library, name
fn crtmsgq(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = wm::obj::params::qualified(&p.pos_or(0, "MSGQ")?);
    let lib = if lib == "*LIBL" {
        "QGPL".to_string()
    } else {
        lib
    };
    let mut q = MsgQ::default();
    apply(&mut q, p)?;
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| {
        let l = wm::obj::library(txn, qsys, &lib).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        let stamp = wm::obj::Stamp {
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: now_unix(),
        };
        wm::obj::create(txn, l, n, types::MSGQ, &q.text, &stamp, &q.encode())
            .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    Ok(format!("Message queue {name} created in library {lib}.\n"))
}

/// `CHGMSGQ`: `DLVRY(*BREAK)` allocates the queue to this job.
fn chgmsgq(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = queue_name(&p.pos_or(0, "MSGQ")?);
    let me = system.current_job;
    let brk = with_queue(system, "msgq.change", &lib, &name, |q, _| {
        apply(q, p)?;
        Ok((q.delivery == "*BREAK", true))
    })?;
    if brk {
        msgs::allocate(system, &lib, &name, me)?;
    }
    Ok(format!("Message queue {name} changed.\n"))
}

/// `DLTMSGQ`.
fn dltmsgq(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = queue_name(&p.pos_or(0, "MSGQ")?);
    let id = with_queue(system, "obj.check", &lib, &name, |_, id| Ok((id, false)))?;
    let job = JobHandle(u64::from(system.current_job));
    let s = system.storage.as_mut().ok_or("object store not open")?;
    super::db::with_txn(&mut s.store, |txn| {
        ops::delete(txn, job, id).map_err(|e| e.to_string())
    })?;
    system.break_alloc.remove(&id);
    Ok(format!("Message queue {name} deleted.\n"))
}

/// `CLRMSGQ`.
fn clrmsgq(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = queue_name(&p.pos_or(0, "MSGQ")?);
    let which = p.upper("CLEAR").unwrap_or_else(|| "*ALL".into());
    let n = with_queue(system, "msgq.clear", &lib, &name, |q, _| Ok((q.clear(&which), true)))?;
    Ok(format!("{n} message(s) removed from {name}.\n"))
}

/// `SNDMSG` and `SNDBRKMSG`.
fn sndmsg(system: &mut System, p: &Params, brk: bool) -> Result<String, String> {
    let text = text_kw(p, "MSG");
    if text.is_empty() {
        return Err("CPD0071 MSG is needed".into());
    }
    let to = p
        .upper("TOMSGQ")
        .or_else(|| {
            p.upper("TOUSR").map(|u| match u.as_str() {
                "*SYSOPR" | "QSYSOPR" => "*SYSOPR".into(),
                u => format!("QUSRSYS/{u}"),
            })
        })
        .ok_or("CPD0071 TOMSGQ or TOUSR is needed")?;
    let kind = match p.upper("MSGTYPE").as_deref() {
        None | Some("*INFO") => MsgKind::Info,
        Some("*INQ") => MsgKind::Inq,
        Some(t) => return Err(format!("CPD0084 MSGTYPE({t})")),
    };
    let me = system.current_job;
    let mut m = msgs::immediate(system, me, kind, &text);
    if kind == MsgKind::Inq {
        m.reply_to = p.upper("RPYMSGQ").map_or_else(
            || ("QSYSOPR".into(), "QSYS".into()),
            |r| {
                let (l, n) = queue_name(&r);
                (n, l)
            },
        );
    }
    if brk {
        // Break messages reach the queue whatever its delivery.
        m.severity = 99;
    }
    let (lib, name) = queue_name(&to);
    let key = msgs::send(system, &lib, &name, m)?;
    Ok(format!("Message sent to {lib}/{name}, key {key:08X}.\n"))
}

fn key_kw(p: &Params) -> Result<u32, String> {
    let k = p.pos_or(0, "MSGKEY")?;
    u32::from_str_radix(
        k.trim_matches('\'')
            .trim_start_matches("X'")
            .trim_end_matches('\''),
        16,
    )
    .map_err(|_| format!("CPD0084 MSGKEY({k}) is hexadecimal"))
}

/// `SNDRPYMSG MSGKEY() MSGQ() RPY()`.
fn sndrpymsg(system: &mut System, p: &Params) -> Result<String, String> {
    let key = key_kw(p)?;
    let (lib, name) = queue_name(&p.upper("MSGQ").unwrap_or_else(|| "*SYSOPR".into()));
    let reply = text_kw(p, "RPY");
    let from = msgs::sender(system, system.current_job);
    let rkey = with_queue(system, "msgq.reply", &lib, &name, |q, _| {
        let k = q
            .reply(key, &reply, from, msgs::now_micros())
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2422")))?;
        Ok((k, true))
    })?;
    Ok(format!(
        "Reply sent to message {key:08X}, reply key {rkey:08X}.\n"
    ))
}

/// `RCVMSG MSGQ() [MSGTYPE() MSGKEY() RMV()]`.
fn rcvmsg(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = queue_name(&p.pos_or(0, "MSGQ")?);
    let kind = match p.upper("MSGTYPE").as_deref() {
        None | Some("*ANY" | "*NEXT" | "*FIRST") => None,
        Some(t) => Some(MsgKind::parse(t).ok_or_else(|| format!("CPD0084 MSGTYPE({t})"))?),
    };
    let key = match p.kw("MSGKEY") {
        Some(k) => {
            Some(u32::from_str_radix(k.trim(), 16).map_err(|_| format!("CPD0084 MSGKEY({k})"))?)
        }
        None => None,
    };
    let remove = !matches!(p.upper("RMV").as_deref(), Some("*NO"));
    let m = with_queue(system, if remove { "msgq.remove" } else { "msgq.receive" }, &lib, &name, |q, _| {
        let m = q.receive(kind, key, remove);
        let changed = m.is_some();
        Ok((m, changed))
    })?;
    Ok(match m {
        Some(m) => format!(
            "{:08X} {} {:02} {}\n",
            m.key,
            m.kind.name(),
            m.severity,
            m.text
        ),
        None => "No message received.\n".into(),
    })
}

/// `RMVMSG MSGQ() MSGKEY()|CLEAR()`.
fn rmvmsg(system: &mut System, p: &Params) -> Result<String, String> {
    let (lib, name) = queue_name(&p.upper("MSGQ").ok_or("CPD0071 MSGQ is needed")?);
    if let Some(which) = p.upper("CLEAR") {
        let n = with_queue(system, "msgq.clear", &lib, &name, |q, _| Ok((q.clear(&which), true)))?;
        return Ok(format!("{n} message(s) removed.\n"));
    }
    let key = u32::from_str_radix(
        p.kw("MSGKEY")
            .ok_or("CPD0071 MSGKEY or CLEAR is needed")?
            .trim(),
        16,
    )
    .map_err(|_| "CPD0084 MSGKEY is hexadecimal".to_string())?;
    with_queue(system, "msgq.remove", &lib, &name, |q, _| {
        q.remove(key)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2410")))?;
        Ok(((), true))
    })?;
    Ok(format!("Message {key:08X} removed.\n"))
}

/// `DSPMSG`: the queue's attributes, then each message with its key,
/// sender and time; an inquiry shows its reply or that it waits.
fn dspmsg(system: &mut System, p: &Params) -> Result<String, String> {
    let which = p
        .pos
        .first()
        .cloned()
        .or_else(|| p.upper("MSGQ"))
        .unwrap_or_else(|| "*SYSOPR".into());
    let (lib, name) = queue_name(&which);
    let (msgs, delivery, sev) = with_queue(system, "msgq.receive", &lib, &name, |q, _| {
        Ok(((q.msgs.clone(), q.delivery.clone(), q.severity), false))
    })?;
    let mut out = format!(
        "Queue . . . . . :   {name:<10}    Library . . . . :   {lib}\nDelivery  . . . :   {delivery:<10}    Severity  . . . :   {sev:02}\n\n"
    );
    if msgs.is_empty() {
        out.push_str("No messages available.\n");
    }
    for m in msgs.iter().filter(|m| m.kind != MsgKind::Rpy) {
        let (date, time) = wm::joblog::stamp_of(m.time);
        let _ = writeln!(
            out,
            "{:08X}  From {:<10}  {date} {}  {:<8} {}",
            m.key,
            m.job.1,
            &time[..8],
            m.kind.name(),
            if m.msgid.is_empty() { "" } else { &m.msgid },
        );
        let _ = writeln!(out, "          {}", m.text);
        match m.status {
            Status::Unanswered => {
                let _ = writeln!(
                    out,
                    "          Reply . . . :   (waiting; default {})",
                    if m.dft.is_empty() { "none" } else { &m.dft }
                );
            }
            Status::Answered => {
                let _ = writeln!(out, "          Reply . . . :   {}", m.reply);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Help for the reply list commands.
pub const HELP_RPYL: &str = "\
addrpyle SEQNBR(n) MSGID(id|*ANY) [CMPDTA('..' start) RPY(r|*DFT|*RQD) DUMP(*YES)] | chgrpyle SEQNBR(n) ...
rmvrpyle SEQNBR(n) | wrkrpyle
";

/// The reply list commands; `None` when `cmd` is not one.
pub fn rpyl_command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    if matches!(cmd, "addrpyle" | "chgrpyle" | "rmvrpyle")
        && let Err(e) = super::security::actor()
            .require(sec::profile::SpecialAuth::JOBCTL, "change the reply list")
    {
        return Some(format!("{e}\n"));
    }
    let r = match cmd {
        "addrpyle" => rpyle(system, &p, Edit::Add),
        "chgrpyle" => rpyle(system, &p, Edit::Change),
        "rmvrpyle" => rpyle(system, &p, Edit::Remove),
        "wrkrpyle" => with_rpyl(system, |l| Ok((l.display(), false))),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Edit {
    Add,
    Change,
    Remove,
}

/// Run `f` on the reply list; saved when it says it changed it.
fn with_rpyl<T>(
    system: &mut System,
    f: impl FnOnce(&mut wm::obj::rpyl::RpyList) -> Result<(T, bool), String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    super::db::with_txn(&mut s.store, |txn| {
        let mut l = wm::obj::rpyl::load(txn, qsys).map_err(|e| e.to_string())?;
        let (r, changed) = f(&mut l)?;
        if changed {
            let stamp = wm::obj::Stamp {
                owner: "QSYS".into(),
                system: "QS400".into(),
                now: now_unix(),
            };
            wm::obj::rpyl::save(txn, qsys, &l, &stamp).map_err(|e| e.to_string())?;
        }
        Ok(r)
    })
}

/// `ADDRPYLE`, `CHGRPYLE`, `RMVRPYLE`.
fn rpyle(system: &mut System, p: &Params, edit: Edit) -> Result<String, String> {
    let seq: u16 = p
        .pos_or(0, "SEQNBR")?
        .parse()
        .map_err(|_| "CPD0085 SEQNBR is 1 through 9999".to_string())?;
    let err = |e: wm::Error| format!("{} {e}", e.msgid().unwrap_or("CPF2555"));
    with_rpyl(system, |l| {
        if edit == Edit::Remove {
            l.remove(seq).map_err(err)?;
            return Ok((format!("Reply list entry {seq} removed.\n"), true));
        }
        let mut e = match edit {
            Edit::Change => l
                .entries
                .iter()
                .find(|e| e.seq == seq)
                .cloned()
                .ok_or_else(|| format!("CPF2556 sequence number {seq} is not in the reply list"))?,
            _ => wm::obj::rpyl::RpyEntry {
                seq,
                msgid: "*ANY".into(),
                start: 1,
                reply: "*DFT".into(),
                ..wm::obj::rpyl::RpyEntry::default()
            },
        };
        if let Some(id) = p.upper("MSGID") {
            e.msgid = id;
        }
        if let Some(v) = p.kw("CMPDTA") {
            let v = v.trim();
            if v.eq_ignore_ascii_case("*NONE") {
                e.cmpdta.clear();
            } else {
                let (text, rest) = match v.strip_prefix('\'').and_then(|r| r.split_once('\'')) {
                    Some((t, rest)) => (t.to_string(), rest.trim()),
                    None => (v.to_string(), ""),
                };
                e.cmpdta = text;
                e.start = rest.parse().unwrap_or(1);
            }
        }
        if let Some(r) = p.kw("RPY") {
            e.reply = r.trim().trim_matches('\'').to_ascii_uppercase();
        }
        if let Some(d) = p.upper("DUMP") {
            e.dump = d == "*YES";
        }
        if edit == Edit::Change {
            l.remove(seq).map_err(err)?;
        }
        l.add(e).map_err(err)?;
        let what = if edit == Edit::Add {
            "added"
        } else {
            "changed"
        };
        Ok((format!("Reply list entry {seq} {what}.\n"), true))
    })
}

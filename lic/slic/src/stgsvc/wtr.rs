// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print writers in the service. `STRPRTWTR` submits a writer job (type
//! `W`) to `QSYS/QSPL`, routed by `PRTWTR` to `QSPWTR` in the spooling
//! subsystem; the job asks for the next ready file on its queue, prints
//! it and says so. The capture device's writer prints into the store: a
//! user space `QSPL/CPnnnnnnnn` per file with its text and its PDF,
//! which `prtcapture` reads. A file is complete, counted and saved or
//! deleted, only once the writer says the device took all of it; a
//! printer session's refusal leaves it ready and the writer on an
//! inquiry (`device_error`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use store::obj::usrspc;
use store::obj::{CreateParams, ObjectId, ops, types};
use wm::cmd::Params;
use wm::obj::devd::Devd;
use wm::obj::jobq::SbmJob;
use wm::obj::splf::{SplfAttrs, Status};
use wm::wtr::{WtrDest, WtrStep};

use super::spool::{self, with_outq, with_store};
use crate::ipl::system::{System, now_unix};

use wm::wtr::AfterReply;
pub use wm::wtr::Writer;

/// The inquiry a writer sends when its printer session refuses a file.
pub const DEVICE_ERROR: &str = "CPA3388";
/// The message when a printer session goes while a file prints.
pub const SESSION_LOST: &str = "CPF3429";

/// Every started writer, by name.
pub type Writers = BTreeMap<String, Writer>;

fn device(system: &mut System, dev: &str) -> Result<Devd, String> {
    let b = super::sbs::wm_object(system, &format!("QSYS/{dev}"), "*DEVD")
        .map_err(|_| format!("CPF3310 device {dev} not found"))?;
    Devd::decode(&b).map_err(|e| e.to_string())
}

/// `STRPRTWTR DEV() [OUTQ(*DEV|lib/q)] [WTR(*DEV|name)] [FORMTYPE()] [FILESEP()] [AUTOEND()]`.
pub fn strprtwtr(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let dev = p.pos_or(0, "DEV")?.to_ascii_uppercase();
    let d = device(system, &dev)?;
    if !d.is_printer() || d.is_display() {
        return Err(format!("CPF3310 device {dev} is not a printer"));
    }
    if d.printer.as_ref().is_some_and(|l| !l.is_ip()) {
        return Err(format!(
            "CPF3310 device {dev}: only LANATTACH(*IP) LAN printers print here"
        ));
    }
    let name = match p.upper("WTR").as_deref() {
        None | Some("*DEV") => dev.clone(),
        Some(w) => w.to_string(),
    };
    let q = match p.upper("OUTQ").as_deref() {
        None | Some("*DEV") => d.outq.clone(),
        Some(q) => q.to_string(),
    };
    start(system, me, &name, &dev, &q, p, false)
}

/// `STRRMTWTR OUTQ(lib/q|*ALL) [WTR(*OUTQ|name)] [FORMTYPE()] [AUTOEND()]`:
/// a writer that sends a remote output queue's files over LPR.
pub fn strrmtwtr(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let q = p.pos_or(0, "OUTQ")?.to_ascii_uppercase();
    if q == "*ALL" {
        let mut out = String::new();
        for real in remote_queues(system)? {
            let name = real.rsplit('/').next().unwrap_or(&real).to_string();
            if system.writers.contains_key(&name) {
                continue;
            }
            match start(system, me, &name, "", &real, p, true) {
                Ok(s) => out.push_str(&s),
                Err(e) => {
                    let _ = writeln!(out, "{e}");
                }
            }
        }
        if out.is_empty() {
            out.push_str("No remote output queue is without a writer.\n");
        }
        return Ok(out);
    }
    let (real, remote) = with_outq(system, &q, |oq, real| {
        Ok(((real.to_string(), oq.is_remote()), false))
    })?;
    if !remote {
        return Err(format!(
            "CPF3310 output queue {real} has no remote system (RMTSYS)"
        ));
    }
    let name = match p.upper("WTR").as_deref() {
        None | Some("*OUTQ" | "*SYSGEN") => real.rsplit('/').next().unwrap_or(&real).to_string(),
        Some(w) => w.to_string(),
    };
    start(system, me, &name, "", &real, p, true)
}

/// Every output queue with a remote system, `lib/name`.
fn remote_queues(system: &mut System) -> Result<Vec<String>, String> {
    with_store(system, |txn, qsys| {
        let mut libs = vec![("QSYS".to_string(), qsys)];
        for (k, id) in ops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
            libs.push((k.name.to_string(), id));
        }
        let mut out = Vec::new();
        for (lname, lib) in libs {
            for (k, id) in ops::list(txn, lib, Some(types::OUTQ)).map_err(|e| e.to_string())? {
                let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
                if wm::obj::outq::OutQ::decode(&b).is_ok_and(|q| q.is_remote()) {
                    out.push(format!("{lname}/{}", k.name));
                }
            }
        }
        Ok(out)
    })
}

/// The first word of `FORMTYPE(type msgopt)`.
fn formtype(p: &Params) -> String {
    p.upper("FORMTYPE")
        .and_then(|f| {
            f.trim_matches(['(', ')'])
                .split_whitespace()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| "*ALL".into())
}

/// Submit writer `name` for queue `q`: the job, the queue's writer,
/// the table entry.
fn start(
    system: &mut System,
    me: u32,
    name: &str,
    dev: &str,
    q: &str,
    p: &Params,
    remote: bool,
) -> Result<String, String> {
    if system.writers.contains_key(name) {
        return Err(format!("CPF3313 writer {name} is already started"));
    }
    let (real, busy) = with_outq(system, q, |oq, real| {
        Ok(((real.to_string(), oq.writer.clone()), false))
    })?;
    if !busy.is_empty() {
        return Err(format!(
            "CPF3311 writer {busy} already prints output queue {real}"
        ));
    }
    let r = SbmJob {
        cmd: name.to_string(),
        job: name.to_string(),
        jobd: "QSYS/QSPLJOBD".into(),
        jobq: "QSYS/QSPL".into(),
        user: "QSPLJOB".into(),
        rtgdta: "PRTWTR".into(),
        msgq: "*NONE".into(),
        inllibl: Some(Vec::new()),
        curlib: "*CRTDFT".into(),
        ..SbmJob::default()
    };
    // The system starts the writer as QSPLJOB, not the user asking.
    let (job, _) = sec::guard::as_system(|| super::jobqs::sbmjob(system, me, &r))?;
    with_outq(system, &real, |oq, _| {
        oq.writer = name.to_string();
        Ok(((), true))
    })?;
    // A new writer starts its Printed count at zero.
    let mut w = Writer::started(dev, &real, job);
    w.formtype = formtype(p);
    w.filesep = p.upper("FILESEP").and_then(|n| n.parse().ok()).unwrap_or(0);
    w.autoend = p
        .upper("AUTOEND")
        .is_some_and(|a| a.trim_start_matches('(').starts_with("*YES"));
    w.remote = remote;
    system.writers.insert(name.to_string(), w);
    Ok(format!(
        "Writer {name} submitted as job {job:06}/QSPLJOB/{name} to print output queue {real}.\n"
    ))
}

/// A writer's status as `WRKWTR` shows it.
#[must_use]
pub fn status(system: &System, w: &Writer) -> &'static str {
    if w.ending {
        "END"
    } else if w.held {
        "HLD"
    } else if w.msgw.is_some() {
        "MSGW"
    } else if w.current.is_some() {
        "WTR"
    } else if system.jobs.get(w.job).is_some() {
        "STR"
    } else {
        "JOBQ"
    }
}

/// The `*WSCST` payload `lib/name` names; when none is named, the
/// shipped object `QSYS/QWP...` of a model other than the HP lasers
/// (`print::wscst::shipped`); empty when there is none.
fn wscst_payload(system: &mut System, q: &str, mfrtypmdl: &str) -> Vec<u8> {
    if q.is_empty() {
        if print::transform::Model::parse(mfrtypmdl).is_some() {
            return Vec::new();
        }
        return print::wscst::shipped::for_mfrtypmdl(mfrtypmdl)
            .and_then(|n| super::sbs::wm_object(system, &format!("QSYS/{n}"), "*WSCST").ok())
            .unwrap_or_default();
    }
    super::sbs::wm_object(system, q, "*WSCST").unwrap_or_default()
}

/// Where writer `name` prints.
pub fn target(system: &mut System, name: &str) -> Result<WtrDest, String> {
    let w = system
        .writers
        .get(name)
        .cloned()
        .ok_or_else(|| format!("CPF3313 writer {name} is not started"))?;
    if w.remote {
        let (_, oq, _) = spool::on_queue(system, &w.outq)?;
        let wscst = wscst_payload(system, &oq.remote.wscst, &oq.remote.mfrtypmdl);
        return Ok(WtrDest::Remote {
            host: oq.remote_host(),
            queue: oq.rmtprtq.clone(),
            desttype: oq.remote.desttype,
            destopt: oq.remote.destopt.clone(),
            seppage: oq.remote.seppage,
            transform: oq.remote.transform,
            mfrtypmdl: oq.remote.mfrtypmdl.clone(),
            wscst,
            local_host: system.system_name(),
        });
    }
    let d = device(system, &w.dev)?;
    if d.devcls == "*VRT" && d.is_printer() {
        return Ok(WtrDest::Session {
            device: w.dev.clone(),
        });
    }
    let Some(l) = d.printer.filter(lan_ip) else {
        return Ok(WtrDest::Capture);
    };
    let wscst = wscst_payload(system, &l.wscst, &l.mfrtypmdl);
    Ok(WtrDest::Lan {
        host: l.rmtlocname.clone(),
        port: u16::try_from(l.port).unwrap_or(0),
        driver: l.sysdrvpgm.clone(),
        secure: l.securecnn,
        transform: l.transform,
        mfrtypmdl: l.mfrtypmdl.clone(),
        wscst,
        chrid: l.chrid_text(),
    })
}

/// Whether a printer section is a LAN printer over TCP/IP.
#[must_use]
pub fn lan_ip(l: &wm::obj::devprt::LanPrinter) -> bool {
    l.is_ip()
}

/// Writer `name` could not print `id`: the file is ready again (held
/// when the far end refused it), the writer waits with a message, and
/// the message goes to the system operator.
pub fn failed(
    system: &mut System,
    name: &str,
    id: u32,
    msgid: &str,
    text: &str,
    hold: bool,
) -> Result<(), String> {
    let Some(w) = system.writers.get_mut(name) else {
        return Err(format!("CPF3313 writer {name} is not started"));
    };
    w.current = None;
    w.msgw = Some(msgid.to_string());
    let job = w.job;
    spool::update(system, id, |a: &mut SplfAttrs| {
        a.status = if hold { Status::Hld } else { Status::Rdy };
        if hold {
            a.held_from = Status::Rdy;
        }
        a.writer.clear();
        Ok(())
    })?;
    let (file, number, owner) = system
        .spool
        .files
        .get(&id)
        .map(|e| (e.attrs.name.clone(), e.attrs.number, e.attrs.job.clone()))
        .unwrap_or_default();
    // TCP3701's data: the file, its number, then its job.
    let data = if msgid == "TCP3701" {
        let mut d = cmd::error::data(&[(&file, 10)]);
        d.extend_from_slice(&u16::try_from(number).unwrap_or(u16::MAX).to_be_bytes());
        d.extend(cmd::error::data(&[
            (&owner.0, 10),
            (&owner.1, 10),
            (&format!("{:06}", owner.2), 6),
        ]));
        d
    } else {
        Vec::new()
    };
    let m = wm::obj::msgq::QMsg {
        msgid: msgid.into(),
        msgf: ("QTCPMSGF".into(), "QTCP".into()),
        severity: 40,
        data,
        text: format!("Writer {name} could not send spooled file {file}: {text}"),
        job: (name.to_string(), "QSPLJOB".into(), job),
        from_pgm: "QSPWTR".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
    Ok(())
}

/// Writer `name`'s printer session did not take spooled file `id`: the
/// file is ready again and nothing is counted. A lost session gets a
/// message to the device's message queue (the session that ends ends
/// the writer; one that takes the device again finds the file ready).
/// A refusal gets an inquiry there with the client's diagnostic, and
/// the writer waits in `MSGW` for its reply.
pub fn device_error(
    system: &mut System,
    name: &str,
    id: u32,
    lost: bool,
    text: &str,
    diag: &str,
) -> Result<(), String> {
    let Some(w) = system.writers.get_mut(name) else {
        return Err(format!("CPF3313 writer {name} is not started"));
    };
    w.stopped();
    let (dev, job) = (w.dev.clone(), w.job);
    spool::update(system, id, |a: &mut SplfAttrs| {
        if a.status == Status::Wtr {
            a.status = Status::Rdy;
        }
        a.writer.clear();
        Ok(())
    })?;
    let (file, number) = system
        .spool
        .files
        .get(&id)
        .map(|e| (e.attrs.name.clone(), e.attrs.number))
        .unwrap_or_default();
    let msgq = device(system, &dev)
        .ok()
        .map(|d| d.msgq)
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| "QSYS/QSYSOPR".into());
    let (lib, q) = super::msgs::queue_name(&msgq);
    let number = format!("{number:06}");
    let mut fields: Vec<(&str, usize)> = vec![(name, 10), (&file, 10), (&number, 6), (&dev, 10)];
    let msgid = if lost { SESSION_LOST } else { DEVICE_ERROR };
    if !lost {
        fields.push((diag, 8));
        fields.push((text, 64));
    }
    let data = cmd::error::data(&fields);
    let mut m = wm::obj::msgq::QMsg {
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 40,
        text: super::hist::message_text(msgid, &data),
        data,
        job: (name.to_string(), "QSPLJOB".into(), job),
        from_pgm: "QSPWTR".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    if !lost {
        m.kind = wm::obj::msgq::MsgKind::Inq;
        m.severity = 99;
        m.dft = "R".into();
        m.reply_to = (q.clone(), lib.clone());
    }
    let key = super::msgs::send(system, &lib, &q, m)?;
    if !lost && let Some(w) = system.writers.get_mut(name) {
        w.ask(DEVICE_ERROR, &format!("{lib}/{q}"), key, id);
    }
    Ok(())
}

/// The reply to inquiry `key` on `queue` (`lib/name`), taken; `None`
/// while it waits. An inquiry no longer on the queue counts as
/// answered with its default.
fn take_reply(system: &mut System, queue: &str, key: u32) -> Option<String> {
    use wm::obj::msgq::Status as M;
    let (lib, name) = super::msgs::queue_name(queue);
    super::msgs::with_queue(system, "msgq.receive", &lib, &name, |q, _| {
        let Some(m) = q.msgs.iter().find(|m| m.key == key) else {
            return Ok((Some(String::new()), false));
        };
        if m.status == M::Unanswered {
            return Ok((None, false));
        }
        let (reply, rk) = (m.reply.clone(), m.reply_key);
        let _ = q.receive(None, Some(rk), false);
        Ok((Some(reply), true))
    })
    .unwrap_or(Some(String::new()))
}

/// `ENDWTR`, `HLDWTR`, `RLSWTR`, `CHGWTR`.
pub fn control(system: &mut System, me: u32, cmd: &str, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "WTR")?.to_ascii_uppercase();
    let w = system
        .writers
        .get_mut(&name)
        .ok_or_else(|| format!("CPF3313 writer {name} is not started"))?;
    match cmd {
        "hldwtr" => {
            w.held = true;
            Ok(format!("Writer {name} held.\n"))
        }
        "rlswtr" => {
            w.held = false;
            Ok(format!("Writer {name} released.\n"))
        }
        "chgwtr" => {
            if let Some(f) = p.upper("FORMTYPE") {
                w.formtype = f;
            }
            if let Some(n) = p.upper("FILESEP").and_then(|n| n.parse().ok()) {
                w.filesep = n;
            }
            let q = p.upper("OUTQ");
            let (old, job) = (w.outq.clone(), w.job);
            if let Some(q) = q {
                let real = with_outq(system, &q, |oq, real| {
                    oq.writer.clone_from(&name);
                    Ok((real.to_string(), true))
                })?;
                clear_outq(system, &old);
                if let Some(w) = system.writers.get_mut(&name) {
                    w.outq = real;
                }
            }
            let _ = job;
            Ok(format!("Writer {name} changed.\n"))
        }
        _ => {
            let immediate = matches!(p.upper("OPTION").as_deref(), Some("*IMMED"));
            w.ending = true;
            let job = w.job;
            if immediate {
                let spec = system
                    .jobs
                    .get(job)
                    .map(|j| format!("{:06}/{}/{}", j.number, j.user, j.name));
                if let Some(spec) = spec {
                    super::jobqs::endjob(system, me, &spec, true)?;
                }
            }
            Ok(format!("Writer {name} is ending.\n"))
        }
    }
}

fn clear_outq(system: &mut System, q: &str) {
    let _ = with_outq(system, q, |oq, _| {
        oq.writer.clear();
        Ok(((), true))
    });
}

/// `WRKWTR`.
pub fn wrkwtr(system: &System) -> String {
    let mut out = String::from(
        "Opt  Writer      Type  Device      Sts   Form Type   Output Queue  Library     Printed\n",
    );
    for (name, w) in &system.writers {
        let status = status(system, w);
        let kind = if w.remote { "RMT" } else { "PRT" };
        let (lib, q) = w.outq.split_once('/').unwrap_or(("", &w.outq));
        let _ = writeln!(
            out,
            "     {name:<10}  {kind}   {:<10}  {status:<4}  {:<10}  {q:<12}  {lib:<10}  {:>7}",
            w.dev, w.formtype, w.printed
        );
        match (&w.msgw, &w.inquiry) {
            (Some(m), Some((q, _, _))) => {
                let _ = writeln!(out, "       waiting for the reply to {m} on {q}");
            }
            (Some(m), None) => {
                let _ = writeln!(out, "       waiting after {m}; the writer tries again");
            }
            _ => {}
        }
    }
    if system.writers.is_empty() {
        out.push_str("  (No writers started)\n");
    }
    out
}

/// The writer job `name` asks what to do.
pub fn next(system: &mut System, name: &str) -> WtrStep {
    let Some(w) = system.writers.get(name).cloned() else {
        return WtrStep::End;
    };
    if w.ending {
        return WtrStep::End;
    }
    if let Some((q, key, _)) = w.inquiry.clone() {
        let Some(reply) = take_reply(system, &q, key) else {
            return WtrStep::MsgWait;
        };
        let after = system.writers.get_mut(name).map(|w| w.answered(&reply));
        match after {
            Some(AfterReply::End) | None => return WtrStep::End,
            Some(AfterReply::Hold(id)) => {
                let _ = spool::update(system, id, |a: &mut SplfAttrs| {
                    if a.status == Status::Rdy {
                        a.held_from = Status::Rdy;
                        a.status = Status::Hld;
                    }
                    Ok(())
                });
            }
            Some(AfterReply::Go) => {}
        }
    }
    if w.held {
        return WtrStep::Wait;
    }
    let Ok((_, oq, files)) = spool::on_queue(system, &w.outq) else {
        return WtrStep::Wait;
    };
    if oq.held {
        return WtrStep::Wait;
    }
    let pick = files
        .iter()
        .find(|f| f.status == Status::Rdy && (w.formtype == "*ALL" || w.formtype == f.formtype));
    let Some(f) = pick else {
        return if w.autoend {
            WtrStep::End
        } else {
            WtrStep::Wait
        };
    };
    let id = f.id;
    let wname = name.to_string();
    if spool::update(system, id, |a| {
        a.status = Status::Wtr;
        a.writer.clone_from(&wname);
        Ok(())
    })
    .is_err()
    {
        return WtrStep::Wait;
    }
    if let Some(w) = system.writers.get_mut(name) {
        w.current = Some(id);
    }
    match spool::data(system, id) {
        Ok((attrs, data)) => WtrStep::File { attrs, data },
        Err(_) => WtrStep::Wait,
    }
}

/// The capture user space of spool identifier `id`.
fn capture_name(id: u32) -> String {
    format!("CP{id:08}")
}

/// The writer printed `id`: its capture kept, the file saved or gone.
pub fn printed(
    system: &mut System,
    name: &str,
    id: u32,
    text: &str,
    pdf: &[u8],
) -> Result<(), String> {
    let attrs = system
        .spool
        .files
        .get(&id)
        .map(|e| e.attrs.clone())
        .ok_or_else(|| format!("CPF3309 no spooled file {id}"))?;
    // Only the file this writer has in hand completes.
    if attrs.status != Status::Wtr || attrs.writer != name {
        return Err(format!(
            "CPF3330 spooled file {} is not printing on writer {name}",
            attrs.name
        ));
    }
    if let Some(w) = system.writers.get_mut(name) {
        w.msgw = None;
    }
    super::acg::printed(system, &attrs, name);
    // A network printer leaves no capture.
    if !text.is_empty() || !pdf.is_empty() {
        capture(system, id, &attrs, text, pdf)?;
    }
    finish(system, name, id, &attrs)
}

/// Keep what the capture device printed: text and PDF in a user space.
fn capture(
    system: &mut System,
    id: u32,
    attrs: &SplfAttrs,
    text: &str,
    pdf: &[u8],
) -> Result<(), String> {
    let meta = format!("{} {} {}", attrs.name, attrs.job_text(), attrs.number);
    let mut body = Vec::with_capacity(8 + text.len() + pdf.len());
    body.extend_from_slice(&u32::try_from(text.len()).unwrap_or(0).to_le_bytes());
    body.extend_from_slice(&u32::try_from(pdf.len()).unwrap_or(0).to_le_bytes());
    body.extend_from_slice(text.as_bytes());
    body.extend_from_slice(pdf);
    with_store(system, |txn, qsys| {
        let qspl = wm::obj::library(txn, qsys, "QSPL").map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(&capture_name(id)).map_err(|e| e.to_string())?;
        let params = CreateParams {
            library: qspl,
            name: n,
            type_code: types::USRSPC,
            attribute: "QSCAPTURE".into(),
            text: meta.clone(),
            owner: "QSPLJOB".into(),
            system: "QS400".into(),
            now: now_unix(),
            by_system: true,
        };
        let sp =
            usrspc::create(txn, &params, body.len() as u64, true, 0).map_err(|e| e.to_string())?;
        usrspc::write(txn, sp, 0, &body).map_err(|e| e.to_string())
    })
}

/// The file is printed (the device took all of it): counted once,
/// then saved or deleted.
fn finish(system: &mut System, name: &str, id: u32, attrs: &SplfAttrs) -> Result<(), String> {
    if let Some(w) = system.writers.get_mut(name) {
        w.completed();
    }
    if attrs.save {
        spool::update(system, id, |a| {
            a.status = Status::Sav;
            a.writer.clear();
            a.last_page = a.pages;
            Ok(())
        })
        .map(|_| ())
    } else {
        spool::delete(system, id)
    }
}

/// A job ended: if it was a writer, the writer goes and its file in
/// hand is ready again.
pub fn job_ended(system: &mut System, number: u32) {
    let Some(name) = system
        .writers
        .iter()
        .find(|(_, w)| w.job == number)
        .map(|(n, _)| n.clone())
    else {
        return;
    };
    let Some(w) = system.writers.remove(&name) else {
        return;
    };
    clear_outq(system, &w.outq);
    if let Some(id) = w.current {
        let _ = spool::update(system, id, |a: &mut SplfAttrs| {
            a.status = Status::Rdy;
            a.writer.clear();
            Ok(())
        });
    }
}

/// At start: no writer survives an IPL, so no output queue has one.
pub fn clear_queues(system: &mut System) {
    let _ = with_store(system, |txn, qsys| {
        let mut libs = vec![qsys];
        for (_, id) in ops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
            libs.push(id);
        }
        for lib in libs {
            for (_, id) in ops::list(txn, lib, Some(types::OUTQ)).map_err(|e| e.to_string())? {
                let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
                let Ok(mut oq) = wm::obj::outq::OutQ::decode(&b) else {
                    continue;
                };
                if !oq.writer.is_empty() {
                    oq.writer.clear();
                    wm::obj::write(txn, id, &oq.encode()).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    });
}

/// Every capture: its object and what it holds.
fn captures(system: &mut System) -> Result<Vec<(ObjectId, String)>, String> {
    with_store(system, |txn, qsys| {
        let qspl = wm::obj::library(txn, qsys, "QSPL").map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for (k, id) in ops::list(txn, qspl, Some(types::USRSPC)).map_err(|e| e.to_string())? {
            if !k.name.to_string().starts_with("CP") {
                continue;
            }
            let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
            out.push((id, ccsid::decode_trimmed(&h.text)));
        }
        Ok(out)
    })
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64, 76 characters a line.
fn base64(data: &[u8]) -> String {
    let mut out = String::new();
    let mut line = 0;
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            let c = if i <= chunk.len() {
                B64[((n >> (18 - 6 * i)) & 63) as usize] as char
            } else {
                '='
            };
            out.push(c);
        }
        line += 4;
        if line == 76 {
            out.push('\n');
            line = 0;
        }
    }
    if line > 0 {
        out.push('\n');
    }
    out
}

/// `prtcapture list` | `prtcapture get FILE [JOB(n/u/name)] [SPLNBR(n)] [FORMAT(*TEXT|*PDF)]`
/// | `prtcapture clear`.
pub fn prtcapture(system: &mut System, line: &str) -> Result<String, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let all = captures(system)?;
    match words.get(1).copied() {
        None | Some("list") => {
            let mut out =
                String::from("Capture     File        Job                           Nbr\n");
            for (_, meta) in &all {
                let _ = writeln!(out, "  {meta}");
            }
            Ok(out)
        }
        Some("clear") => {
            let n = all.len();
            with_store(system, |txn, _| {
                for (id, _) in &all {
                    ops::delete(txn, store::obj::JobHandle(0), *id).map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
            Ok(format!("{n} captures deleted.\n"))
        }
        Some("get") => {
            let p = Params::parse(&words[1..].join(" "));
            let file = p.pos_or(0, "FILE")?.to_ascii_uppercase();
            let job = p.upper("JOB");
            let nbr = p.upper("SPLNBR");
            let hit = all
                .iter()
                .rev()
                .find(|(_, meta)| {
                    let m: Vec<&str> = meta.split_whitespace().collect();
                    m.first() == Some(&file.as_str())
                        && job.as_ref().is_none_or(|j| {
                            m.get(1).is_some_and(|mj| {
                                let jt = (
                                    mj.rsplit('/').next().unwrap_or("").to_string(),
                                    mj.split('/').nth(1).unwrap_or("").to_string(),
                                    mj.split('/')
                                        .next()
                                        .and_then(|n| n.parse().ok())
                                        .unwrap_or(0),
                                );
                                spool::job_matches(j, &jt)
                            })
                        })
                        && nbr.as_ref().is_none_or(|n| m.get(2) == Some(&n.as_str()))
                })
                .ok_or_else(|| format!("CPF3309 no capture of {file}"))?;
            let id = hit.0;
            let body = with_store(system, |txn, _| {
                let a = usrspc::describe(txn, id).map_err(|e| e.to_string())?;
                usrspc::read(txn, id, 0, usize::try_from(a.size).unwrap_or(0))
                    .map_err(|e| e.to_string())
            })?;
            let tl =
                u32::from_le_bytes(body[..4].try_into().map_err(|_| "capture damaged")?) as usize;
            let pl =
                u32::from_le_bytes(body[4..8].try_into().map_err(|_| "capture damaged")?) as usize;
            let text =
                String::from_utf8_lossy(body.get(8..8 + tl).ok_or("capture damaged")?).to_string();
            let pdf = body.get(8 + tl..8 + tl + pl).ok_or("capture damaged")?;
            if matches!(p.upper("FORMAT").as_deref(), Some("*PDF")) {
                Ok(format!(
                    "-----BEGIN PDF {} bytes-----\n{}-----END PDF-----\n",
                    pdf.len(),
                    base64(pdf)
                ))
            } else {
                Ok(format!(
                    "-----BEGIN TEXT {}-----\n{}-----END TEXT-----\n",
                    hit.1,
                    text.replace('\x0c', "\n<FF>\n")
                ))
            }
        }
        Some(w) => Err(format!("prtcapture {w}: list, get or clear")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_lines() {
        assert_eq!(base64(b"Man"), "TWFu\n");
        assert_eq!(base64(b"Ma"), "TWE=\n");
        assert_eq!(base64(b"M"), "TQ==\n");
        assert_eq!(base64(&[0u8; 57]).lines().next().unwrap().len(), 76);
    }
}

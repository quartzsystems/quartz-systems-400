// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print writers in the service. `STRPRTWTR` submits a writer job (type
//! `W`) to `QSYS/QSPL`, routed by `PRTWTR` to `QSPWTR` in the spooling
//! subsystem; the job asks for the next ready file on its queue, prints
//! it and says so. The capture device's writer prints into the store: a
//! user space `QSPL/CPnnnnnnnn` per file with its text and its PDF,
//! which `prtcapture` reads. `MIGRATE(phase10)`: real printers and
//! remote writers.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use store::obj::usrspc;
use store::obj::{CreateParams, ObjectId, ops, types};
use wm::cmd::Params;
use wm::obj::devd::Devd;
use wm::obj::jobq::SbmJob;
use wm::obj::splf::{SplfAttrs, Status};
use wm::wtr::WtrStep;

use super::spool::{self, with_outq, with_store};
use crate::ipl::system::{System, now_unix};

/// A started writer.
#[derive(Clone, Debug)]
pub struct Writer {
    /// The device.
    pub dev: String,
    /// The queue it prints, `lib/name`.
    pub outq: String,
    /// Its job's number.
    pub job: u32,
    /// `HLDWTR`.
    pub held: bool,
    /// `ENDWTR`: end after the file in hand.
    pub ending: bool,
    /// `FORMTYPE`: `*ALL` or a form type.
    pub formtype: String,
    /// `FILESEP`.
    pub filesep: u16,
    /// `AUTOEND(*YES)`: end when the queue is empty.
    pub autoend: bool,
    /// The file being printed.
    pub current: Option<u32>,
    /// Files printed.
    pub printed: u32,
}

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
    if d.devcls != "*PRT" {
        return Err(format!("CPF3310 device {dev} is not a printer"));
    }
    let name = match p.upper("WTR").as_deref() {
        None | Some("*DEV") => dev.clone(),
        Some(w) => w.to_string(),
    };
    if system.writers.contains_key(&name) {
        return Err(format!("CPF3313 writer {name} is already started"));
    }
    let q = match p.upper("OUTQ").as_deref() {
        None | Some("*DEV") => d.outq.clone(),
        Some(q) => q.to_string(),
    };
    let (real, busy) = with_outq(system, &q, |oq, real| {
        Ok(((real.to_string(), oq.writer.clone()), false))
    })?;
    if !busy.is_empty() {
        return Err(format!(
            "CPF3311 writer {busy} already prints output queue {real}"
        ));
    }
    let r = SbmJob {
        cmd: name.clone(),
        job: name.clone(),
        jobd: "QSYS/QSPLJOBD".into(),
        jobq: "QSYS/QSPL".into(),
        user: "QSPLJOB".into(),
        rtgdta: "PRTWTR".into(),
        msgq: "*NONE".into(),
        inllibl: Some(Vec::new()),
        curlib: "*CRTDFT".into(),
        ..SbmJob::default()
    };
    let (job, _) = super::jobqs::sbmjob(system, me, &r)?;
    with_outq(system, &real, |oq, _| {
        oq.writer.clone_from(&name);
        Ok(((), true))
    })?;
    system.writers.insert(
        name.clone(),
        Writer {
            dev,
            outq: real.clone(),
            job,
            held: false,
            ending: false,
            formtype: p.upper("FORMTYPE").unwrap_or_else(|| "*ALL".into()),
            filesep: p.upper("FILESEP").and_then(|n| n.parse().ok()).unwrap_or(0),
            autoend: p.upper("AUTOEND").is_some_and(|a| a.starts_with("*YES")),
            current: None,
            printed: 0,
        },
    );
    Ok(format!(
        "Writer {name} submitted as job {job:06}/QSPLJOB/{name} to print output queue {real}.\n"
    ))
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
        let status = if w.ending {
            "END"
        } else if w.held {
            "HLD"
        } else if w.current.is_some() {
            "WTR"
        } else if system.jobs.get(w.job).is_some() {
            "STR"
        } else {
            "JOBQ"
        };
        let (lib, q) = w.outq.split_once('/').unwrap_or(("", &w.outq));
        let _ = writeln!(
            out,
            "     {name:<10}  PRT   {:<10}  {status:<4}  {:<10}  {q:<12}  {lib:<10}  {:>7}",
            w.dev, w.formtype, w.printed
        );
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
    })?;
    if let Some(w) = system.writers.get_mut(name) {
        w.current = None;
        w.printed += 1;
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

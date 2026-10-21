// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The output queue and spooled file commands: `CRTOUTQ`, `CHGOUTQ`,
//! `DLTOUTQ`, `HLDOUTQ`, `RLSOUTQ`, `CLROUTQ`, `WRKOUTQ`, `WRKOUTQD`,
//! `WRKSPLF`, `WRKSPLFA`, `DSPSPLF`, `CPYSPLF`, `CHGSPLFA`, `HLDSPLF`,
//! `RLSSPLF`, `DLTSPLF`.

use std::fmt::Write as _;

use store::obj::{JobHandle, ops, types};
use wm::cmd::Params;
use wm::obj::outq::OutQ;
use wm::obj::params::qualified;
use wm::obj::splf::{SplfAttrs, Status};

use super::spool::{self, with_outq, with_store};
use crate::ipl::system::{System, now_unix};

fn err(e: &wm::Error) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPD0084"))
}

/// The calling job as `number/user/name`.
fn me_job(system: &System, me: u32) -> String {
    system
        .jobs
        .get(me)
        .map(|j| format!("{:06}/{}/{}", j.number, j.user, j.name))
        .unwrap_or_default()
}

fn me_user(system: &System, me: u32) -> String {
    system.jobs.get(me).map(|j| j.user.clone()).unwrap_or_default()
}

/// The spooled file `FILE() JOB() SPLNBR()` names.
fn target(system: &System, me: u32, p: &Params) -> Result<u32, String> {
    let file = p.pos_or(0, "FILE")?.to_ascii_uppercase();
    let job = match p.upper("JOB").as_deref() {
        None | Some("*") => me_job(system, me),
        Some(j) => j.to_string(),
    };
    let nbr = p.upper("SPLNBR").unwrap_or_else(|| "*ONLY".into());
    spool::find(system, &file, &job, &nbr)
}

/// `CRTOUTQ`, `CHGOUTQ`, `DLTOUTQ`.
fn crt_chg_dlt(system: &mut System, me: u32, cmd: &str, p: &Params) -> Result<String, String> {
    let q = p.pos_or(0, "OUTQ")?.to_ascii_uppercase();
    let (lib, name) = qualified(&q);
    match cmd {
        "crtoutq" => {
            let lib = if lib == "*LIBL" || lib == "*CURLIB" {
                "QGPL".to_string()
            } else {
                lib
            };
            let mut oq = OutQ::default();
            oq.apply(p).map_err(|e| err(&e))?;
            let owner = me_user(system, me);
            with_store(system, |txn, qsys| {
                let l = wm::obj::library(txn, qsys, &lib).map_err(|e| err(&e))?;
                let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
                let stamp = wm::obj::Stamp {
                    owner,
                    system: "QS400".into(),
                    now: now_unix(),
                };
                wm::obj::create(txn, l, n, types::OUTQ, &oq.text, &stamp, &oq.encode())
                    .map_err(|e| err(&e))?;
                Ok(())
            })?;
            Ok(format!("Output queue {name} created in library {lib}.\n"))
        }
        "chgoutq" => {
            with_outq(system, &q, |oq, _| {
                oq.apply(p).map_err(|e| err(&e))?;
                Ok(((), true))
            })?;
            Ok(format!("Output queue {name} changed.\n"))
        }
        _ => {
            let (real, oq, _) = spool::on_queue(system, &q)?;
            if !oq.files.is_empty() || !oq.writer.is_empty() {
                return Err(format!(
                    "CPF3417 output queue {real} has spooled files or a writer"
                ));
            }
            with_store(system, |txn, qsys| {
                let (id, _) = spool::outq_object(txn, qsys, &q)?;
                ops::delete(txn, JobHandle(u64::from(me)), id).map_err(|e| e.to_string())
            })?;
            Ok(format!("Output queue {real} deleted.\n"))
        }
    }
}

/// `HLDOUTQ`, `RLSOUTQ`, `CLROUTQ`.
fn queue_op(system: &mut System, cmd: &str, p: &Params) -> Result<String, String> {
    let q = p.pos_or(0, "OUTQ")?.to_ascii_uppercase();
    match cmd {
        "hldoutq" | "rlsoutq" => {
            let hold = cmd == "hldoutq";
            let real = with_outq(system, &q, |oq, real| {
                oq.held = hold;
                Ok((real.to_string(), true))
            })?;
            Ok(format!(
                "Output queue {real} {}.\n",
                if hold { "held" } else { "released" }
            ))
        }
        _ => {
            let (real, _, files) = spool::on_queue(system, &q)?;
            let mut n = 0;
            for f in files {
                if !matches!(f.status, Status::Opn | Status::Wtr) {
                    spool::delete(system, f.id)?;
                    n += 1;
                }
            }
            Ok(format!("{n} spooled files deleted from output queue {real}.\n"))
        }
    }
}

/// `WRKOUTQ`: every queue, or one queue's files.
fn wrkoutq(system: &mut System, p: &Params) -> Result<String, String> {
    let Some(q) = p.pos.first().map(|q| q.to_ascii_uppercase()) else {
        return all_queues(system);
    };
    let (real, oq, files) = spool::on_queue(system, &q)?;
    let (lib, name) = qualified(&real);
    let mut out = format!(
        "Queue: {name:<10}  Library: {lib:<10}  Status: {}\n\n",
        status_of(&oq)
    );
    out.push_str(
        "Opt  File        User        User Data   Sts   Pages  Copies  Form Type   Pty\n",
    );
    for f in &files {
        let _ = writeln!(
            out,
            "     {:<10}  {:<10}  {:<10}  {:<4} {:>6}  {:>6}  {:<10}  {:>3}",
            f.name,
            f.job.1,
            f.usrdta,
            f.status.code(),
            f.pages,
            f.copies_left.max(1),
            f.formtype,
            f.outpty
        );
    }
    if files.is_empty() {
        out.push_str("  (No spooled output files)\n");
    }
    Ok(out)
}

fn status_of(oq: &OutQ) -> String {
    let base = if oq.held { "HLD" } else { "RLS" };
    if oq.writer.is_empty() {
        base.to_string()
    } else {
        format!("{base}/WTR")
    }
}

fn all_queues(system: &mut System) -> Result<String, String> {
    let queues = with_store(system, |txn, qsys| {
        let mut libs = vec![("QSYS".to_string(), qsys)];
        for (k, id) in ops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())? {
            libs.push((k.name.to_string(), id));
        }
        let mut out = Vec::new();
        for (lname, lid) in libs {
            for (k, id) in ops::list(txn, lid, Some(types::OUTQ)).map_err(|e| e.to_string())? {
                let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
                if let Ok(oq) = OutQ::decode(&b) {
                    out.push((lname.clone(), k.name.to_string(), oq));
                }
            }
        }
        Ok(out)
    })?;
    let mut out =
        String::from("Opt  Queue       Library        Files  Writer      Status\n");
    for (lib, name, oq) in queues {
        let _ = writeln!(
            out,
            "     {name:<10}  {lib:<10}  {:>8}  {:<10}  {}",
            oq.files.len(),
            if oq.writer.is_empty() { "" } else { &oq.writer },
            if oq.held { "HLD" } else { "RLS" }
        );
    }
    Ok(out)
}

/// `WRKSPLF [SELECT(user|*CURRENT|*ALL)] [JOB()] [VIEW(1|2|3)]`.
fn wrksplf(system: &System, me: u32, p: &Params) -> String {
    let user = match p.upper("SELECT").as_deref() {
        None | Some("*CURRENT") => Some(me_user(system, me)),
        Some("*ALL") => None,
        Some(u) => Some(u.split_whitespace().next().unwrap_or(u).to_string()),
    };
    let job = p.upper("JOB");
    let view = p.upper("VIEW").unwrap_or_else(|| "1".into());
    let mut files: Vec<&SplfAttrs> = system
        .spool
        .files
        .values()
        .map(|e| &e.attrs)
        .filter(|a| user.as_ref().is_none_or(|u| a.job.1 == *u))
        .filter(|a| job.as_ref().is_none_or(|j| spool::job_matches(j, &a.job)))
        .collect();
    files.sort_by_key(|a| (a.created, a.id));
    let mut out = String::new();
    match view.as_str() {
        "2" => {
            out.push_str("Opt  File        Device or Queue  User Data   Sts   Total Pages  Cur Page  Copy  Pty\n");
            for f in files {
                let q = qualified(&f.outq).1;
                let _ = writeln!(
                    out,
                    "     {:<10}  {:<15}  {:<10}  {:<4} {:>11}  {:>8}  {:>4}  {:>3}",
                    f.name,
                    if f.writer.is_empty() { &q } else { &f.writer },
                    f.usrdta,
                    f.status.code(),
                    f.pages,
                    f.last_page,
                    f.copies_left.max(1),
                    f.outpty
                );
            }
        }
        "3" => {
            out.push_str("Opt  File        File Nbr  Job         User        Number  Date      Time\n");
            for f in files {
                let (date, time) = wm::joblog::stamp_of(f.created);
                let _ = writeln!(
                    out,
                    "     {:<10}  {:>8}  {:<10}  {:<10}  {:06}  {date}  {}",
                    f.name, f.number, f.job.0, f.job.1, f.job.2, &time[..8]
                );
            }
        }
        _ => {
            out.push_str("Opt  File        User        Queue       User Data   Sts   Pages  Copies  Form Type   Pty\n");
            for f in files {
                let _ = writeln!(
                    out,
                    "     {:<10}  {:<10}  {:<10}  {:<10}  {:<4} {:>6}  {:>6}  {:<10}  {:>3}",
                    f.name,
                    f.job.1,
                    qualified(&f.outq).1,
                    f.usrdta,
                    f.status.code(),
                    f.pages,
                    f.copies_left.max(1),
                    f.formtype,
                    f.outpty
                );
            }
        }
    }
    out
}

/// `DSPSPLF FILE JOB SPLNBR [VIEW(*TEXT|*FCFC|*HEX)] [PAGE(n)]`.
fn dspsplf(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let id = target(system, me, p)?;
    let (a, data) = spool::data(system, id)?;
    let view = p.upper("VIEW").unwrap_or_else(|| "*TEXT".into());
    let mut out = format!(
        "File  . . . . . :   {:<10}  Job . . . :   {}\nFile number . . :   {:<10}  Pages . . :   {}\n",
        a.name,
        a.job_text(),
        a.number,
        a.pages
    );
    if view == "*HEX" {
        out.push_str(&::spool::view::hex(&data));
        return Ok(out);
    }
    let (pages, _) = ::spool::scs::decode(&data);
    if view == "*FCFC" {
        out.push_str("Control  ....+....1....+....2....+....3....+....4....+....5....+....6\n");
        for r in ::spool::view::fcfc(&pages) {
            let (c, t) = r.split_at(1);
            let _ = writeln!(out, "   {c}     {t}");
        }
        return Ok(out);
    }
    let only: Option<u32> = p
        .upper("PAGE")
        .map(|n| n.parse().map_err(|_| format!("CPD0084 PAGE({n}) is not valid")))
        .transpose()?;
    out.push_str("Page/Line  ....+....1....+....2....+....3....+....4....+....5....+....6\n");
    for (pg, ln, text) in ::spool::view::rows(&pages) {
        if only.is_some_and(|o| o != pg) {
            continue;
        }
        let _ = writeln!(out, "{pg:>4}/{ln:<3}  {text}");
    }
    Ok(out)
}

/// `CPYSPLF FILE TOFILE JOB SPLNBR TOMBR MBROPT CTLCHAR`.
fn cpysplf(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let id = target(system, me, p)?;
    let to = p.pos_or(1, "TOFILE")?.to_ascii_uppercase();
    let replace = p.upper("MBROPT").is_none_or(|o| o == "*REPLACE");
    let mbr = match p.upper("TOMBR").as_deref() {
        None | Some("*FIRST") => qsname::MemberSpec::First,
        Some(m) => qsname::MemberSpec::Name(
            qsname::Name::parse(m).map_err(|e| format!("CPD0078 TOMBR({m}): {e}"))?,
        ),
    };
    let (_, data) = spool::data(system, id)?;
    let (pages, _) = ::spool::scs::decode(&data);
    let lines = match p.upper("CTLCHAR").as_deref() {
        None | Some("*NONE") => ::spool::view::rows(&pages)
            .into_iter()
            .map(|(_, _, t)| t)
            .collect(),
        Some("*FCFC") => ::spool::view::fcfc(&pages),
        Some("*PRTCTL") => ::spool::view::prtctl(&pages),
        Some(c) => return Err(format!("CPD0084 CTLCHAR({c}) is not valid")),
    };
    let (lib, name) = qualified(&to);
    let n = with_store(system, |txn, qsys| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let file = wm::obj::find(txn, qsys, &names, &lib, &name, types::FILE)
            .map_err(|_| format!("CPF2817 file {name} in library {lib} not found"))?;
        db::util::put_lines(txn, JobHandle(u64::from(me)), file, mbr, &lines, replace)
            .map_err(|e| format!("CPF2817 {e}"))
    })?;
    Ok(format!("{n} records copied to file {name} in {lib}.\n"))
}

/// `CHGSPLFA`, `HLDSPLF`, `RLSSPLF`, `DLTSPLF`, `WRKSPLFA`.
fn file_op(system: &mut System, me: u32, cmd: &str, p: &Params) -> Result<String, String> {
    let id = target(system, me, p)?;
    let name = system.spool.files.get(&id).map(|e| e.attrs.name.clone()).unwrap_or_default();
    match cmd {
        "dltsplf" => {
            if system.spool.files.get(&id).is_some_and(|e| e.attrs.status == Status::Wtr) {
                return Err(format!("CPF3330 spooled file {name} is being printed"));
            }
            spool::delete(system, id)?;
            Ok(format!("Spooled file {name} deleted.\n"))
        }
        "hldsplf" => {
            spool::update(system, id, |a| {
                if a.status != Status::Hld {
                    a.held_from = a.status;
                    a.status = Status::Hld;
                }
                Ok(())
            })?;
            Ok(format!("Spooled file {name} held.\n"))
        }
        "rlssplf" => {
            spool::update(system, id, |a| {
                if a.status == Status::Hld {
                    a.status = match a.held_from {
                        Status::Opn | Status::Clo => a.held_from,
                        _ => Status::Rdy,
                    };
                }
                Ok(())
            })?;
            Ok(format!("Spooled file {name} released.\n"))
        }
        "wrksplfa" => Ok(system
            .spool
            .files
            .get(&id)
            .map(|e| e.attrs.describe())
            .unwrap_or_default()),
        _ => {
            if let Some(q) = p.upper("OUTQ") {
                spool::move_to(system, id, &q)?;
            }
            spool::update(system, id, |a| a.apply(p).map_err(|e| err(&e)))?;
            Ok(format!("Spooled file {name} changed.\n"))
        }
    }
}

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, me: u32, cmd: &str, line: &str) -> Option<Result<String, String>> {
    let p = Params::parse(line);
    Some(match cmd {
        "crtoutq" | "chgoutq" | "dltoutq" => crt_chg_dlt(system, me, cmd, &p),
        "hldoutq" | "rlsoutq" | "clroutq" => queue_op(system, cmd, &p),
        "wrkoutq" => wrkoutq(system, &p),
        "wrkoutqd" => p
            .pos_or(0, "OUTQ")
            .and_then(|q| with_outq(system, &q.to_ascii_uppercase(), |oq, _| Ok((oq.describe(), false)))),
        "wrksplf" => Ok(wrksplf(system, me, &p)),
        "dspsplf" => dspsplf(system, me, &p),
        "cpysplf" => cpysplf(system, me, &p),
        "chgsplfa" | "hldsplf" | "rlssplf" | "dltsplf" | "wrksplfa" => {
            file_op(system, me, cmd, &p)
        }
        _ => return None,
    })
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job accounting in the service (`docs/WORKMGMT.md`, "Job
//! accounting"): each job's accounting code, what it used since its
//! code last changed, and the `QACGJRN` entries `QACGLVL` asks for: `JB`
//! when a job ends or `CHGACGCDE` changes its code, `SP` when a writer
//! prints a spooled file. Printer files always spool here, so `DP` is
//! never written.

use std::collections::BTreeMap;

use wm::cmd::Params;
use wm::obj::splf::SplfAttrs;

use crate::ipl::system::{System, now_unix};

/// The accounting state of the active jobs.
#[derive(Debug, Default)]
pub struct Acct {
    /// Each job's accounting code (a submitted job's from `SBMJOB`).
    codes: BTreeMap<u32, String>,
    /// Each job's current segment: since it started or its code last
    /// changed.
    segs: BTreeMap<u32, Segment>,
    /// Each spooled file's code, from its job when it was made.
    splf: BTreeMap<u32, String>,
}

/// What a job had used when its segment began, and its print counts
/// since.
#[derive(Clone, Copy, Debug, Default)]
struct Segment {
    entered: u64,
    started: u64,
    cpu_ms: u64,
    aux: u64,
    reads: u64,
    writes: u64,
    txns: u64,
    txn_ms: u64,
    lines: u64,
    pages: u64,
    files: u64,
}

/// What `QACGLVL` asks for now.
pub fn level(system: &System) -> acg::Level {
    acg::Level::parse(
        &system
            .sysvals
            .as_ref()
            .and_then(|v| v.get("QACGLVL").ok())
            .map(|v| v.text())
            .unwrap_or_default(),
    )
}

/// The code a job description gives `user`: its own, or with
/// `*USRPRF` the profile's.
fn resolve(system: &mut System, jobd: &str, user: &str) -> String {
    let d = super::sbs::wm_object(system, jobd, "*JOBD")
        .ok()
        .and_then(|b| wm::obj::jobd::JobD::decode(&b).ok())
        .map_or_else(|| "*USRPRF".into(), |d| d.acgcde);
    if d == "*USRPRF" { profile_code(system, user) } else { d }
}

/// The accounting code of `user`'s profile; blank for a special value.
fn profile_code(system: &mut System, user: &str) -> String {
    let code = super::spool::with_store(system, |txn, qsys| {
        sec::profile::objects::read(txn, qsys, user)
            .map(|(_, p)| p.acgcde)
            .map_err(|e| e.to_string())
    })
    .unwrap_or_default();
    if code.starts_with('*') { String::new() } else { code }
}

/// Job `number`'s accounting code; blank when it has none.
pub fn code(system: &System, number: u32) -> String {
    system.stats.acg.codes.get(&number).cloned().unwrap_or_default()
}

/// The code a submitted job starts with: `SBMJOB ACGCDE()` as given,
/// `*CURRENT` the submitter's, `*USRPRF` the user's profile's;
/// absent is `*CURRENT`. `changes` are `SBMJOB`'s other keywords.
pub fn submitted(
    system: &mut System,
    number: u32,
    submitter: u32,
    user: &str,
    changes: &[(String, String)],
) {
    let v = changes
        .iter()
        .find(|(k, _)| k == "ACGCDE")
        .map_or("*CURRENT", |(_, v)| v.trim().trim_matches('\''));
    let code = if v.eq_ignore_ascii_case("*CURRENT") {
        code(system, submitter)
    } else if v.eq_ignore_ascii_case("*USRPRF") {
        profile_code(system, user)
    } else if v.eq_ignore_ascii_case("*BLANK") {
        String::new()
    } else {
        v.to_string()
    };
    system.stats.acg.codes.insert(number, code);
}

/// The job's figures now: CPU, I/O and transactions since it started.
fn figures(system: &System, number: u32) -> Segment {
    let (cpu_ms, _, _, aux) = super::stats::job_figures(system, number);
    let io = system.stats.io.get(&number).copied().unwrap_or_default();
    let t = system.stats.txns.get(&number).copied().unwrap_or_default();
    Segment {
        cpu_ms,
        aux,
        reads: io.db_reads,
        writes: io.db_writes,
        txns: t.count,
        txn_ms: t.ms,
        ..Segment::default()
    }
}

/// A job started: its code (unless `SBMJOB` gave one) and its first
/// segment. `entered` is when it entered the system (submitted).
pub fn job_started(system: &mut System, job: &wm::table::JobInfo, entered: u64) {
    if !system.stats.acg.codes.contains_key(&job.number) {
        let c = resolve(system, &job.jobd, &job.user);
        system.stats.acg.codes.insert(job.number, c);
    }
    let seg = Segment {
        entered: if entered == 0 { job.started } else { entered },
        started: job.started,
        ..figures(system, job.number)
    };
    system.stats.acg.segs.insert(job.number, seg);
}

/// The local offset from UTC, seconds.
fn offset(system: &System, at: u64) -> i64 {
    system
        .sysvals
        .as_ref()
        .map_or(0, |v| i64::from(v.zone().offset_at(at)))
}

/// End job `number`'s segment: with `*JOB`, its `JB` entry. The next
/// segment starts now.
fn close_segment(system: &mut System, number: u32, completion: u16) {
    let Some(seg) = system.stats.acg.segs.get(&number).copied() else {
        return;
    };
    let now = now_unix();
    let fig = figures(system, number);
    let next = Segment {
        entered: seg.entered,
        started: now,
        ..fig
    };
    system.stats.acg.segs.insert(number, next);
    if !level(system).job {
        return;
    }
    let Some(j) = system.jobs.get(number).cloned() else {
        return;
    };
    let off = offset(system, now);
    let local = |t: u64| i64::try_from(t).unwrap_or(0) + off;
    let datfmt = system.attrs.get(&number).map(|a| a.datfmt.clone()).unwrap_or_default();
    let u = acg::JobUse {
        job: j.name.clone(),
        user: j.user.clone(),
        number,
        code: code(system, number),
        kind: j.kind,
        datfmt,
        entered: local(seg.entered),
        started: local(seg.started),
        ended: local(now),
        cpu_ms: fig.cpu_ms.saturating_sub(seg.cpu_ms),
        routing_steps: 1,
        transactions: fig.txns.saturating_sub(seg.txns),
        transaction_secs: fig.txn_ms.saturating_sub(seg.txn_ms) / 1000,
        aux_io: fig.aux.saturating_sub(seg.aux),
        completion,
        lines: seg.lines,
        pages: seg.pages,
        files: seg.files,
        db_puts: fig.writes.saturating_sub(seg.writes),
        db_gets: fig.reads.saturating_sub(seg.reads),
        db_updates: 0,
        suspended_ms: 0,
    };
    let who = jrn::JobInfo {
        name: j.name,
        user: j.user,
        number,
    };
    let r = acg::jb_data(&u).and_then(|data| {
        super::spool::with_store(system, |txn, qsys| acg::write(txn, qsys, "JB", data, who))
    });
    if let Err(e) = r {
        eprintln!("qsstg: job accounting: JB for {number:06}: {e}");
    }
}

/// A job ended with `completion`: its last `JB` entry, and it is
/// forgotten.
pub fn job_ended(system: &mut System, number: u32, completion: u16) {
    close_segment(system, number, completion);
    system.stats.acg.segs.remove(&number);
    system.stats.acg.codes.remove(&number);
}

/// A spooled file closed: its pages, lines and the file count go to its
/// job's segment, and it keeps the job's code until it prints.
pub fn spool_closed(system: &mut System, a: &SplfAttrs) {
    let number = a.job.2;
    let c = code(system, number);
    system.stats.acg.splf.insert(a.id, c);
    if let Some(s) = system.stats.acg.segs.get_mut(&number) {
        s.lines += u64::from(a.records);
        s.pages += u64::from(a.pages);
        s.files += 1;
    }
}

/// A spooled file is gone (deleted or printed and not saved).
pub fn spool_gone(system: &mut System, id: u32) {
    system.stats.acg.splf.remove(&id);
}

/// A writer printed `a` on `device`: with `*PRINT`, its `SP` entry.
pub fn printed(system: &mut System, a: &SplfAttrs, device: &str) {
    let code = system
        .stats
        .acg
        .splf
        .get(&a.id)
        .cloned()
        .unwrap_or_else(|| code(system, a.job.2));
    if !level(system).print {
        return;
    }
    let (lib, file) = a.prtf.split_once('/').unwrap_or(("", a.prtf.as_str()));
    let p = acg::Printed {
        job: a.job.0.clone(),
        user: a.job.1.clone(),
        number: a.job.2,
        code,
        file: (file.to_string(), lib.to_string()),
        device: (device.to_string(), a.devtype.clone(), String::new()),
        pages: u64::from(a.pages),
        lines: u64::from(a.records),
        splf: Some((a.name.clone(), a.number)),
        outpty: a.outpty,
        formtype: a.formtype.clone(),
        bytes: a.data_len,
        usrdta: a.usrdta.clone(),
    };
    let who = jrn::JobInfo {
        name: a.job.0.clone(),
        user: a.job.1.clone(),
        number: a.job.2,
    };
    let r = acg::print_data(&p).and_then(|data| {
        super::spool::with_store(system, |txn, qsys| acg::write(txn, qsys, "SP", data, who))
    });
    if let Err(e) = r {
        eprintln!("qsstg: job accounting: SP for {}: {e}", a.name);
    }
}

/// `CHGACGCDE JOB() ACGCDE()` from job `me`: the old segment journaled
/// under the old code, the new code from now.
pub fn chgacgcde(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let job = p.kw("JOB").map_or_else(String::new, |j| j.trim().to_string());
    let number = if job.is_empty() || job == "*" {
        me
    } else {
        let id = wm::job::JobId::parse(&job).ok_or_else(|| format!("CPF1070 job {job} not found"))?;
        let found = system
            .jobs
            .get(id.number)
            .filter(|j| j.name == id.name.to_string() && j.user == id.user.to_string())
            .ok_or_else(|| format!("CPF1070 job {job} not found"))?;
        super::security::may_control(&found.user, &found.qualified())?;
        found.number
    };
    let v = p
        .kw("ACGCDE")
        .or_else(|| p.pos.first().map(String::as_str))
        .unwrap_or("*SAME")
        .trim()
        .trim_matches('\'')
        .to_string();
    if v.eq_ignore_ascii_case("*SAME") {
        return Ok("Accounting code not changed.".into());
    }
    let new = if v.eq_ignore_ascii_case("*BLANK") { String::new() } else { v };
    if new.chars().count() > 15 {
        return Err("CPD0074 ACGCDE: at most 15 characters".into());
    }
    let j = system
        .jobs
        .get(number)
        .cloned()
        .ok_or_else(|| format!("CPF1070 job {number:06} not found"))?;
    close_segment(system, number, 0);
    system.stats.acg.codes.insert(number, new);
    Ok(format!(
        "Accounting code changed for job {number:06}/{}/{}.",
        j.user, j.name
    ))
}

/// A queued job that never started: forgotten.
pub fn forget(system: &mut System, number: u32) {
    system.stats.acg.codes.remove(&number);
    system.stats.acg.segs.remove(&number);
}

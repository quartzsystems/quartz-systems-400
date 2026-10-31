// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Operational Assistant's commands in the service: `CHGCLNUP`,
//! `STRCLNUP`, `ENDCLNUP` and the cleanup run itself; `CHGBCKUP`,
//! `DSPBCKUP`, `DSPBCKSTS`; `RUNBCKUP`'s plan and its outcome
//! (`QSBCKUP`), the `GO SAVE` defaults; `PRTSYSINF`; and the prompt
//! override values of `CHGCLNUP` and `CHGBCKUP` (`QSPMTOVR`). The
//! options are text in user spaces in `QUSRSYS` (`wm::opa`).

use std::fmt::Write as _;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::block::Parms;
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types, usrspc};
use store::txn::Txn;
use wm::obj::msgq::Status as MsgStatus;
use wm::opa::bckup::{self, Backup, Which};
use wm::opa::clnup::{self, Cleanup, Removed};
use wm::opa::gosave::{self, Defaults};
use wm::opa::hist::{self as bhist, History, Run};
use wm::opa::{DAY, Keep};

use super::direct::{failed, in_store};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CHGCLNUP" => chgclnup(system, parms),
        "STRCLNUP" => strclnup(system, parms),
        "ENDCLNUP" => endclnup(system),
        "CHGBCKUP" => chgbckup(system, parms),
        "DSPBCKUP" => dspbckup(system, parms),
        "DSPBCKSTS" => dspbcksts(system),
        "PRTSYSINF" => prtsysinf(system),
        "QSPMTOVR" => pmtovr(system, parms),
        "QSBCKUP" => qsbckup(system, parms),
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| failed(name, &e)))
}

/// Cleanup options changed.
const CLN_CHANGED: &str = "CPC1E20";
/// Cleanup scheduled.
const CLN_SCHEDULED: &str = "CPC1E21";
/// Cleanup ended.
const CLN_ENDED: &str = "CPC1E22";
/// Cleanup ran.
const CLN_DONE: &str = "CPC1E23";
/// Cleanup is not allowed.
const CLN_NOT_ALLOWED: &str = "CPF1E24";
/// Cleanup was not started.
const CLN_NOT_STARTED: &str = "CPF1E25";
/// Backup options changed.
const BCK_CHANGED: &str = "CPC1E30";
/// A backup completed.
const BCK_DONE: &str = "CPC1E31";
/// A backup ended with saves that failed.
const BCK_FAILED: &str = "CPF1E32";
/// No device for the backup.
const BCK_NO_DEV: &str = "CPF1E33";
/// System information printed.
const SYSINF_DONE: &str = "CPC1E35";

fn text_reply(text: String) -> CmdReply {
    CmdReply {
        text,
        ..CmdReply::default()
    }
}

// --- the user spaces in QUSRSYS

fn space(txn: &mut Txn<'_>, qsys: ObjectId, obj: (&str, &str), create: bool) -> Result<Option<ObjectId>, String> {
    let lib = wm::obj::library(txn, qsys, obj.0).map_err(|e| format!("CPF2110 {e}"))?;
    let name = qsname::Name::parse(obj.1).map_err(|e| e.to_string())?;
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    if let Some(id) = l.lookup(txn, types::USRSPC, name).map_err(|e| e.to_string())? {
        return Ok(Some(id));
    }
    if !create {
        return Ok(None);
    }
    let params = ops::CreateParams {
        library: lib,
        name,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: "Operational Assistant options".into(),
        owner: "QSYS".into(),
        system: String::new(),
        now: now_unix(),
        by_system: true,
    };
    usrspc::create(txn, &params, 16, true, 0)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The text kept in `obj`; empty when it was never written.
fn load_text(system: &mut System, obj: (&str, &str)) -> Result<String, String> {
    in_store(system, |txn, qsys, _| {
        let Some(id) = space(txn, qsys, obj, false)? else {
            return Ok(String::new());
        };
        let len = usrspc::read(txn, id, 0, 8).map_err(|e| e.to_string())?;
        let len = u64::from_be_bytes(len.try_into().map_err(|_| "short space")?);
        let text = usrspc::read(txn, id, 8, usize::try_from(len).unwrap_or(0)).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&text).into_owned())
    })
}

fn store_text(system: &mut System, obj: (&str, &str), text: &str) -> Result<(), String> {
    in_store(system, |txn, qsys, _| {
        let id = space(txn, qsys, obj, true)?.ok_or("not created")?;
        usrspc::write(txn, id, 0, &(text.len() as u64).to_be_bytes()).map_err(|e| e.to_string())?;
        usrspc::write(txn, id, 8, text.as_bytes()).map_err(|e| e.to_string())
    })
}

fn cleanup(system: &mut System) -> Result<Cleanup, String> {
    Ok(Cleanup::decode(&load_text(system, clnup::OBJECT)?))
}

fn backup(system: &mut System) -> Result<Backup, String> {
    Ok(Backup::decode(&load_text(system, bckup::OBJECT)?))
}

/// The `GO SAVE` command defaults.
fn save_defaults(system: &mut System) -> Result<Defaults, String> {
    Ok(Defaults::decode(&load_text(system, gosave::OBJECT)?))
}

// --- running a command string here

/// Run a CL command string in the service: analyzed against its
/// shipped definition, its processing program called. The reply.
pub(super) fn run_cl(system: &mut System, text: &str) -> CmdReply {
    let name = text
        .trim()
        .trim_start_matches('?')
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let Ok(obj) = sys_cpps::install::object(&name, 0) else {
        return super::escape("CPD0030", &name, &format!("command {name} not found"));
    };
    let mut src = MemSource::default();
    src.put(obj.def);
    let mut env = Env::new(&mut src);
    let r = match analyze(&mut env, text.trim().trim_start_matches('?')) {
        Ok(r) => r,
        Err(e) => return super::escape(&e.escape.msgid, &name, &e.to_string()),
    };
    let parms = Parms::from_bound(&r.def, r.parms.clone());
    super::run(system, &r.name, &parms, "")
}

/// A reply's last line, or its escape.
fn outcome(r: &CmdReply) -> (bool, String) {
    let last = r
        .text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("")
        .to_string();
    match &r.escape {
        Some(e) => (false, if last.is_empty() { format!("{} ended", e.msgid) } else { last }),
        None => (true, last),
    }
}

// --- cleanup

fn me(system: &System) -> u32 {
    system.current_job
}

fn chgclnup(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "change the cleanup options")?;
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, "change the cleanup options")?;
    let mut c = cleanup(system)?;
    let get = |k: &str| -> Option<String> {
        if !parms.specified(k) {
            return None;
        }
        match k {
            "USRMSG" => parms.elems(k).first().copied().flatten().map(cmd::Value::source),
            "JOBQ" => {
                let (n, l) = parms.qual(k);
                Some(if l.is_empty() || n.starts_with('*') { n } else { format!("{l}/{n}") })
            }
            _ => Some(parms.text(k)),
        }
    };
    c.apply(get).map_err(|clnup::Bad(k, v)| format!("CPD0084 {k}({v}) is not valid"))?;
    store_text(system, clnup::OBJECT, &c.encode())?;
    if c.scheduled {
        schedule(system, &c)?;
    }
    Ok(super::done(format!("{CLN_CHANGED} Cleanup options changed.\n"), CLN_CHANGED, &[]))
}

/// The daily entry, replaced with the options' time and queue.
fn schedule(system: &mut System, c: &Cleanup) -> Result<(), String> {
    let me = me(system);
    let _ = crate::stgsvc::jobscd::command(system, me, "rmvjobscde", &format!("rmvjobscde JOB({})", clnup::SCHED_ENTRY));
    let Some(line) = c.sched_entry() else {
        // *SCDPWROFF: the cleanup runs when the power schedule ends the system.
        return Ok(());
    };
    crate::stgsvc::jobscd::command(system, me, "addjobscde", &line).map(|_| ())
}

fn strclnup(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "start cleanup")?;
    let mut c = cleanup(system)?;
    if !c.allow {
        return Err(format!("{CLN_NOT_ALLOWED} Cleanup is not allowed: CHGCLNUP ALWCLNUP(*YES) allows it."));
    }
    if parms.name("OPTION") == "*IMMED" {
        let now = now_unix();
        let removed = run_cleanup(system, &c, now);
        let summary = removed.summary();
        c.last_run = now;
        c.last_result.clone_from(&summary);
        store_text(system, clnup::OBJECT, &c.encode())?;
        let who = crate::stgsvc::msgs::sender(system, me(system));
        let data = wm::joblog::data(&[(&summary, 200)]);
        crate::stgsvc::hist::system_message(system, CLN_DONE, &data, "01", who);
        return Ok(super::done(format!("{CLN_DONE} Cleanup ran: {summary}.\n"), CLN_DONE, &[(&summary, 200)]));
    }
    schedule(system, &c).map_err(|e| format!("{CLN_NOT_STARTED} {e}"))?;
    c.scheduled = true;
    store_text(system, clnup::OBJECT, &c.encode())?;
    Ok(super::done(
        format!("{CLN_SCHEDULED} Cleanup scheduled daily at {}.\n", c.strtime),
        CLN_SCHEDULED,
        &[(&c.strtime, 10)],
    ))
}

fn endclnup(system: &mut System) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::JOBCTL, "end cleanup")?;
    let mut c = cleanup(system)?;
    let me = me(system);
    let _ = crate::stgsvc::jobscd::command(system, me, "rmvjobscde", &format!("rmvjobscde JOB({})", clnup::SCHED_ENTRY));
    c.scheduled = false;
    store_text(system, clnup::OBJECT, &c.encode())?;
    Ok(super::done(format!("{CLN_ENDED} Cleanup ended.\n"), CLN_ENDED, &[]))
}

/// `yyddd` of a history file name to Unix seconds.
fn hist_day(name: &str) -> Option<u64> {
    let yy: i64 = name.get(4..6)?.parse().ok()?;
    let ddd: i64 = name.get(6..9)?.parse().ok()?;
    let days = wm::obj::jobscd::days_from_civil(2000 + yy, 1, 1) + ddd - 1;
    u64::try_from(days).ok().map(|d| d * DAY)
}

/// One cleanup: each kind of item past its keeping removed.
pub fn run_cleanup(system: &mut System, c: &Cleanup, now: u64) -> Removed {
    let mut r = Removed::default();
    // Job logs and system output.
    let spooled: Vec<u32> = system
        .spool
        .files
        .values()
        .filter(|e| matches!(e.attrs.outq.as_str(), "QUSRSYS/QEZJOBLOG" | "QUSRSYS/QEZDEBUG"))
        .filter(|e| !matches!(e.attrs.status, wm::obj::splf::Status::Opn | wm::obj::splf::Status::Wtr))
        .filter(|e| c.sysprt.expired(e.attrs.created / 1_000_000, now))
        .map(|e| e.attrs.id)
        .collect();
    r.spooled = spooled
        .iter()
        .filter(|id| crate::stgsvc::spool::delete(system, **id).is_ok())
        .count();
    // Orphaned spool storage and expired files (QRCLSPLSTG).
    let _ = crate::stgsvc::spool::maintain(system);
    (r.user_msgs, r.sys_msgs) = clean_messages(system, c, now);
    // History files, system journal receivers, QRPLOBJ.
    let keep = c.syslog;
    let temp: Vec<String> = super::ptfcmd::load_db(system)
        .map(|db| {
            db.ptfs
                .values()
                .filter(|p| p.status == ptf::status::Status::TempApplied)
                .flat_map(|p| p.replaced.iter().map(|x| x.saved_as.clone()))
                .collect()
        })
        .unwrap_or_default();
    let counts = in_store(system, |txn, qsys, _| {
        let mut hist: Vec<(String, ObjectId)> = ops::list(txn, qsys, Some(types::FILE))
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(k, id)| (k.name.to_string(), id))
            .filter(|(n, _)| n.len() == 10 && n.starts_with("QHST") && n[4..].bytes().all(|b| b.is_ascii_digit()))
            .collect();
        hist.sort();
        // The newest file is the one being written.
        hist.pop();
        let mut files = 0;
        for (n, id) in hist {
            if hist_day(&n).is_some_and(|t| keep.expired(t + DAY, now))
                && db::file::delete_file(txn, JobHandle(0), id).is_ok()
            {
                files += 1;
            }
        }
        let mut rcvs = 0;
        for (_, jid) in ops::list(txn, qsys, Some(types::type_by_name("*JRN").map_or(0x0018, |t| t.code))).map_err(|e| e.to_string())? {
            let Ok(j) = jrn::Journal::load(txn, jid) else {
                continue;
            };
            for d in j.receivers(txn).unwrap_or_default() {
                let Ok(rcv) = jrn::Receiver::load(txn, d.receiver) else {
                    continue;
                };
                let detached = matches!(rcv.attr.status, jrn::RcvStatus::Online | jrn::RcvStatus::Saved);
                if detached
                    && rcv.attr.detached != 0
                    && keep.expired(rcv.attr.detached, now)
                    && jrn::rcv::dltjrnrcv(txn, JobHandle(0), d.receiver).is_ok()
                {
                    rcvs += 1;
                }
            }
        }
        let mut replaced = 0;
        if let Ok(rpl) = wm::obj::library(txn, qsys, ptf::apply::QRPLOBJ) {
            for (k, id) in ops::list(txn, rpl, None).map_err(|e| e.to_string())? {
                if temp.contains(&k.name.to_string()) {
                    continue;
                }
                if ops::delete(txn, JobHandle(0), id).is_ok() {
                    replaced += 1;
                }
            }
        }
        Ok((files, rcvs, replaced))
    })
    .unwrap_or((0, 0, 0));
    (r.hist_files, r.receivers, r.replaced) = counts;
    // Problems.
    if let Keep::Days(n) = keep
        && let Ok(mut log) = crate::stgsvc::prblog::load(system)
    {
        let f = prb::Filter {
            before: Some(now.saturating_sub(u64::from(n) * DAY)),
            ..prb::Filter::default()
        };
        r.problems = log.delete(&f);
        if r.problems > 0 {
            let _ = crate::stgsvc::prblog::store(system, &log);
        }
    }
    r
}


/// Messages past keeping removed: (user, system and workstation).
fn clean_messages(system: &mut System, c: &Cleanup, now: u64) -> (usize, usize) {
    let mut r = (0, 0);
    // Messages: QUSRSYS holds the users' queues; QSYS QSYSOPR and the
    // workstations'.
    let queues = in_store(system, |txn, qsys, _| {
        let mut out = Vec::new();
        for (lib, system_q) in [("QUSRSYS", false), ("QSYS", true)] {
            let Ok(id) = wm::obj::library(txn, qsys, lib) else {
                continue;
            };
            for (k, _) in ops::list(txn, id, Some(types::MSGQ)).map_err(|e| e.to_string())? {
                let n = k.name.to_string();
                if n != "QHST" {
                    out.push((lib.to_string(), n, system_q));
                }
            }
        }
        Ok(out)
    })
    .unwrap_or_default();
    for (lib, name, system_q) in queues {
        let n = crate::stgsvc::msgs::with_queue(system, "msgq.clear", &lib, &name, |q, _| {
            let before = q.msgs.len();
            q.msgs.retain(|m| {
                let unanswered = m.status == MsgStatus::Unanswered;
                !clnup::message_expired(c, system_q, m.severity, unanswered, m.time / 1_000_000, now)
            });
            let gone = before - q.msgs.len();
            Ok((gone, gone > 0))
        })
        .unwrap_or(0);
        if system_q {
            r.1 += n;
        } else {
            r.0 += n;
        }
    }
    r
}

// --- backup

fn which(parms: &Parms) -> Result<Vec<Which>, String> {
    let v = parms.name("BCKUPOPT");
    if v == "*ALL" {
        return Ok(Which::ALL.to_vec());
    }
    Which::parse(&v)
        .map(|w| vec![w])
        .ok_or_else(|| format!("CPD0084 BCKUPOPT({v}) is not valid"))
}

/// A list parameter's items, space-separated.
fn items(parms: &Parms, kwd: &str) -> String {
    parms
        .list(kwd)
        .iter()
        .map(|v| v.source().trim().trim_matches('\'').to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn chgbckup(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, "change the backup options")?;
    crate::stgsvc::security::require(SpecialAuth::SAVSYS, "change the backup options")?;
    let ws = which(parms)?;
    let mut b = backup(system)?;
    let get = |k: &str| -> Option<String> {
        if !parms.specified(k) {
            return None;
        }
        match k {
            "DEV" | "TAPSET" => {
                let v = items(parms, k);
                Some(if v.eq_ignore_ascii_case("*NONE") { String::new() } else { v })
            }
            "EXITPGM" => {
                let (n, l) = parms.qual(k);
                Some(if l.is_empty() { n } else { format!("{l}/{n}") })
            }
            _ => Some(parms.text(k)),
        }
    };
    // DEV(*NONE) clears the devices.
    let clear_dev = parms.specified("DEV") && items(parms, "DEV").eq_ignore_ascii_case("*NONE");
    b.apply(&ws, get).map_err(|bckup::Bad(k, v)| format!("CPD0084 {k}({v}) is not valid"))?;
    if clear_dev {
        for w in &ws {
            b.of_mut(*w).dev.clear();
        }
    }
    store_text(system, bckup::OBJECT, &b.encode())?;
    Ok(super::done(format!("{BCK_CHANGED} Backup options changed.\n"), BCK_CHANGED, &[]))
}

/// The date of a Unix time, `MM/DD/YY`; blank for never.
fn date_of(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    wm::joblog::stamp_of(t.saturating_mul(1_000_000)).0
}

/// The display's values of one backup, by panel variable.
fn backup_fields(b: &Backup, w: Which) -> Vec<(String, String)> {
    let o = b.of(w);
    let yes = |v: bool| if v { "Yes" } else { "No" }.to_string();
    let mut v = vec![
        ("BCKTYPE".to_string(), w.word().to_string()),
        ("LASTDATE".into(), date_of(o.last)),
        ("LASTTAP".into(), o.last_tapset.clone()),
        ("BCKDEV".into(), o.dev.join(" ")),
        ("CLRTAP".into(), yes(o.clrtap)),
        ("USRLIB".into(), o.lib.word().into()),
        ("USRFLR".into(), o.flr.word().into()),
        ("USRDIR".into(), o.dir.word().into()),
        ("SECDTA".into(), yes(o.secdta)),
        ("CFGDTA".into(), yes(o.cfg)),
        ("SBMJOB".into(), yes(o.sbmjob)),
        ("CHGONLY".into(), yes(o.chgonly)),
        ("PRTRPT".into(), yes(o.prtrpt)),
        ("EXITPGM".into(), o.exitpgm.clone()),
    ];
    for i in 0..7 {
        v.push((format!("TAPSET{}", i + 1), o.tapset.get(i).cloned().unwrap_or_default()));
    }
    v
}

fn dspbckup(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let ws = which(parms)?;
    let b = backup(system)?;
    let mut out = String::new();
    for w in ws {
        let o = b.of(w);
        let yes = |v: bool| if v { "Yes" } else { "No" };
        let _ = writeln!(out, "{} backup options", w.word());
        let _ = writeln!(out, "  Last backup date . . . . . : {}", date_of(o.last));
        let _ = writeln!(out, "  Last tape set used . . . . : {}", o.last_tapset);
        let _ = writeln!(out, "  Backup device  . . . . . . : {}", o.dev.join(" "));
        let _ = writeln!(out, "  Tape sets to rotate  . . . : {}", o.tapset.join(" "));
        let _ = writeln!(out, "  Erase tape before backup . : {}", yes(o.clrtap));
        let _ = writeln!(out, "  User libraries . . . . . . : {}", o.lib.word());
        let _ = writeln!(out, "  Folders  . . . . . . . . . : {}", o.flr.word());
        let _ = writeln!(out, "  User directories . . . . . : {}", o.dir.word());
        let _ = writeln!(out, "  Security data  . . . . . . : {}", yes(o.secdta));
        let _ = writeln!(out, "  Configuration  . . . . . . : {}", yes(o.cfg));
        let _ = writeln!(out, "  Submit as a batch job  . . : {}", yes(o.sbmjob));
        let _ = writeln!(out, "  Changed objects only . . . : {}", yes(o.chgonly));
        let _ = writeln!(out, "  Print detailed report  . . : {}", yes(o.prtrpt));
        let _ = writeln!(out, "  User exit program  . . . . : {}", o.exitpgm);
        let _ = writeln!(out, "  IPL after the backup . . . : {}", yes(o.ipl_after));
    }
    if parms.name("OUTPUT") == "*PRINT" {
        let lines: Vec<String> = out.lines().map(str::to_string).collect();
        return crate::svc::security::print_as(system, "QPEZBCKUP", "DSPBCKUP", &lines, false).map(text_reply);
    }
    Ok(text_reply(out))
}

fn dspbcksts(system: &mut System) -> Result<CmdReply, String> {
    let h = History::decode(&load_text(system, bhist::OBJECT)?);
    let mut out = String::from("Backup status\n");
    if h.runs.is_empty() {
        out.push_str("  No backup has run.\n");
    }
    for r in h.runs.iter().rev() {
        let (d, t) = wm::joblog::stamp_of(r.started.saturating_mul(1_000_000));
        let _ = writeln!(
            out,
            "  {d} {t}  {:<9} {:<10} {:<5} {}",
            r.option,
            r.dev,
            r.tapset,
            if r.ok { "Completed" } else { "Ended with errors" }
        );
        for s in r.steps.split(';').filter(|s| !s.is_empty()) {
            let _ = writeln!(out, "      {s}");
        }
    }
    Ok(text_reply(out))
}

/// `QSBCKUP ACTION(*PLAN)`: the commands of a backup, one per line,
/// `R ` before one that needs the restricted state; the first line is
/// `DEV dev TAPSET set RESTRICTED Y|N SBMJOB Y|N IPL Y|N CTL sbsd`.
/// `*DONE` records a run (`DATA` is `ok;step;step...`). `*DFT` and
/// `*SETDFT` read and keep the `GO SAVE` defaults.
fn qsbckup(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    match parms.name("ACTION").as_str() {
        "*PLAN" => plan(system, parms),
        "*DONE" => done(system, parms),
        "*DFT" => Ok(text_reply(save_defaults(system)?.encode())),
        "*FIELDS" => {
            let w = Which::parse(&parms.name("BCKUPOPT")).ok_or("CPD0084 BCKUPOPT is not valid")?;
            let b = backup(system)?;
            let mut text = String::new();
            for (k, v) in backup_fields(&b, w) {
                let _ = writeln!(text, "{k}={v}");
            }
            Ok(text_reply(text))
        }
        "*SETDFT" => {
            let d = Defaults::decode(&parms.text("DATA").replace(';', "\n"));
            store_text(system, gosave::OBJECT, &d.encode())?;
            Ok(text_reply(String::new()))
        }
        a => Err(format!("CPD0084 ACTION({a}) is not valid")),
    }
}

fn plan(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::SAVSYS, "run a backup")?;
    let w = Which::parse(&parms.name("BCKUPOPT")).ok_or("CPD0084 BCKUPOPT is not valid")?;
    let b = backup(system)?;
    let o = b.of(w).clone();
    let dev = parms.name("DEV");
    let dev = if dev.is_empty() || dev == "*BCKUPOPT" {
        o.dev.first().cloned().unwrap_or_default()
    } else {
        dev.split_whitespace().next().unwrap_or("").to_string()
    };
    if dev.is_empty() {
        return Err(format!(
            "{BCK_NO_DEV} The {} backup has no device: CHGBCKUP DEV() names one.",
            w.word().to_ascii_lowercase()
        ));
    }
    let libs = super::savcmd::expand_libs(system, &["*ALLUSR".into()])?;
    let steps = bckup::plan(&o, &dev, &libs);
    let yn = |b: bool| if b { 'Y' } else { 'N' };
    let mut out = format!(
        "DEV {dev} TAPSET {} RESTRICTED {} SBMJOB {} IPL {} CTL {}\n",
        if o.tapset.is_empty() { "*NONE".into() } else { o.next_tapset() },
        yn(system.sbs.is_empty()),
        yn(o.sbmjob),
        yn(o.ipl_after),
        crate::stgsvc::sbs::controlling(system),
    );
    for s in steps {
        let _ = writeln!(out, "{}{}", if s.restricted { "R " } else { "- " }, s.cmd);
    }
    Ok(text_reply(out))
}

fn done(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let opt = parms.name("BCKUPOPT");
    let data = parms.text("DATA");
    let mut fields = data.split(';');
    let ok = fields.next() == Some("1");
    let tapset = fields.next().unwrap_or("").to_string();
    let steps: Vec<&str> = fields.collect();
    let now = now_unix();
    let mut h = History::decode(&load_text(system, bhist::OBJECT)?);
    let started = steps.first().and_then(|s| s.parse::<u64>().ok()).unwrap_or(now);
    h.add(Run {
        option: opt.clone(),
        started,
        ended: now,
        dev: parms.name("DEV"),
        tapset: tapset.clone(),
        ok,
        steps: steps.iter().skip(1).copied().collect::<Vec<_>>().join(";"),
    });
    store_text(system, bhist::OBJECT, &h.encode())?;
    if let Some(w) = Which::parse(&opt) {
        let mut b = backup(system)?;
        b.of_mut(w).last = now;
        if tapset != "*NONE" {
            b.of_mut(w).last_tapset = tapset;
        }
        store_text(system, bckup::OBJECT, &b.encode())?;
    }
    let who = crate::stgsvc::msgs::sender(system, me(system));
    let id = if ok { BCK_DONE } else { BCK_FAILED };
    let data = wm::joblog::data(&[(&opt, 10)]);
    crate::stgsvc::hist::system_message(system, id, &data, if ok { "01" } else { "15" }, who);
    if ok {
        Ok(super::done(format!("{id} Backup {opt} completed.\n"), id, &[(&opt, 10)]))
    } else {
        Err(format!("{id} Backup {opt} ended; one or more saves failed."))
    }
}

// --- the prompt overrides

fn pmtovr(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let key = parms.text("KEY");
    let form = match parms.name("CMD").as_str() {
        "CHGCLNUP" => cleanup(system)?.keyword_form(),
        "CHGBCKUP" => {
            let v = wm::cmd::Params::parse(&format!("x {key}"));
            match v.upper("BCKUPOPT").as_deref().and_then(Which::parse) {
                Some(w) => backup(system)?.keyword_form(w),
                None => String::new(),
            }
        }
        _ => String::new(),
    };
    Ok(text_reply(form))
}

// --- PRTSYSINF

fn prtsysinf(system: &mut System) -> Result<CmdReply, String> {
    crate::stgsvc::security::require(SpecialAuth::ALLOBJ, "print the system information")?;
    let mut lines: Vec<String> = Vec::new();
    let (d, t) = wm::joblog::stamp_of(now_unix().saturating_mul(1_000_000));
    lines.push(format!("System information for {}    {d} {t}", system.system_name()));
    lines.push(String::new());
    let sections: [(&str, &str); 7] = [
        ("System values", "WRKSYSVAL OUTPUT(*PRINT)"),
        ("Network attributes", "DSPNETA"),
        ("Configuration", "WRKCFGSTS CFGTYPE(*DEV)"),
        ("User profiles", "DSPAUTUSR"),
        ("Licensed programs", "DSPSFWRSC"),
        ("Program temporary fixes", "DSPPTF"),
        ("Library list", "DSPLIBL"),
    ];
    for (title, cmd) in sections {
        lines.push(format!("--- {title}"));
        let r = run_cl(system, cmd);
        let (ok, last) = outcome(&r);
        if r.text.trim().is_empty() || !ok {
            lines.push(format!("  ({cmd}: {last})"));
        } else {
            lines.extend(r.text.lines().map(|l| format!("  {l}")));
        }
        lines.push(String::new());
    }
    lines.push("--- Startup program QSTRUP, as shipped".into());
    lines.extend(osobjects::qstrup::SOURCE.lines().map(|l| format!("  {l}")));
    let printed = crate::svc::security::print_as(system, "QPRTSYSINF", "PRTSYSINF", &lines, false)?;
    Ok(super::done(
        format!("{printed}{SYSINF_DONE} System information printed.\n"),
        SYSINF_DONE,
        &[],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_file_dates() {
        // QHST26001 is 2026-01-01.
        assert_eq!(hist_day("QHST260010"), Some(1_767_225_600));
        assert_eq!(hist_day("QHST26032x"), Some(1_767_225_600 + 31 * DAY));
        assert_eq!(hist_day("QHST"), None);
    }
}

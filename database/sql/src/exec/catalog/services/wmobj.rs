// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management objects as views: `JOB_DESCRIPTION_INFO` from the
//! `*JOBD` objects work management keeps (`wm::obj::jobd`).

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use crate::error::Result;

/// `JOB_DESCRIPTION_INFO`'s `NOT NULL` columns with no value here: a
/// QS/400 job description keeps no job switches.
pub const JOBD_NO_VALUE: &[&str] = &["JOB_SWITCHES"];

/// Every object of `type_code` the user can list: library name, object
/// name, object.
pub(super) fn all_of(txn: &mut Txn<'_>, ctx: &SqlContext, type_code: u16) -> Result<Vec<(String, String, ObjectId)>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    let mut out = Vec::new();
    for lib in libs {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let Ok(list) = ops::list(txn, lib, Some(type_code)) else { continue };
        for (k, id) in list {
            out.push((lh.name.to_string(), k.name.to_string(), id));
        }
    }
    Ok(out)
}

/// `JOB_DESCRIPTION_INFO`.
pub fn jobds(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, types::JOBD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(d) = wm::obj::jobd::JobD::decode(&bytes) else { continue };
        out.row();
        out.text("JOB_DESCRIPTION_LIBRARY", lib);
        out.text("JOB_DESCRIPTION", name);
        out.text("AUTHORIZATION_NAME", d.user.trim());
        out.text("ACCOUNTING_CODE", d.acgcde.trim());
        out.text("ROUTING_DATA", d.rtgdta.trim());
        out.text_or_null("REQUEST_DATA", &d.rqsdta);
        if let Some(l) = &d.inllibl {
            out.int("LIBRARY_LIST_COUNT", l.len());
            out.text("LIBRARY_LIST", l.iter().map(|n| format!("{n:<10}")).collect::<Vec<_>>().concat().trim_end().to_string());
        } else {
            out.int("LIBRARY_LIST_COUNT", 0);
            out.text("LIBRARY_LIST", "*SYSVAL");
        }
        out.text_or_null("TEXT_DESCRIPTION", &d.text);
        out.text("JOB_QUEUE_LIBRARY", d.jobq.0.as_str());
        out.text("JOB_QUEUE", d.jobq.1.as_str());
        out.int("JOB_QUEUE_PRIORITY", d.jobpty);
        out.yes("HOLD_ON_JOB_QUEUE", d.hold);
        out.text_or_null("OUTPUT_QUEUE_LIBRARY", &d.outq.0);
        out.text("OUTPUT_QUEUE", d.outq.1.trim());
        out.int("OUTPUT_QUEUE_PRIORITY", d.outpty);
        out.text("SPOOLED_FILE_ACTION", d.splfacn.trim());
        out.text("PRINTER_DEVICE", d.prtdev.trim());
        out.text_or_null("PRINT_TEXT", &d.prttxt);
        if let Some(m) = d.jobmsgqmx {
            out.int("JOB_MESSAGE_QUEUE_MAXIMUM_SIZE", m);
        }
        out.text("JOB_MESSAGE_QUEUE_FULL_ACTION", d.jobmsgqfl.trim());
        if let Some(s) = d.syntax {
            out.int("SYNTAX_CHECK_SEVERITY", s);
        }
        out.int("JOB_END_SEVERITY", d.endsev);
        out.text("JOBLOG_OUTPUT", d.logoutput.trim());
        out.text("INQUIRY_MESSAGE_REPLY", d.inqmsgrpy.trim());
        out.int("MESSAGE_LOGGING_LEVEL", d.log_level);
        out.int("MESSAGE_LOGGING_SEVERITY", d.log_sev);
        out.text("MESSAGE_LOGGING_TEXT", d.log_text.trim());
        out.text("LOG_CL_PROGRAM_COMMANDS", d.logclpgm.trim());
        out.text("DEVICE_RECOVERY_ACTION", d.devrcyacn.trim());
        out.text("TIME_SLICE_END_POOL", d.tsepool.trim());
        out.text("ALLOW_MULTIPLE_THREADS", if d.alwmltthd { "*YES" } else { "*NO" });
        out.text_or_null("ASPGRP", &d.inlaspgrp);
        out.text("DDM_CONVERSATION", d.ddmcnv.trim());
    }
    Ok(())
}

/// `SUBSYSTEM_INFO`'s `NOT NULL` columns with no value here: the
/// shipped sign-on display (`*QDSIGNON`) has no library.
pub const SBS_NO_VALUE: &[&str] = &["SIGNON_DEVICE_FILE_LIBRARY"];

/// `JOB_QUEUE_INFO`: every job queue, its jobs counted by state and
/// priority, the subsystem whose job queue entry names it.
pub fn jobqs(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut entries: Vec<(String, String, String, String, wm::obj::sbsd::Jobqe)> = Vec::new();
    for (lib, name, id) in all_of(txn, ctx, types::SBSD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(s) = wm::obj::sbsd::Sbsd::decode(&bytes) else { continue };
        for e in s.jobqes {
            // The entry names the queue as name and library.
            entries.push((e.jobq.1.clone(), e.jobq.0.clone(), lib.clone(), name.clone(), e));
        }
    }
    let active = ctx.host.as_deref().and_then(super::host::SystemHost::active_jobs).unwrap_or_default();
    let now = ctx.clock();
    for (lib, name, id) in all_of(txn, ctx, types::JOBQ)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(q) = wm::obj::jobq::JobQ::decode(&bytes) else { continue };
        out.row();
        out.text("JOB_QUEUE_NAME", name.as_str());
        out.text("JOB_QUEUE_LIBRARY", lib.as_str());
        out.text("JOB_QUEUE_STATUS", if q.held { "HELD" } else { "RELEASED" });
        out.int("NUMBER_OF_JOBS", q.jobs.len());
        let mut held = [0u32; 10];
        let mut released = [0u32; 10];
        let mut scheduled = [0u32; 10];
        for j in &q.jobs {
            let p = usize::from(j.jobpty.min(9));
            if j.held {
                held[p] += 1;
            } else if j.scheduled > now {
                scheduled[p] += 1;
            } else {
                released[p] += 1;
            }
        }
        out.int("HELD_JOBS", held.iter().sum::<u32>());
        out.int("RELEASED_JOBS", released.iter().sum::<u32>());
        out.int("SCHEDULED_JOBS", scheduled.iter().sum::<u32>());
        for p in 0..10 {
            out.int(&format!("RELEASED_JOBS_PRIORITY_{p}"), released[p]);
            out.int(&format!("SCHEDULED_JOBS_PRIORITY_{p}"), scheduled[p]);
            out.int(&format!("HELD_JOBS_PRIORITY_{p}"), held[p]);
        }
        if let Some((_, _, slib, sname, e)) = entries.iter().find(|(l, n, ..)| *l == lib && *n == name) {
            out.text("SUBSYSTEM_NAME", sname.as_str());
            out.text("SUBSYSTEM_LIBRARY_NAME", slib.as_str());
            out.int("SEQUENCE_NUMBER", e.seq);
            if e.maxact != 0 {
                out.int("MAXIMUM_ACTIVE_JOBS", e.maxact);
            }
            for (p, m) in e.maxpty.iter().enumerate() {
                if *m != 0 {
                    out.int(&format!("MAXIMUM_ACTIVE_JOBS_PRIORITY_{}", p + 1), *m);
                }
            }
            let qual = format!("{lib}/{name}");
            let running = active.iter().filter(|a| a.attrs.as_ref().is_some_and(|at| at.jobq == qual)).count();
            out.int("ACTIVE_JOBS", running);
        }
        out.text_or_null("TEXT_DESCRIPTION", &q.text);
        out.text("OPERATOR_CONTROLLED", if q.oprctl { "*YES" } else { "*NO" });
        out.text("AUTHORITY_TO_CHECK", q.autchk.trim());
    }
    Ok(())
}

/// The active subsystems' monitor jobs: subsystem, qualified job,
/// jobs running in it.
fn monitors(ctx: &SqlContext) -> Vec<(String, String, usize)> {
    let jobs = ctx.host.as_deref().and_then(super::host::SystemHost::active_jobs).unwrap_or_default();
    jobs.iter()
        .filter(|a| a.job.kind == 'M')
        .map(|m| {
            let n = jobs.iter().filter(|a| a.job.kind != 'M' && a.job.subsystem.trim() == m.job.name.trim()).count();
            (m.job.name.trim().to_string(), m.job.qualified(), n)
        })
        .collect()
}

/// `SUBSYSTEM_INFO`: each subsystem description; active when its
/// monitor job runs.
pub fn subsystems(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mons = monitors(ctx);
    for (lib, name, id) in all_of(txn, ctx, types::SBSD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(s) = wm::obj::sbsd::Sbsd::decode(&bytes) else { continue };
        let mon = mons.iter().find(|(n, ..)| *n == name);
        out.row();
        out.text("SUBSYSTEM_DESCRIPTION_LIBRARY", lib.as_str());
        out.text("SUBSYSTEM_DESCRIPTION", name.as_str());
        out.text("STATUS", if mon.is_some() { "ACTIVE" } else { "INACTIVE" });
        if s.maxjobs != u32::MAX && s.maxjobs != 0 {
            out.int("MAXIMUM_ACTIVE_JOBS", s.maxjobs);
        }
        out.int("CURRENT_ACTIVE_JOBS", mon.map_or(0, |m| m.2));
        if let Some(m) = mon {
            out.text("SUBSYSTEM_MONITOR_JOB", m.1.as_str());
        }
        out.text_or_null("TEXT_DESCRIPTION", &s.text);
        out.yes("CONTROLLING_SUBSYSTEM", false);
        out.text_or_null("SIGNON_DEVICE_FILE_LIBRARY", &s.sgndspf.1);
        out.text("SIGNON_DEVICE_FILE", s.sgndspf.0.as_str());
        out.text_or_null("SECONDARY_LANGUAGE_LIBRARY", s.syslible.trim_start_matches("*NONE"));
    }
    Ok(())
}

/// `SUBSYSTEM_POOL_INFO`: each subsystem description's pools.
pub fn subsystem_pools(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, types::SBSD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(s) = wm::obj::sbsd::Sbsd::decode(&bytes) else { continue };
        for p in &s.pools {
            out.row();
            out.text("SUBSYSTEM_DESCRIPTION_LIBRARY", lib.as_str());
            out.text("SUBSYSTEM_DESCRIPTION", name.as_str());
            out.int("POOL_ID", p.id);
            out.text("POOL_NAME", pool_name(p));
            if p.actlvl != 0 {
                out.int("MAXIMUM_ACTIVE_JOBS", p.actlvl);
            }
            if p.size_kb != 0 {
                out.dec("POOL_SIZE", &format!("{}.{:02}", p.size_kb / 1024, (p.size_kb % 1024) * 100 / 1024));
            }
        }
    }
    Ok(())
}

/// `REPLY_LIST_INFO`: the system reply list.
#[allow(clippy::unnecessary_wraps)] // a view's rows may fail
pub fn reply_list(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let Ok(l) = wm::obj::rpyl::load(txn, ctx.qsys) else { return Ok(()) };
    for e in &l.entries {
        out.row();
        out.int("SEQUENCE_NUMBER", e.seq);
        out.text("MESSAGE_ID", e.msgid.as_str());
        out.text("MESSAGE_REPLY", e.reply.as_str());
        if !e.cmpdta.is_empty() && e.cmpdta != "*NONE" {
            out.text("COMPARISON_DATA", e.cmpdta.as_str());
            out.int("COMPARISON_DATA_OFFSET", e.start);
        }
        out.yes("DUMP_JOB", e.dump);
    }
    Ok(())
}

/// `MESSAGE_QUEUE_INFO`: the messages of every message queue the user
/// can read.
pub fn messages(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for (lib, name, id) in all_of(txn, ctx, types::MSGQ)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(q) = wm::obj::msgq::MsgQ::decode(&bytes) else { continue };
        for m in &q.msgs {
            out.row();
            out.text("MESSAGE_QUEUE_LIBRARY", lib.as_str());
            out.text("MESSAGE_QUEUE_NAME", name.as_str());
            out.text_or_null("MESSAGE_ID", &m.msgid);
            let kind = match m.kind.name() {
                "*INFO" => "INFORMATIONAL",
                "*INQ" => "INQUIRY",
                "*RPY" => "REPLY",
                "*COMP" => "COMPLETION",
                "*DIAG" => "DIAGNOSTIC",
                "*NOTIFY" => "NOTIFY",
                "*ESCAPE" => "ESCAPE",
                _ => "SENDER",
            };
            out.text("MESSAGE_TYPE", kind);
            out.text("MESSAGE_TEXT", m.text.as_str());
            out.int("SEVERITY", m.severity);
            out.stamp("MESSAGE_TIMESTAMP", m.time);
            out.set("MESSAGE_KEY", crate::value::Value::Bin(m.key.to_be_bytes().to_vec()));
            if m.reply_key != 0 {
                out.set("ASSOCIATED_MESSAGE_KEY", crate::value::Value::Bin(m.reply_key.to_be_bytes().to_vec()));
            }
            out.text("FROM_USER", m.job.1.as_str());
            out.text("FROM_JOB", format!("{:06}/{}/{}", m.job.2, m.job.1, m.job.0));
            out.text("FROM_PROGRAM", m.from_pgm.as_str());
            out.text_or_null("MESSAGE_FILE_LIBRARY", &m.msgf.1);
            out.text_or_null("MESSAGE_FILE_NAME", &m.msgf.0);
            out.text_or_null("MESSAGE_SECOND_LEVEL_TEXT", &m.help);
        }
    }
    Ok(())
}

fn date_of(yyyymmdd: u32) -> Option<db::record::datetime::Date> {
    db::record::datetime::Date::new(
        u16::try_from(yyyymmdd / 10_000).ok()?,
        u8::try_from(yyyymmdd / 100 % 100).ok()?,
        u8::try_from(yyyymmdd % 100).ok()?,
    )
}

/// `SCHEDULED_JOB_INFO`: the job schedule entries.
pub fn schedule(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    use wm::obj::jobscd::{DateKind, Frequency, RcyAcn};
    for (_, _, id) in all_of(txn, ctx, types::JOBSCD)? {
        let Ok(bytes) = wm::obj::read(txn, id) else { continue };
        let Ok(s) = wm::obj::jobscd::JobScd::decode(&bytes) else { continue };
        for e in &s.entries {
            out.row();
            out.int("SCHEDULED_JOB_ENTRY_NUMBER", e.number);
            out.text("SCHEDULED_JOB_ENTRY_NUMBER_CHAR", format!("{:06}", e.number));
            out.text("SCHEDULED_JOB_NAME", e.job.as_str());
            out.text(
                "SCHEDULED_DATE_VALUE",
                match e.date_kind {
                    DateKind::Date => "SCHEDULED_DATE",
                    DateKind::Current => "CURRENT",
                    DateKind::MonthStart => "MONTHSTART",
                    DateKind::MonthEnd => "MONTHEND",
                    DateKind::None => "NONE",
                },
            );
            if e.date_kind == DateKind::Date
                && let Some(d) = date_of(e.date)
            {
                out.set("SCHEDULED_DATE", crate::value::Value::Date(d));
            }
            if let Some(t) = db::record::datetime::Time::new(
                u8::try_from(e.time / 10_000).unwrap_or(0),
                u8::try_from(e.time / 100 % 100).unwrap_or(0),
                u8::try_from(e.time % 100).unwrap_or(0),
            ) {
                out.set("SCHEDULED_TIME", crate::value::Value::Time(t));
            }
            out.text(
                "FREQUENCY",
                match e.frequency {
                    Frequency::Once => "ONCE",
                    Frequency::Weekly => "WEEKLY",
                    Frequency::Monthly => "MONTHLY",
                },
            );
            out.text(
                "RECOVERY_ACTION",
                match e.rcyacn {
                    RcyAcn::SbmRls => "SBMRLS",
                    RcyAcn::SbmHld => "SBMHLD",
                    RcyAcn::NoSbm => "NOSBM",
                },
            );
            if e.next != 0 {
                out.set("NEXT_SUBMISSION_DATE", crate::value::Value::Date(db::record::datetime::Date::from_unix(e.next)));
            }
            out.text("STATUS", if e.held { "HELD" } else { "SCHEDULED" });
            let (ql, qn) = e.jobq.split_once('/').unwrap_or(("", e.jobq.as_str()));
            out.text_or_null("JOB_QUEUE_NAME", qn);
            out.text_or_null("JOB_QUEUE_LIBRARY_NAME", ql);
            out.text_or_null("DESCRIPTION", &e.text);
            out.text_or_null("COMMAND_STRING", &e.cmd);
            out.text_or_null("USER_PROFILE_FOR_SUBMITTED_JOB", &e.user);
            let (dl, dn) = e.jobd.split_once('/').unwrap_or(("", e.jobd.as_str()));
            out.text_or_null("JOB_DESCRIPTION_NAME", dn);
            out.text_or_null("JOB_DESCRIPTION_LIBRARY_NAME", dl);
            out.stamp("LAST_SUCCESSFUL_SUBMISSION_TIMESTAMP", e.last_submit);
            out.text_or_null("LAST_SUCCESSFUL_SUBMISSION_JOB", &e.last_job);
            out.yes("KEEP_ENTRY", e.save);
        }
    }
    Ok(())
}

/// A subsystem pool's name as the view shows it.
fn pool_name(p: &wm::obj::sbsd::Pool) -> String {
    use wm::obj::sbsd::PoolKind;
    match p.kind {
        PoolKind::Private => "PRIVATE".into(),
        PoolKind::Base => "*BASE".into(),
        PoolKind::Interact => "*INTERACT".into(),
        PoolKind::Spool => "*SPOOL".into(),
        PoolKind::Machine => "*MACHINE".into(),
        PoolKind::Shared(n) => format!("*SHRPOOL{n}"),
    }
}

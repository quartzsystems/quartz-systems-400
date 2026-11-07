// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ACTIVE_JOB_INFO`, `JOB_INFO` and `SYSTEM_STATUS_INFO`: the job table
//! and the figures `WRKACTJOB` and `WRKSYSSTS` show, as the storage
//! service has them.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::host::ActiveJob;
use super::{Args, Out, ask};
use crate::error::Result;

/// `SYSTEM_STATUS_INFO`'s `NOT NULL` columns QS/400 has no figure for:
/// partitions, single-level storage segments and addresses, which a
/// Linux machine does not have.
pub const STATUS_NO_VALUE: &[&str] = &[
    "JOBS_DISCONNECTED",
    "JOBS_SUSPENDED_BY_SYSREQ",
    "JOBS_SUSPENDED_BY_GROUP_JOB",
    "JOBS_PRINT_WAITING",
    "BATCH_PRINT_WAITING",
    "CURRENT_CPU_CAPACITY",
    "AVERAGE_CPU_RATE",
    "MINIMUM_CPU_UTILIZATION",
    "MAXIMUM_CPU_UTILIZATION",
    "PERMANENT_ADDRESS_RATE",
    "TEMPORARY_ADDRESS_RATE",
    "TEMPORARY_256MB_SEGMENTS",
    "TEMPORARY_4GB_SEGMENTS",
    "PERMANENT_256MB_SEGMENTS",
    "PERMANENT_4GB_SEGMENTS",
    "PARTITION_ID",
    "NUMBER_OF_PARTITIONS",
    "SERIAL_NUMBER",
    "MACHINE_SERIAL_NUMBER",
    "ATTENTION_LIGHT",
    "MACHINE_TYPE",
];

/// `lib/name` split; the library empty when there is none.
fn split(q: &str) -> (&str, &str) {
    match q.split_once('/') {
        Some((l, n)) => (l.trim(), n.trim()),
        None => ("", q.trim()),
    }
}

/// A list filter's names: comma or blank separated, upper case.
fn list(text: Option<String>) -> Option<Vec<String>> {
    let t = text?;
    let v: Vec<String> = t.split([',', ' ']).map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect();
    if v.is_empty() || v.iter().any(|s| s == "*ALL") { None } else { Some(v) }
}

/// Whether `name` matches a name or generic (`ABC*`) filter.
fn matches(filter: &str, name: &str) -> bool {
    match filter.strip_suffix('*') {
        Some(p) => name.starts_with(p),
        None => filter == name,
    }
}

/// The job's `WRKACTJOB` type.
fn job_type(j: &wm::table::JobInfo) -> &'static str {
    j.job_type().map_or("SYS", |t| wm::job::wrkactjob_type(t, j.sub_type()))
}

/// `JOB_TYPE_ENHANCED` from the type.
fn enhanced(t: &str) -> &'static str {
    match t {
        "ASJ" => "AUTOSTART",
        "BCH" => "BATCH",
        "BCI" => "BATCH_IMMEDIATE",
        "EVK" => "EVOKED",
        "INT" => "INTERACTIVE",
        "M36" | "MRT" => "MRT",
        "PJ" => "PRESTART",
        "PDJ" => "PRINT_DRIVER",
        "RDR" => "SPOOL_READER",
        "SBS" => "SUBSYSTEM",
        "WTR" => "SPOOL_WRITER",
        _ => "SYSTEM",
    }
}

fn internal_id(j: &wm::table::JobInfo) -> Vec<u8> {
    wm::job::JobId::new(&j.name, &j.user, j.number).map_or_else(Vec::new, |id| id.internal(j.started).0.to_vec())
}

/// A `*NOMAX` limit is null, as IBM shows it.
fn limit(out: &mut Out, col: &str, v: u32) {
    if v != u32::MAX {
        out.int(col, v);
    }
}

/// The columns every job row shares.
fn common(out: &mut Out, aj: &ActiveJob) {
    let job = &aj.job;
    out.text("JOB_NAME", job.qualified());
    out.text("JOB_NAME_SHORT", job.name.as_str());
    out.text("JOB_USER", job.user.as_str());
    out.text("JOB_NUMBER", format!("{:06}", job.number));
    out.set("INTERNAL_JOB_ID", crate::value::Value::Bin(internal_id(job)));
    let ty = job_type(job);
    out.text("JOB_TYPE", ty);
    out.text("JOB_TYPE_ENHANCED", enhanced(ty));
    out.stamp("JOB_ENTERED_SYSTEM_TIME", job.started);
    out.stamp("JOB_ACTIVE_TIME", job.started);
    if let Some(at) = &aj.attrs {
        let (lib, name) = split(&at.jobd);
        out.text_or_null("JOB_DESCRIPTION_LIBRARY", lib);
        out.text_or_null("JOB_DESCRIPTION", name);
        out.int("CCSID", at.ccsid);
        out.int("DEFAULT_WAIT", at.dftwait.min(i32::MAX.unsigned_abs()));
        limit(out, "TIME_SLICE", at.timeslice);
        out.text("LANGUAGE_ID", at.langid.as_str());
        out.text("DATE_FORMAT", at.datfmt.as_str());
        out.text("DATE_SEPARATOR", at.datsep.as_str());
        out.text("TIME_SEPARATOR", at.timsep.as_str());
        out.text(
            "DECIMAL_FORMAT",
            match at.decfmt.trim() {
                "J" => "J",
                "I" => "I",
                _ => "*BLANK",
            },
        );
    }
}

/// `ACTIVE_JOB_INFO(RESET_STATISTICS, SUBSYSTEM_LIST_FILTER,
/// JOB_NAME_FILTER, CURRENT_USER_LIST_FILTER, DETAILED_INFO)`.
#[allow(clippy::many_single_char_names)]
pub fn active(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let jobs = ask(txn, ctx, "the active jobs", |h| h.active_jobs())?;
    let sbs = list(args.text("SUBSYSTEM_LIST_FILTER"));
    let users = list(args.text("CURRENT_USER_LIST_FILTER"));
    let name = args.word("JOB_NAME_FILTER").filter(|n| n != "*ALL");
    let detail = args.word("DETAILED_INFO").unwrap_or_else(|| "NONE".into());
    let me = u32::try_from(ctx.job.0).unwrap_or(0);
    let mut n = 0;
    for a in &jobs {
        let j = &a.job;
        if sbs.as_ref().is_some_and(|l| !l.iter().any(|s| s == j.subsystem.trim())) {
            continue;
        }
        if users.as_ref().is_some_and(|l| !l.iter().any(|u| u == j.user.trim())) {
            continue;
        }
        let t = job_type(j);
        let keep = match name.as_deref() {
            None => true,
            Some("*" | "*CURRENT") => j.number == me,
            Some("*SBS") => t == "SBS",
            Some("*SYS") => t == "SYS",
            Some("*SBS*SYS" | "*SYS*SBS") => t == "SBS" || t == "SYS",
            Some(f) => matches(f, j.name.trim()),
        };
        if !keep {
            continue;
        }
        n += 1;
        out.row();
        out.int("ORDINAL_POSITION", n);
        common(out, a);
        if !j.subsystem.trim().is_empty() {
            out.text("SUBSYSTEM", j.subsystem.trim());
            out.text("SUBSYSTEM_LIBRARY_NAME", "QSYS");
        }
        out.text("AUTHORIZATION_NAME", j.user.as_str());
        if let Some((ty, f)) = j.function.split_once('-') {
            out.text("FUNCTION_TYPE", ty);
            out.text("FUNCTION", f);
        }
        out.text("JOB_STATUS", j.job_status().code());
        out.text_or_null("MEMORY_POOL", j.pool.trim_start_matches('*'));
        out.int("RUN_PRIORITY", a.attrs.as_ref().map_or(0, |at| at.runpty));
        out.int("THREAD_COUNT", a.threads.max(1));
        out.int("TEMPORARY_STORAGE", a.temp_mb);
        out.int("CPU_TIME", a.cpu_ms);
        out.int("TOTAL_DISK_IO_COUNT", a.aux_io);
        if matches!(t, "INT" | "M36") {
            out.int("ELAPSED_INTERACTION_COUNT", a.transactions);
            out.int("ELAPSED_TOTAL_RESPONSE_TIME", a.response_ms / 1000);
        }
        out.int("ELAPSED_TOTAL_DISK_IO_COUNT", a.elapsed_aux_io);
        out.dec("ELAPSED_CPU_PERCENTAGE", &format!("{}.{:02}", a.cpu_tenths / 10, (a.cpu_tenths % 10) * 10));
        out.dec("ELAPSED_TIME", &format!("{}.{:03}", a.elapsed_ms / 1000, a.elapsed_ms % 1000));
        if let Some(at) = &a.attrs {
            let (l, q) = split(&at.jobq);
            out.text_or_null("JOB_QUEUE_LIBRARY", l);
            out.text_or_null("JOB_QUEUE", q);
            let (l, q) = split(&at.outq);
            out.text_or_null("OUTPUT_QUEUE_LIBRARY", l);
            out.text_or_null("OUTPUT_QUEUE", q);
            out.int("DEFAULT_CCSID", at.dftccsid);
            let (l, s) = split(&at.srtseq);
            out.text_or_null("SORT_SEQUENCE_LIBRARY", l);
            out.text_or_null("SORT_SEQUENCE", s);
            limit(out, "MAXIMUM_ACTIVE_THREADS", at.maxthd);
            limit(out, "MAXIMUM_PROCESSING_TIME_ALLOWED", at.cputime);
            limit(out, "MAXIMUM_TEMPORARY_STORAGE_ALLOWED", at.maxtmpstg);
        }
        if a.pool_number != 0 {
            out.int("SYSTEM_POOL_ID", a.pool_number);
        }
        if detail != "NONE" {
            out.text_or_null("POOL_NAME", &j.pool);
        }
    }
    Ok(())
}

/// `JOB_INFO(JOB_STATUS_FILTER, JOB_TYPE_FILTER, JOB_SUBSYSTEM_FILTER,
/// JOB_USER_FILTER, JOB_SUBMITTER_FILTER, JOB_NAME_FILTER)`: the active
/// jobs; jobs on job queues and jobs with output only are not in the
/// job table the service keeps (BLOCKED(rel): no table of ended jobs).
#[allow(clippy::many_single_char_names)]
pub fn job_info(txn: &mut Txn<'_>, ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let status = args.word("JOB_STATUS_FILTER").unwrap_or_else(|| "*ALL".into());
    if !matches!(status.as_str(), "*ALL" | "*ACTIVE") {
        return Ok(());
    }
    let jobs = ask(txn, ctx, "the active jobs", |h| h.active_jobs())?;
    let ty = args.word("JOB_TYPE_FILTER").unwrap_or_else(|| "*ALL".into());
    let sbs = args.word("JOB_SUBSYSTEM_FILTER").unwrap_or_else(|| "*ALL".into());
    let user = args.word("JOB_USER_FILTER").unwrap_or_else(|| ctx.user.clone());
    let name = args.word("JOB_NAME_FILTER").unwrap_or_else(|| "*ALL".into());
    for a in &jobs {
        let j = &a.job;
        let t = job_type(j);
        let type_ok = match ty.as_str() {
            "*ALL" => true,
            "*BATCH" => matches!(t, "BCH" | "BCI" | "EVK" | "PJ" | "MRT"),
            "*INTERACT" => matches!(t, "INT" | "M36"),
            "*AUTO" => t == "ASJ",
            "*SBS" => t == "SBS",
            "*SYSTEM" => t == "SYS",
            "*READER" => t == "RDR",
            "*WRITER" => matches!(t, "WTR" | "PDJ"),
            _ => false,
        };
        let user_ok = user == "*ALL" || (user == "*USER" && j.user.trim() == ctx.user.trim()) || matches(&user, j.user.trim());
        let name_ok = name == "*ALL" || matches(&name, j.name.trim());
        let sbs_ok = sbs == "*ALL" || sbs == j.subsystem.trim();
        if !(type_ok && user_ok && name_ok && sbs_ok) {
            continue;
        }
        out.row();
        common(out, a);
        out.text("JOB_INFORMATION", "YES");
        out.text("JOB_STATUS", "ACTIVE");
        out.text_or_null("JOB_SUBSYSTEM", &j.subsystem);
        if let Some(at) = &a.attrs {
            let (l, q) = split(&at.jobq);
            out.text_or_null("JOB_QUEUE_LIBRARY", l);
            out.text_or_null("JOB_QUEUE_NAME", q);
            out.int("JOB_QUEUE_PRIORITY", at.jobpty);
            out.int("JOB_MESSAGE_QUEUE_MAXIMUM_SIZE", at.jobmsgqmx);
            out.text_or_null("JOB_MESSAGE_QUEUE_FULL_ACTION", &at.jobmsgqfl);
            out.yes("ALLOW_MULTIPLE_THREADS", at.alwmltthd);
            limit(out, "MAXIMUM_PROCESSING_TIME_ALLOWED", at.cputime);
            limit(out, "MAXIMUM_TEMPORARY_STORAGE_ALLOWED", at.maxtmpstg);
            out.text_or_null("JOB_SWITCHES", &at.sws);
            out.text_or_null("ROUTING_DATA", &at.rtgdta);
            out.text_or_null("COUNTRY_ID", &at.cntryid);
            let (l, s) = split(&at.srtseq);
            out.text_or_null("SORT_SEQUENCE_LIBRARY", l);
            out.text_or_null("SORT_SEQUENCE_NAME", s);
            out.int("MESSAGE_LOGGING_LEVEL", at.log_level);
            out.int("MESSAGE_LOGGING_SEVERITY", at.log_sev);
            out.text_or_null("MESSAGE_LOGGING_TEXT", &at.log_text);
            out.text_or_null("LOG_CL_PROGRAM_COMMANDS", &at.logclpgm);
            out.text_or_null("STATUS_MESSAGE", &at.stsmsg);
            out.text_or_null("INQUIRY_MESSAGE_REPLY", &at.inqmsgrpy);
            out.text_or_null("BREAK_MESSAGE", &at.brkmsg);
            out.text_or_null("JOB_LOG_OUTPUT", &at.logoutput);
            out.int("OUTPUT_QUEUE_PRIORITY", at.outpty);
            let (l, q) = split(&at.outq);
            out.text_or_null("OUTPUT_QUEUE_LIBRARY", l);
            out.text_or_null("OUTPUT_QUEUE_NAME", q);
            out.text_or_null("SPOOLED_FILE_ACTION", &at.splfacn);
            out.text_or_null("PRINTER_DEVICE_NAME", &at.prtdev);
            out.text_or_null("PRINT_KEY_FORMAT", &at.prtkeyfmt);
            out.text_or_null("PRINT_TEXT", &at.prttxt);
            out.text_or_null("DEVICE_RECOVERY_ACTION", &at.devrcyacn);
            out.text_or_null("DDM_CONVERSATION", &at.ddmcnv);
            out.int("JOB_END_SEVERITY", at.endsev);
        }
    }
    Ok(())
}

/// `SYSTEM_STATUS_INFO` and `SYSTEM_STATUS_INFO_BASIC`.
pub fn status(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let s = ask(txn, ctx, "the system status", |h| h.system_status())?;
    out.row();
    out.int("TOTAL_JOBS_IN_SYSTEM", s.total_jobs);
    out.int("MAXIMUM_JOBS_IN_SYSTEM", s.max_jobs);
    out.int("ACTIVE_JOBS_IN_SYSTEM", s.active_jobs);
    out.float("INTERACTIVE_JOBS_IN_SYSTEM", s.interactive_pct, 2);
    out.int("JOBS_SIGNED_ON", s.signed_on);
    for (col, v) in [
        "BATCH_RUNNING",
        "BATCH_WAITING_TO_RUN",
        "BATCH_ENDING",
        "BATCH_MESSAGE_WAIT",
        "BATCH_HELD_WHILE_RUNNING",
        "BATCH_HELD_ON_JOBQ",
        "BATCH_ON_HELD_JOBQ",
        "BATCH_ON_UNASSIGNED_JOBQ",
    ]
    .iter()
    .zip(s.batch)
    {
        out.int(col, v);
    }
    out.int("ELAPSED_TIME", s.elapsed_ms / 1000);
    out.float("ELAPSED_CPU_USED", s.cpu_pct, 2);
    out.int("CONFIGURED_CPUS", s.cpus);
    out.float("AVERAGE_CPU_UTILIZATION", s.cpu_pct, 2);
    out.int("MAIN_STORAGE_SIZE", s.main_storage_kb);
    out.int("SYSTEM_ASP_STORAGE", s.asp_mb);
    out.int("TOTAL_AUXILIARY_STORAGE", s.asp_mb);
    out.float("SYSTEM_ASP_USED", s.asp_used_pct, 2);
    out.int("CURRENT_TEMPORARY_STORAGE", s.temp_mb);
    out.int("MAXIMUM_TEMPORARY_STORAGE_USED", s.temp_max_mb);
    let in_use = s.active_jobs + s.table[0] + s.table[1] + s.table[2];
    out.int("TOTAL_JOB_TABLE_ENTRIES", in_use);
    out.int("AVAILABLE_JOB_TABLE_ENTRIES", s.max_jobs.saturating_sub(in_use));
    out.int("IN_USE_JOB_TABLE_ENTRIES", in_use);
    out.int("ACTIVE_JOB_TABLE_ENTRIES", s.active_jobs);
    out.int("JOBQ_JOB_TABLE_ENTRIES", s.table[0]);
    out.int("OUTQ_JOB_TABLE_ENTRIES", s.table[1]);
    out.int("JOBLOG_PENDING_JOB_TABLE_ENTRIES", s.table[2]);
    out.text("HOST_NAME", s.host_name.as_str());
    out.int("ACTIVE_THREADS_IN_SYSTEM", s.threads);
    out.yes("RESTRICTED_STATE", s.restricted);
    out.text_or_null("MACHINE_TYPE", &s.machine.0);
    out.text("MACHINE_MODEL", s.machine.1.as_str());
    if !s.machine.2.is_empty() {
        out.text("SERIAL_NUMBER", s.machine.2.as_str());
        out.text("MACHINE_SERIAL_NUMBER", s.machine.2.as_str());
    }
    out.text("IPL_MODE", s.ipl_mode.as_str());
    out.text("IPL_TYPE", s.ipl_type.as_str());
    out.text("JOB_SCHEDULE_STATUS", if s.scheduler_active { "ACTIVE" } else { "INACTIVE" });
    Ok(())
}

/// `SYSTEM_STATUS(RESET_STATISTICS, DETAILED_INFO)`: the view's row.
/// Statistics are reset by `WRKSYSSTS`'s own interval, not here.
pub fn status_function(txn: &mut Txn<'_>, ctx: &SqlContext, _args: &Args, out: &mut Out) -> Result<()> {
    status(txn, ctx, out)
}

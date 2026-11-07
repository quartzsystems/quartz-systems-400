// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Jobs` service (version 8, draft) on a connection job's
//! secondary thread, as the signed-on profile. `ListJobs` reads what
//! `WRKUSRJOB` reads: the active job table, the job queues' entries and
//! the jobs that left spooled files. `JobLog` is the service's copy of
//! an active job's log (`joblog_of`, its own user or `*JOBCTL`) or an
//! ended job's spooled `QPJOBLOG` read back (`wm::joblog::parse_printed`).
//! `HoldJob`, `ReleaseJob` and `EndJob` are `HLDJOB`, `RLSJOB` and
//! `ENDJOB` analyzed as typed lines and run in the service, with their
//! checks and audit entries.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use qap_proto::ops::jobs::{EndJob, JobEntry, JobLogEntry, JobState, ListJobs};
use qap_proto::ops::messages::MessageType;
use qap_proto::types::{EndHow, JobId};
use qap_proto::{ServerMessage, Status};
use qapd::Refusal;
use sec::profile::SpecialAuth;
use stg::{Client, JobInfo};
use wm::obj::jobq::JobQ;

use super::develop::command;
use super::manage::Ctx;

fn refuse(cx: &Ctx<'_>, status: Status, id: &str, data: &[&str]) -> Refusal {
    let message = super::host::message(&mut cx.shared.borrow_mut(), id, data);
    Refusal { status, message }
}

/// The job type as `WRKUSRJOB` shows it.
fn type_of(j: &JobInfo) -> String {
    let code = j
        .job_type()
        .map_or("", |t| wm::job::wrkactjob_type(t, j.sub_type()))
        .trim()
        .to_string();
    match code.as_str() {
        "INT" => "INTER".into(),
        "BCH" | "BCI" | "EVK" => "BATCH".into(),
        "ASJ" => "AUTO".into(),
        _ => code,
    }
}

fn id_of(name: &str, user: &str, number: u32) -> JobId {
    JobId {
        name: name.to_string(),
        user: user.to_string(),
        number: format!("{number:06}"),
    }
}

fn micros_ms(us: u64) -> i64 {
    i64::try_from(us / 1000).unwrap_or(i64::MAX)
}

/// Every job of `user` (`None` for all), as `WRKUSRJOB` lists them.
fn jobs_of(cx: &Ctx<'_>, user: Option<&str>) -> Vec<JobEntry> {
    let mine = |u: &str| user.is_none_or(|w| w == u);
    let all = cx.shared.borrow_mut().with(Client::active_jobs);
    let mut active: Vec<JobEntry> = all
        .iter()
        .filter(|j| mine(&j.user) && j.kind != 'S')
        .map(|j| JobEntry {
            id: id_of(&j.name, &j.user, j.number),
            state: JobState::Active,
            status: "*ACTIVE".into(),
            r#type: type_of(j),
            subsystem: j.subsystem.trim().to_string(),
            function: j.function.trim().to_string(),
            entered: i64::try_from(j.started).unwrap_or(0).saturating_mul(1000),
        })
        .collect();
    active.sort_by_key(|e| e.entered);
    let mut waiting = Vec::new();
    let queues = cx
        .shared
        .borrow_mut()
        .with(|l| l.list_objects("*ALL".into(), "*ALL".into(), "*JOBQ".into()));
    for o in queues {
        let q = format!("{}/{}", o.lib, o.name);
        let Ok(data) = cx
            .shared
            .borrow_mut()
            .with(|l| l.wm_object(q, "*JOBQ".into()))
        else {
            continue;
        };
        let Ok(jq) = JobQ::decode(&data) else {
            continue;
        };
        for j in jq.jobs.iter().filter(|j| mine(&j.user)) {
            waiting.push(JobEntry {
                id: id_of(&j.name, &j.user, j.number),
                state: JobState::OnJobQueue,
                status: "*JOBQ".into(),
                r#type: "BATCH".into(),
                subsystem: String::new(),
                function: String::new(),
                entered: micros_ms(j.submitted),
            });
        }
    }
    waiting.sort_by_key(|e| e.entered);
    // Ended jobs whose spooled files are still on the system.
    let files = cx
        .shared
        .borrow_mut()
        .with(|l| l.splf_list(user.unwrap_or("").to_string(), String::new()));
    let mut ended: BTreeMap<u32, JobEntry> = BTreeMap::new();
    for f in files {
        let (name, owner, number) = f.job.clone();
        let seen = active
            .iter()
            .chain(waiting.iter())
            .any(|j| j.id.number == format!("{number:06}"));
        if !mine(&owner) || seen {
            continue;
        }
        let at = micros_ms(f.created);
        let e = ended.entry(number).or_insert_with(|| JobEntry {
            r#type: if name.starts_with("QPADEV") {
                "INTER".into()
            } else {
                "BATCH".into()
            },
            id: id_of(&name, &owner, number),
            state: JobState::OnOutputQueue,
            status: "*OUTQ".into(),
            subsystem: String::new(),
            function: String::new(),
            entered: at,
        });
        e.entered = e.entered.min(at);
    }
    let mut ended: Vec<JobEntry> = ended.into_values().collect();
    ended.sort_by_key(|e| e.entered);
    active.into_iter().chain(waiting).chain(ended).collect()
}

/// `ListJobs`: the profile's own; another's or `*ALL` with `*JOBCTL`
/// (`CPF1343`); an unknown profile `CPF2204`.
pub(super) fn list_jobs(cx: &Ctx<'_>, req: &ListJobs) -> Result<Vec<JobEntry>, Refusal> {
    let me = cx.shared.borrow_mut().with(Client::whoami);
    let user = match req.user.trim().to_ascii_uppercase().as_str() {
        "" | "*CURRENT" => me.user.clone(),
        u => u.to_string(),
    };
    if user != "*ALL" {
        let u = user.clone();
        if cx.shared.borrow_mut().with(|l| l.user_info(u)).is_none() {
            return Err(refuse(cx, Status::NotFound, "CPF2204", &[&user]));
        }
    }
    if user != me.user && !me.has(SpecialAuth::JOBCTL) {
        return Err(refuse(
            cx,
            Status::NotAuthorized,
            "CPF1343",
            &[&me.user, &user],
        ));
    }
    let all = jobs_of(cx, (user != "*ALL").then_some(user.as_str()));
    Ok(all
        .into_iter()
        .filter(|j| req.status.is_none_or(|s| s == j.state))
        .collect())
}

/// A job log entry's type as service 7 names it.
fn kind_of(k: &str) -> MessageType {
    match k {
        "Escape" => MessageType::Escape,
        "Diagnostic" => MessageType::Diagnostic,
        "Completion" => MessageType::Completion,
        "Inquiry" => MessageType::Inquiry,
        "Reply" => MessageType::Reply,
        "Notify" => MessageType::Notify,
        // Information, requests and status messages.
        _ => MessageType::Informational,
    }
}

fn entry_of(e: &wm::joblog::Entry) -> JobLogEntry {
    JobLogEntry {
        message: ServerMessage {
            id: e.msgid.clone(),
            severity: u8::try_from(e.severity).unwrap_or(99),
            data: Vec::new(),
            text: e.text.clone(),
            help: e.help.clone(),
        },
        kind: kind_of(&e.kind),
        sent: micros_ms(e.time),
        from_program: e.from.0.trim().to_string(),
        to_program: e.to.0.trim().to_string(),
    }
}

/// `JobLog`: an active job's log, or an ended one's spooled
/// `QPJOBLOG`; `CPF1321` when it has neither.
pub(super) fn job_log(cx: &Ctx<'_>, job: &JobId) -> Result<Vec<JobLogEntry>, Refusal> {
    let number: u32 = job.number.trim().parse().unwrap_or(0);
    let (name, user) = (
        job.name.trim().to_ascii_uppercase(),
        job.user.trim().to_ascii_uppercase(),
    );
    let all = cx.shared.borrow_mut().with(Client::active_jobs);
    if all
        .iter()
        .any(|j| j.number == number && j.name == name && j.user == user)
    {
        let r = cx.shared.borrow_mut().with(|l| l.joblog_of(number));
        return match r {
            Ok(entries) => Ok(entries.iter().map(entry_of).collect()),
            Err(e) => {
                let id = e.split_whitespace().next().unwrap_or("CPF1343");
                let status = if id == "CPF1321" {
                    Status::NotFound
                } else {
                    Status::NotAuthorized
                };
                Err(refuse(cx, status, id, &[&user, &name]))
            }
        };
    }
    let me = cx.shared.borrow_mut().with(Client::whoami);
    if user != me.user && !me.has(SpecialAuth::JOBCTL) {
        return Err(refuse(
            cx,
            Status::NotAuthorized,
            "CPF1343",
            &[&me.user, &name],
        ));
    }
    let qual = format!("{number:06}/{user}/{name}");
    let r = cx
        .shared
        .borrow_mut()
        .with(|l| l.splf_fetch("QPJOBLOG".into(), qual, "*LAST".into()));
    let Ok((_, data)) = r else {
        return Err(refuse(cx, Status::NotFound, "CPF1321", &[&name]));
    };
    let (pages, _) = spool::scs::decode(&data);
    let text = spool::view::text(&pages);
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_start_matches('\u{c}'))
        .collect();
    Ok(wm::joblog::parse_printed(&lines)
        .iter()
        .map(entry_of)
        .collect())
}

fn qualified(j: &JobId) -> String {
    format!(
        "{}/{}/{}",
        j.number.trim(),
        j.user.trim().to_ascii_uppercase(),
        j.name.trim().to_ascii_uppercase()
    )
}

/// `HoldJob` and `ReleaseJob`: `HLDJOB` and `RLSJOB`.
pub(super) fn hold_job(cx: &Ctx<'_>, job: &JobId, hold: bool) -> Result<(), Refusal> {
    let verb = if hold { "HLDJOB" } else { "RLSJOB" };
    command(cx, &format!("{verb} JOB({})", qualified(job)))
}

/// `EndJob`: `ENDJOB OPTION() DELAY()`.
pub(super) fn end_job(cx: &Ctx<'_>, req: &EndJob) -> Result<(), Refusal> {
    let mut line = format!("ENDJOB JOB({})", qualified(&req.job));
    match req.how {
        EndHow::Controlled => line.push_str(" OPTION(*CNTRLD)"),
        EndHow::Immediate => line.push_str(" OPTION(*IMMED)"),
    }
    if let Some(d) = req.delay {
        let _ = write!(line, " DELAY({d})");
    }
    command(cx, &line)
}

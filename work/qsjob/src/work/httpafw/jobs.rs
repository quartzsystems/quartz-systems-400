// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::jobs::Jobs` as the user, as `WRKUSRJOB` reads them:
//! the active job table (`active_jobs`), the job queues' entries
//! (`list_objects` and `wm_object *JOBQ`) and the jobs that left
//! spooled files (`splf_list`); the server jobs are the instance's own
//! (`ACCESSWEB` in `QHTTPSVR`) and the Telnet server's 5250 sessions.
//! Hold and Release are commands run as the user.

use std::collections::BTreeMap;

use accessweb::env::jobs::{Jobs, Stage, UserJob};
use qsweb::msg::Message;
use stg::{Client, JobInfo};
use wm::obj::jobq::JobQ;

use super::Link;
use crate::work::web::bridge::Main;

/// Jobs.
pub struct LiveJobs {
    /// The job's thread.
    pub link: Link,
}

/// What My jobs calls a job's type.
fn type_word(j: &JobInfo) -> String {
    let code = j
        .job_type()
        .map_or("", |t| wm::job::wrkactjob_type(t, j.sub_type()))
        .trim()
        .to_string();
    match code.as_str() {
        "INT" => "Interactive".into(),
        "BCH" | "BCI" | "EVK" => "Batch".into(),
        "PJ" => "Prestart".into(),
        "ASJ" => "Autostart".into(),
        "WTR" => "Writer".into(),
        "SBS" => "Subsystem monitor".into(),
        "SYS" => "System".into(),
        _ => code,
    }
}

fn active(j: &JobInfo, kind: String, serving: &str) -> UserJob {
    UserJob {
        name: j.name.clone(),
        user: j.user.clone(),
        number: format!("{:06}", j.number),
        kind,
        status: j.job_status().code().trim().to_string(),
        stage: Stage::Active,
        subsystem: j.subsystem.trim().to_string(),
        function: j.function.trim().to_string(),
        entered: j.started,
        serving: serving.to_string(),
    }
}

/// `user`'s jobs waiting on the job queues it may read.
fn queued(m: &Main, user: &str) -> Vec<UserJob> {
    let queues = m.ask(|l| l.list_objects("*ALL".into(), "*ALL".into(), "*JOBQ".into()));
    let mut out = Vec::new();
    for o in queues {
        let q = format!("{}/{}", o.lib, o.name);
        let Ok(data) = m.ask(|l| l.wm_object(q, "*JOBQ".into())) else {
            continue;
        };
        let Ok(jq) = JobQ::decode(&data) else {
            continue;
        };
        for j in jq.jobs.iter().filter(|j| j.user == user) {
            out.push(UserJob {
                name: j.name.clone(),
                user: j.user.clone(),
                number: format!("{:06}", j.number),
                kind: "Batch".into(),
                status: if j.held || jq.held {
                    "HLD".into()
                } else {
                    "JOBQ".into()
                },
                stage: Stage::OnQueue,
                entered: j.submitted / 1_000_000,
                ..UserJob::default()
            });
        }
    }
    out
}

fn user_jobs(m: &Main, user: &str) -> Vec<UserJob> {
    let all = m.ask(Client::active_jobs);
    let mut out: Vec<UserJob> = all
        .iter()
        .filter(|j| j.user == user && j.kind != 'S')
        .map(|j| active(j, type_word(j), ""))
        .collect();
    out.extend(queued(m, user));
    // Ended jobs whose printer output is still on the system.
    let u = user.to_string();
    let files = m.ask(|l| l.splf_list(u, String::new()));
    let mut ended: BTreeMap<u32, UserJob> = BTreeMap::new();
    for f in files {
        let (name, owner, number) = f.job.clone();
        if owner != user
            || all.iter().any(|j| j.number == number)
            || out.iter().any(|j| j.number == format!("{number:06}"))
        {
            continue;
        }
        let at = f.created / 1_000_000;
        let e = ended.entry(number).or_insert_with(|| UserJob {
            kind: if name.starts_with("QPADEV") {
                "Interactive".into()
            } else {
                "Batch".into()
            },
            name,
            user: owner,
            number: format!("{number:06}"),
            status: "OUTQ".into(),
            stage: Stage::Completed,
            entered: at,
            ..UserJob::default()
        });
        e.entered = e.entered.min(at);
    }
    out.extend(ended.into_values().rev());
    out
}

impl Jobs for LiveJobs {
    fn user_jobs(&self, user: &str) -> Vec<UserJob> {
        let u = user.to_string();
        self.link.read(user, move |m| user_jobs(m, &u))
    }

    fn server_jobs(&self, user: &str) -> Vec<UserJob> {
        self.link.read(user, |m| {
            let mut all = m.ask(Client::active_jobs);
            all.sort_by_key(|j| j.number);
            let code = |j: &JobInfo| {
                j.job_type()
                    .map_or("", |t| wm::job::wrkactjob_type(t, j.sub_type()))
                    .trim()
                    .to_string()
            };
            all.iter()
                .filter_map(|j| {
                    let serving = if j.name == accessweb::SELF_INSTANCE
                        && j.subsystem.trim() == "QHTTPSVR"
                        && j.kind != 'M'
                    {
                        format!(
                            "Access for Web (port {}/{})",
                            accessweb::PORT,
                            accessweb::TLS_PORT
                        )
                    } else if j.name == "QTVTELNET" {
                        "5250 session (telnet)".to_string()
                    } else {
                        return None;
                    };
                    Some(active(j, code(j), &serving))
                })
                .collect()
        })
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

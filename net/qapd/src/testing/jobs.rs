// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's jobs (`Jobs`, version 8, draft) and the catalog's
//! routines and indexes (`Sql` 12 and 13), under the commands' rules:
//!
//! - `BOB`'s jobs: `QPADEV0001` active in `QINTER`, `NIGHTLY` on job
//!   queue `QBATCH`, `BUILD` ended with a spooled job log, `OLDJOB`
//!   ended without one (`JobLog` is `NotFound`, `CPF1321`); `ANN`'s
//!   `PAYRUN` active in `QBATCH`. Another user's job needs `*JOBCTL`
//!   (`CPF1343`), as `*ALL` does; an unknown profile is `CPF2204`.
//! - `HLDJOB` and `RLSJOB` of an ended job, and `ENDJOB` of one, are
//!   `CPF1362`; an ended job leaves the list it was in for the output
//!   queue's.
//! - The catalog: `HLINV` has the procedure `RESTOCK` (two, specific
//!   names `RESTOCK` and `RESTOCK2`) and the function `ASSETAGE`; table
//!   `HLINV.ASSETS` has the unique index `ASSETSI1` (`ASSETID`) and
//!   `ASSETSL1` (`LOCATION`, `ASSETID`); `HLPAY.SALARIES`'s indexes are
//!   `ANN`'s alone (`SQL0551`). A schema not there is `SQL0204`.

use qap_proto::ServerMessage;
use qap_proto::Status;
use qap_proto::ops::jobs::{EndJob, JobEntry, JobLogEntry, JobState, ListJobs};
use qap_proto::ops::messages::MessageType;
use qap_proto::ops::sql::{Index, Indexes, ListIndexes, ListRoutines, Routine, RoutineKind};
use qap_proto::types::{EndHow, JobId};

use super::spool::No;
use crate::sql::like;

fn no(status: Status, id: &'static str, data: &[&str]) -> No {
    (status, id, data.iter().map(ToString::to_string).collect())
}

/// One job.
#[derive(Clone, Debug)]
pub struct Job {
    /// As the list shows it.
    pub entry: JobEntry,
    /// Held.
    pub held: bool,
    /// Its log; `None` for an ended job without a spooled one.
    pub log: Option<Vec<JobLogEntry>>,
    /// The `ENDJOB` it got, if any.
    pub ended: Option<(EndHow, Option<u32>)>,
}

/// The test system's jobs and catalog routines.
#[derive(Default)]
pub struct Jobs {
    /// The jobs.
    pub jobs: Vec<Job>,
}

const T0: i64 = 1_793_000_000_000;

fn entry(
    (name, user, number): (&str, &str, &str),
    state: JobState,
    ty: &str,
    sbs: &str,
    function: &str,
    entered: i64,
) -> JobEntry {
    JobEntry {
        id: JobId {
            name: name.into(),
            user: user.into(),
            number: number.into(),
        },
        state,
        status: match state {
            JobState::Active => "*ACTIVE",
            JobState::OnJobQueue => "*JOBQ",
            JobState::OnOutputQueue => "*OUTQ",
        }
        .into(),
        r#type: ty.into(),
        subsystem: sbs.into(),
        function: function.into(),
        entered,
    }
}

fn logged(id: &str, kind: MessageType, text: &str, sent: i64) -> JobLogEntry {
    JobLogEntry {
        message: ServerMessage {
            id: id.into(),
            severity: if kind == MessageType::Escape { 40 } else { 0 },
            data: Vec::new(),
            text: text.into(),
            help: String::new(),
        },
        kind,
        sent,
        from_program: "QCMD".into(),
        to_program: "QCMD".into(),
    }
}

impl Jobs {
    /// The fixtures.
    #[must_use]
    pub fn shipped() -> Self {
        use JobState::{Active, OnJobQueue, OnOutputQueue};
        let log = vec![
            logged("CPF1124", MessageType::Informational, "Job started.", T0),
            logged(
                "",
                MessageType::Reply,
                "CRTBNDRPG PGM(HLINV/ASSETS)",
                T0 + 10,
            ),
            logged("CPF1164", MessageType::Completion, "Job ended.", T0 + 20),
        ];
        let job = |e: JobEntry, log: Option<Vec<JobLogEntry>>| Job {
            entry: e,
            held: false,
            log,
            ended: None,
        };
        Self {
            jobs: vec![
                job(
                    entry(
                        ("QPADEV0001", "BOB", "000201"),
                        Active,
                        "INTER",
                        "QINTER",
                        "MNU-MAIN",
                        T0,
                    ),
                    Some(log[..1].to_vec()),
                ),
                job(
                    entry(
                        ("PAYRUN", "ANN", "000300"),
                        Active,
                        "BATCH",
                        "QBATCH",
                        "PGM-PAYROLL",
                        T0 + 5,
                    ),
                    Some(Vec::new()),
                ),
                job(
                    entry(
                        ("NIGHTLY", "BOB", "000202"),
                        OnJobQueue,
                        "BATCH",
                        "",
                        "",
                        T0 + 60,
                    ),
                    Some(Vec::new()),
                ),
                job(
                    entry(
                        ("BUILD", "BOB", "000150"),
                        OnOutputQueue,
                        "BATCH",
                        "",
                        "",
                        T0 - 3_600_000,
                    ),
                    Some(log),
                ),
                job(
                    entry(
                        ("OLDJOB", "BOB", "000151"),
                        OnOutputQueue,
                        "BATCH",
                        "",
                        "",
                        T0 - 1_800_000,
                    ),
                    None,
                ),
            ],
        }
    }

    /// `WRKUSRJOB`: `me`'s own, or with `*JOBCTL` another's or all.
    pub fn list(
        &self,
        me: &str,
        jobctl: bool,
        known: &dyn Fn(&str) -> bool,
        req: &ListJobs,
    ) -> Result<Vec<JobEntry>, No> {
        let user = req.user.trim().to_ascii_uppercase();
        let user = if user == "*CURRENT" || user.is_empty() {
            me.to_string()
        } else {
            user
        };
        if user != "*ALL" && !known(&user) {
            return Err(no(Status::NotFound, "CPF2204", &[&user]));
        }
        if user != me && !jobctl {
            return Err(no(Status::NotAuthorized, "CPF1343", &[me, &user]));
        }
        let mut v: Vec<JobEntry> = self
            .jobs
            .iter()
            .map(|j| j.entry.clone())
            .filter(|e| user == "*ALL" || e.id.user == user)
            .filter(|e| req.status.is_none_or(|s| s == e.state))
            .collect();
        v.sort_by_key(|e| (e.state as u8, e.entered));
        Ok(v)
    }

    fn find(&mut self, me: &str, jobctl: bool, id: &JobId) -> Result<&mut Job, No> {
        let j = self
            .jobs
            .iter_mut()
            .find(|j| {
                j.entry.id.number == id.number.trim()
                    && j.entry.id.name.eq_ignore_ascii_case(id.name.trim())
                    && j.entry.id.user.eq_ignore_ascii_case(id.user.trim())
            })
            .ok_or_else(|| no(Status::NotFound, "CPF1321", &[&id.name]))?;
        if j.entry.id.user != me && !jobctl {
            return Err(no(Status::NotAuthorized, "CPF1343", &[me, &id.name]));
        }
        Ok(j)
    }

    /// `DSPJOBLOG`.
    pub fn log(&mut self, me: &str, jobctl: bool, id: &JobId) -> Result<Vec<JobLogEntry>, No> {
        let j = self.find(me, jobctl, id)?;
        j.log
            .clone()
            .ok_or_else(|| no(Status::NotFound, "CPF1321", &[&id.name]))
    }

    /// `HLDJOB` and `RLSJOB`.
    pub fn hold(&mut self, me: &str, jobctl: bool, id: &JobId, hold: bool) -> Result<(), No> {
        let j = self.find(me, jobctl, id)?;
        if j.entry.state == JobState::OnOutputQueue {
            return Err(no(Status::Error, "CPF1362", &[&id.name]));
        }
        j.held = hold;
        Ok(())
    }

    /// `ENDJOB`: the job ends, its entry moves to the output queue's.
    pub fn end(&mut self, me: &str, jobctl: bool, req: &EndJob) -> Result<(), No> {
        let j = self.find(me, jobctl, &req.job)?;
        if j.entry.state == JobState::OnOutputQueue {
            return Err(no(Status::Error, "CPF1362", &[&req.job.name]));
        }
        j.ended = Some((req.how, req.delay));
        j.entry.state = JobState::OnOutputQueue;
        j.entry.status = "*OUTQ".into();
        j.entry.subsystem.clear();
        j.entry.function.clear();
        Ok(())
    }
}

/// The catalog's routines.
fn routines() -> Vec<Routine> {
    let r = |name: &str, specific: &str, kind, text: &str| Routine {
        schema: "HLINV".into(),
        name: name.into(),
        specific: specific.into(),
        kind,
        text: text.into(),
    };
    vec![
        r(
            "ASSETAGE",
            "ASSETAGE",
            RoutineKind::Function,
            "Age of an asset in days",
        ),
        r("RESTOCK", "RESTOCK", RoutineKind::Procedure, "Order more"),
        r(
            "RESTOCK",
            "RESTOCK2",
            RoutineKind::Procedure,
            "Order more, by location",
        ),
    ]
}

/// `ListRoutines`.
pub fn list_routines(req: &ListRoutines) -> Result<Vec<Routine>, No> {
    let schema = req.schema.trim().to_ascii_uppercase();
    if !["HLINV", "HLTEST", "HLPAY", "QGPL", "QSYS2"].contains(&schema.as_str()) {
        return Err(no(Status::NotFound, "SQL0204", &[&schema, "SCHEMA"]));
    }
    Ok(routines()
        .into_iter()
        .filter(|r| r.schema == schema && like(&r.name, &req.pattern))
        .filter(|r| req.kinds.is_empty() || req.kinds.contains(&r.kind))
        .collect())
}

/// `ListIndexes` as `me`.
pub fn list_indexes(me: &str, req: &ListIndexes) -> Result<Indexes, No> {
    let (schema, table) = (
        req.schema.trim().to_ascii_uppercase(),
        req.table.trim().to_ascii_uppercase(),
    );
    let ix = |name: &str, unique, cols: &[&str]| Index {
        schema: schema.clone(),
        name: name.into(),
        unique,
        columns: cols.iter().map(ToString::to_string).collect(),
    };
    match (schema.as_str(), table.as_str()) {
        ("HLINV", "ASSETS") => Ok(Indexes {
            indexes: vec![
                ix("ASSETSI1", true, &["ASSETID"]),
                ix("ASSETSL1", false, &["LOCATION", "ASSETID"]),
            ],
        }),
        ("HLINV", "LOCATIONS") => Ok(Indexes::default()),
        ("HLPAY", "SALARIES") if me != "ANN" => {
            Err(no(Status::NotAuthorized, "SQL0551", &["SALARIES", "HLPAY"]))
        }
        ("HLPAY", "SALARIES") => Ok(Indexes {
            indexes: vec![ix("SALARIESI1", true, &["EMPNO"])],
        }),
        _ => Err(no(Status::NotFound, "SQL0204", &[&table, &schema])),
    }
}

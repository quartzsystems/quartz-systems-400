// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Jobs` (version 8, draft): a user's jobs, job logs, hold, release and
//! end, as `WRKUSRJOB`, `DSPJOBLOG`, `HLDJOB`, `RLSJOB` and `ENDJOB` do
//! (docs/QAP.md, "Service 10 `Jobs`").

use ipc::{wire_enum, wire_struct};

use crate::envelope::ServerMessage;
use crate::ops::messages::MessageType;
use crate::types::{EndHow, JobId};

/// Where a job is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JobState {
    /// Running.
    #[default]
    Active,
    /// Waiting on a job queue.
    OnJobQueue,
    /// Ended, its spooled files kept.
    OnOutputQueue,
}

wire_enum!(JobState {
    0 => Active,
    1 => OnJobQueue,
    2 => OnOutputQueue,
});

impl JobState {
    /// Every state, in value order.
    pub const ALL: [JobState; 3] = [
        JobState::Active,
        JobState::OnJobQueue,
        JobState::OnOutputQueue,
    ];
}

/// `ListJobs`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListJobs {
    /// `*CURRENT`, `*ALL` or a profile.
    pub user: String,
    /// Only jobs in this state; `None` for all.
    pub status: Option<JobState>,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListJobs {
    user,
    status,
    start,
    max
});

/// A job, as `WRKUSRJOB` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobEntry {
    /// The job.
    pub id: JobId,
    /// Where it is.
    pub state: JobState,
    /// Its status as `WRKUSRJOB` shows it (`*ACTIVE`).
    pub status: String,
    /// Its type (`BATCH`, `INTER`).
    pub r#type: String,
    /// Its subsystem; empty when not active.
    pub subsystem: String,
    /// What it runs, as `WRKACTJOB` shows it (`PGM-QCMD`); empty when
    /// nothing.
    pub function: String,
    /// When it entered the system, milliseconds since 1970-01-01 UTC.
    pub entered: i64,
}

wire_struct!(JobEntry {
    id,
    state,
    status,
    r#type,
    subsystem,
    function,
    entered
});

/// `ListJobs`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Jobs {
    /// Jobs matching.
    pub total: u32,
    /// Those returned: active, then on job queues, then on output
    /// queues, each by when it entered.
    pub jobs: Vec<JobEntry>,
}

wire_struct!(Jobs { total, jobs });

/// `JobLog`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobLog {
    /// The job.
    pub job: JobId,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(JobLog { job, start, max });

/// One message of a job log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobLogEntry {
    /// The message.
    pub message: ServerMessage,
    /// Its type.
    pub kind: MessageType,
    /// When it was sent, milliseconds since 1970-01-01 UTC.
    pub sent: i64,
    /// The program that sent it.
    pub from_program: String,
    /// The program it was sent to.
    pub to_program: String,
}

wire_struct!(JobLogEntry {
    message,
    kind,
    sent,
    from_program,
    to_program
});

/// `JobLog`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobLogEntries {
    /// Entries in the log.
    pub total: u32,
    /// Those returned, oldest first.
    pub entries: Vec<JobLogEntry>,
}

wire_struct!(JobLogEntries { total, entries });

/// A job alone: `HoldJob`'s and `ReleaseJob`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobRef {
    /// The job.
    pub job: JobId,
}

wire_struct!(JobRef { job });

/// `EndJob`'s request: `ENDJOB`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndJob {
    /// The job.
    pub job: JobId,
    /// `OPTION`.
    pub how: EndHow,
    /// `DELAY` seconds for a controlled end; `None` for the job's
    /// default.
    pub delay: Option<u32>,
}

wire_struct!(EndJob { job, how, delay });

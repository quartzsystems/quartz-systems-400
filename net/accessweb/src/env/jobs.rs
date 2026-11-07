// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Jobs (handoff step 17): the user's jobs as `WRKUSRJOB` shows them,
//! completed jobs with output included, and the server jobs that serve
//! Access for Web. Hold and Release are `HLDJOB` and `RLSJOB` run as
//! the signed-on user ([`Jobs::run`]); Job log, Properties and End are
//! Navigator's pages.

use qsweb::msg::Message;

/// Where a job stands, as the Status select filters it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    /// Running.
    #[default]
    Active,
    /// Waiting on a job queue.
    OnQueue,
    /// Ended, its output still on an output queue.
    Completed,
}

/// One job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserJob {
    /// Job name.
    pub name: String,
    /// User profile.
    pub user: String,
    /// Number, 6 digits.
    pub number: String,
    /// `Interactive`, `Batch`, `Prestart (database server)`, ... for My
    /// jobs; `BCH`, `PJ`, ... for Server jobs.
    pub kind: String,
    /// `RUN`, `MSGW`, `JOBQ`, `OUTQ`, ...
    pub status: String,
    /// Active, on a job queue, completed.
    pub stage: Stage,
    /// The subsystem, empty when not active.
    pub subsystem: String,
    /// `PGM-name`, `CMD-name`, ...
    pub function: String,
    /// Entered the system, seconds since 1970.
    pub entered: u64,
    /// For a server job: what it serves.
    pub serving: String,
}

impl UserJob {
    /// `number/user/name`.
    pub fn qualified(&self) -> String {
        format!("{}/{}/{}", self.number, self.user, self.name)
    }
}

/// What the job pages need.
pub trait Jobs: Send + Sync {
    /// `user`'s jobs (`WRKUSRJOB USER(user) STATUS(*ALL)`): active, on a
    /// job queue, completed with output, and the jobs serving the user
    /// under another profile.
    fn user_jobs(&self, user: &str) -> Vec<UserJob>;
    /// The jobs that run Access for Web and serve `user`'s session.
    fn server_jobs(&self, user: &str) -> Vec<UserJob>;
    /// Run CL `command` as `user`: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}

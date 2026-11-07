// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's jobs: `TESTUSER`'s of `web/my-jobs.html` and the
//! server jobs of `web/server-jobs.html`.

use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use crate::env::jobs::{Jobs, Stage, UserJob};
use crate::util::secs;

/// A sample job: name, user, number, type, status, stage, subsystem,
/// function, entered (UTC month, day, hour, minute of 2026), serving.
type Sample = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Stage,
    &'static str,
    &'static str,
    [u32; 4],
    &'static str,
);

/// `TESTUSER`'s jobs, as `WRKUSRJOB` lists them.
const MINE: &[Sample] = &[
    (
        "QPADEV0003",
        "TESTUSER",
        "123488",
        "Interactive",
        "RUN",
        Stage::Active,
        "QINTER",
        "CMD-WRKACTJOB",
        [10, 29, 17, 4],
        "",
    ),
    (
        "INVBATCH",
        "TESTUSER",
        "123503",
        "Batch",
        "MSGW",
        Stage::Active,
        "QBATCH",
        "PGM-HLINV/INVLOAD",
        [10, 29, 17, 58],
        "",
    ),
    (
        "QZDASOINIT",
        "QUSER",
        "123411",
        "Prestart (database server)",
        "RUN",
        Stage::Active,
        "QUSRWRK",
        "",
        [10, 29, 17, 30],
        "",
    ),
    (
        "INVBATCH",
        "TESTUSER",
        "123402",
        "Batch",
        "OUTQ",
        Stage::Completed,
        "",
        "",
        [10, 29, 10, 14],
        "",
    ),
    (
        "QPADEV0003",
        "TESTUSER",
        "123311",
        "Interactive",
        "OUTQ",
        Stage::Completed,
        "",
        "",
        [10, 28, 21, 40],
        "",
    ),
];

/// The jobs serving Access for Web.
const SERVER: &[Sample] = &[
    (
        "ACCESSWEB",
        "QTMHHTTP",
        "123463",
        "BCH",
        "RUN",
        Stage::Active,
        "QHTTPSVR",
        "PGM-QZHBMAIN",
        [10, 29, 10, 0],
        "Access for Web (port 2003/2006)",
    ),
    (
        "ACCESSWEB",
        "QTMHHTTP",
        "123464",
        "BCI",
        "TIMW",
        Stage::Active,
        "QHTTPSVR",
        "PGM-QZSRHTTP",
        [10, 29, 10, 0],
        "Worker thread pool",
    ),
    (
        "QZDASOINIT",
        "QUSER",
        "123411",
        "PJ",
        "RUN",
        Stage::Active,
        "QUSRWRK",
        "PGM-QZDASOINIT",
        [10, 29, 17, 30],
        "Database (this session)",
    ),
    (
        "QZRCSRVS",
        "QUSER",
        "123420",
        "PJ",
        "PSRW",
        Stage::Active,
        "QUSRWRK",
        "",
        [10, 29, 17, 30],
        "Remote command",
    ),
    (
        "QZSOSIGN",
        "QUSER",
        "123421",
        "PJ",
        "PSRW",
        Stage::Active,
        "QUSRWRK",
        "",
        [10, 29, 17, 30],
        "Sign-on",
    ),
    (
        "QTVTELNET",
        "QTCP",
        "123381",
        "BCH",
        "SELW",
        Stage::Active,
        "QSYSWRK",
        "PGM-QTVTELNT",
        [10, 29, 10, 0],
        "5250 session (telnet)",
    ),
];

fn job(s: &Sample) -> UserJob {
    UserJob {
        name: s.0.into(),
        user: s.1.into(),
        number: s.2.into(),
        kind: s.3.into(),
        status: s.4.into(),
        stage: s.5,
        subsystem: s.6.into(),
        function: s.7.into(),
        entered: secs(2026, s.8[0], s.8[1], s.8[2], s.8[3], 0),
        serving: s.9.into(),
    }
}

/// The sample jobs.
#[derive(Debug, Default)]
pub struct FixtureJobs {
    /// The commands run, as `user: command`.
    pub ran: Mutex<Vec<String>>,
}

impl Jobs for FixtureJobs {
    fn user_jobs(&self, user: &str) -> Vec<UserJob> {
        if user == super::USER {
            MINE.iter().map(job).collect()
        } else {
            Vec::new()
        }
    }

    fn server_jobs(&self, _user: &str) -> Vec<UserJob> {
        SERVER.iter().map(job).collect()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran
            .lock()
            .expect("ran")
            .push(format!("{user}: {command}"));
        Ok(Message::new(Kind::Done, "", ""))
    }
}

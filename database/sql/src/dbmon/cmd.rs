// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages of `STRDBMON`, `ENDDBMON` and `CHGQRYA`: each starts
//! with its message ID, as the commands' processing programs return
//! them.

/// Why a parameter must keep its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// A filter or threshold: every statement is recorded.
    Filter,
    /// `INCSYSSQL`: the system's own SQL is not monitored.
    SystemSql,
    /// `TYPE(*DETAIL)`: its `3019` records are not written.
    Detail,
    /// `QRYTIMLMT`: no time estimate.
    TimeLimit,
    /// `QRYSTGLMT`: no storage estimate.
    StorageLimit,
    /// `QRYOPTLIB`: options come from `OVERRIDE_QAQQINI`.
    OptionsLibrary,
    /// `DEGREE(*NBRTASKS n)`.
    Tasks,
}

impl Why {
    fn text(self) -> &'static str {
        match self {
            Self::Filter => "the monitor records every statement of the jobs it watches",
            Self::SystemSql => "QS/400 monitors the statements jobs run, not its own",
            Self::Detail => "TYPE(*DETAIL) adds 3019 records, which are not written",
            Self::TimeLimit => "the planner makes no time estimate a limit could hold a query to",
            Self::StorageLimit => "the planner makes no storage estimate a limit could hold a query to",
            Self::OptionsLibrary => "query options are set with QSYS2.OVERRIDE_QAQQINI",
            Self::Tasks => "a number of tasks is not kept",
        }
    }
}

/// `kwd(value)` refused.
#[must_use]
pub fn refused(kwd: &str, value: &str, why: Why) -> String {
    format!("CPF9898 {kwd}({value}) is not supported: {}.", why.text())
}

/// A generic job name refused.
#[must_use]
pub fn generic_job() -> String {
    "CPF9898 A generic job name is not kept: name one job, * or *ALL.".into()
}

/// The monitor started.
#[must_use]
pub fn started(id: &str, job: &str) -> String {
    format!("CPI436A Monitor {id} now records the SQL of job {job}.\n")
}

/// `CHGQRYA` of a job that does not exist.
#[must_use]
pub fn job_not_found(name: &str, user: &str, number: &str) -> String {
    format!("CPF1321 Job {name} user {user} job number {number} not found.")
}

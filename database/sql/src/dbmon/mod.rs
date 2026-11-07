// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database monitor (`STRDBMON`, `ENDDBMON`): monitors over one job
//! or every job, each writing records in IBM's `QAQQDBMN` layout to its
//! output file. A job's SQL session reports each statement it ran
//! ([`Observed`]), with the planner's choices ([`plan::PlanEvent`]);
//! the records wait in the monitor and go to the file every `FRCRCD`
//! records and when the monitor ends.
//!
//! The monitors live in the storage service, one thread: a thread-local
//! table here, not in the store.

pub mod cmd;
pub mod columns;
pub mod plan;
pub mod record;
mod write;

use std::cell::RefCell;

use store::txn::Txn;

use crate::exec::SqlContext;
use crate::value::Value;

pub use record::{Observed, Record};
pub use write::flush;

/// A monitor error: the message ID and its text.
#[derive(Debug, thiserror::Error)]
#[error("{id} {text}")]
pub struct MonError {
    /// The message ID.
    pub id: &'static str,
    /// What happened.
    pub text: String,
}

fn err(id: &'static str, text: impl Into<String>) -> MonError {
    MonError {
        id,
        text: text.into(),
    }
}

/// The job a session belongs to, as the records name it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobIdent {
    /// Job name.
    pub name: String,
    /// User.
    pub user: String,
    /// Job number.
    pub number: u32,
}

impl JobIdent {
    /// `number/user/name`.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{:06}/{}/{}", self.number, self.user, self.name)
    }
}

/// What a job's SQL context keeps for the monitor.
#[derive(Debug, Default)]
pub struct JobState {
    /// The job; a context without one (the service console, tools) is
    /// never monitored.
    pub ident: Option<JobIdent>,
    /// The system name (`QQSYS`) and the local RDB (`QQRDBN`).
    pub system: String,
    /// Statements numbered so far (`QQSTN`).
    statements: u64,
    /// Queries numbered so far (`QQUCNT`).
    queries: u64,
    /// The planner's choices for the statement running, while a monitor
    /// watches the job.
    pub(crate) trace: Option<Vec<plan::PlanEvent>>,
}

impl JobState {
    /// Record a planner choice when the statement is watched.
    pub(crate) fn note(&mut self, e: plan::PlanEvent) {
        if let Some(t) = self.trace.as_mut() {
            t.push(e);
        }
    }
}

/// The jobs a monitor watches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One job, by number.
    Job(u32),
    /// Every job.
    All,
}

/// `TYPE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// `*BASIC` (and `*SUMMARY`, the same records on IBM i 7.1).
    Basic,
    /// `*DETAIL`.
    Detail,
}

/// What `STRDBMON` asked for.
#[derive(Clone, Debug)]
pub struct StartSpec {
    /// The output file's library (`*LIBL`, `*CURLIB` or a name).
    pub lib: String,
    /// The output file.
    pub file: String,
    /// The member (`*FIRST` for the file's name).
    pub member: String,
    /// `*ADD` rather than `*REPLACE`.
    pub add: bool,
    /// The jobs.
    pub scope: Scope,
    /// `JOB` as given, for the start record and the message.
    pub job_text: String,
    /// `TYPE`.
    pub detail: Detail,
    /// `FRCRCD`; 0 for `*CALC`.
    pub frcrcd: u16,
    /// `INCSYSSQL(*YES)`.
    pub incsyssql: bool,
    /// `COMMENT`.
    pub comment: String,
    /// The command as the start record keeps it.
    pub command: String,
    /// The job starting it.
    pub starter: JobIdent,
}

/// An active monitor.
#[derive(Debug)]
struct Monitor {
    id: String,
    scope: Scope,
    lib: String,
    file: String,
    frcrcd: u16,
    starter: JobIdent,
    pending: Vec<Record>,
}

thread_local! {
    static MONITORS: RefCell<Vec<Monitor>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: RefCell<u32> = const { RefCell::new(0) };
}

/// Records a monitor holds before it writes them when `FRCRCD(*CALC)`.
pub const CALC_RECORDS: usize = 64;

/// Whether a monitor watches job `number`.
#[must_use]
pub fn watching(number: u32) -> bool {
    MONITORS.with(|m| {
        m.borrow()
            .iter()
            .any(|x| x.scope == Scope::All || x.scope == Scope::Job(number))
    })
}

/// The monitors' `(id, scope, lib/file)`, for displays and tests.
#[must_use]
pub fn active() -> Vec<(String, Scope, String)> {
    MONITORS.with(|m| {
        m.borrow()
            .iter()
            .map(|x| {
                (
                    x.id.clone(),
                    x.scope.clone(),
                    format!("{}/{}", x.lib, x.file),
                )
            })
            .collect()
    })
}

/// Start a monitor: the output file made or checked, the member
/// replaced unless `*ADD`, the start record held. The monitor's ID.
pub fn start(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    spec: &StartSpec,
) -> Result<String, MonError> {
    if let Scope::Job(n) = spec.scope
        && MONITORS.with(|m| m.borrow().iter().any(|x| x.scope == Scope::Job(n)))
    {
        return Err(err(
            "CPF436C",
            format!("Job {} is already being monitored.", spec.job_text),
        ));
    }
    let lib = write::prepare_file(txn, ctx, spec)?;
    let seq = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        *n += 1;
        *n
    });
    let id = format!("{:06}{:04}", spec.starter.number % 1_000_000, seq % 10_000);
    let mut mon = Monitor {
        id: id.clone(),
        scope: spec.scope.clone(),
        lib,
        file: spec.file.clone(),
        frcrcd: spec.frcrcd,
        starter: spec.starter.clone(),
        pending: Vec::new(),
    };
    let r = record::monitor(
        ctx,
        &spec.starter,
        &mon.scope,
        &id,
        true,
        &spec.comment,
        &spec.command,
    );
    mon.pending.push(r);
    MONITORS.with(|m| m.borrow_mut().push(mon));
    Ok(id)
}

/// End the monitors `scope` (and `monid`, `*ALL` for every one of the
/// scope) names: each writes what it holds and its end record.
pub fn end(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    scope: &Scope,
    job_text: &str,
    monid: Option<&str>,
    comment: &str,
) -> Result<usize, MonError> {
    let all_ids = monid == Some("*ALL");
    let picked: Vec<usize> = MONITORS.with(|m| {
        m.borrow()
            .iter()
            .enumerate()
            .filter(|(_, x)| &x.scope == scope && (all_ids || monid.is_none_or(|id| id == x.id)))
            .map(|(i, _)| i)
            .collect()
    });
    if picked.is_empty() {
        return Err(err(
            "CPF436D",
            format!("Job {job_text} is not being monitored."),
        ));
    }
    if picked.len() > 1 && monid.is_none() {
        return Err(err(
            "CPF43A1",
            format!("Job {job_text} on ENDDBMON is not unique: name the monitor with MONID."),
        ));
    }
    let mut ended = Vec::new();
    MONITORS.with(|m| {
        let mut m = m.borrow_mut();
        for i in picked.iter().rev() {
            ended.push(m.remove(*i));
        }
    });
    for mut mon in ended {
        let r = record::monitor(ctx, &mon.starter, &mon.scope, &mon.id, false, comment, "");
        mon.pending.push(r);
        write::write(txn, ctx, &mon.lib, &mon.file, &mon.pending)?;
    }
    Ok(picked.len())
}

/// Write `records` to a monitor file of its own, `lib/file` made or
/// replaced (`DUMP_PLAN_CACHE`).
pub(crate) fn dump(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    lib: &str,
    file: &str,
    records: &[Record],
) -> Result<(), MonError> {
    let spec = StartSpec {
        lib: lib.to_string(),
        file: file.to_string(),
        member: "*FIRST".into(),
        add: false,
        scope: Scope::All,
        job_text: String::new(),
        detail: Detail::Basic,
        frcrcd: 0,
        incsyssql: false,
        comment: String::new(),
        command: String::new(),
        starter: ctx.dbmon.ident.clone().unwrap_or_default(),
    };
    let lib = write::prepare_file(txn, ctx, &spec)?;
    write::write(txn, ctx, &lib, file, records)
}

/// Get ready for a statement of the context's job: when a monitor
/// watches it, the planner's choices are kept.
pub fn begin(ctx: &mut SqlContext) -> bool {
    let on = ctx.dbmon.ident.as_ref().is_some_and(|j| watching(j.number));
    ctx.dbmon.trace = on.then(Vec::new);
    on
}

/// The statement ran: its records go to each monitor watching the job.
/// True when one holds enough to write.
pub fn finish(ctx: &mut SqlContext, o: &Observed<'_>) -> bool {
    let plan = ctx.dbmon.trace.take().unwrap_or_default();
    let Some(job) = ctx.dbmon.ident.clone() else {
        return false;
    };
    ctx.dbmon.statements += 1;
    let ucnt = if plan.is_empty() && o.rows.is_none() {
        0
    } else {
        ctx.dbmon.queries += 1;
        ctx.dbmon.queries
    };
    let stn = ctx.dbmon.statements;
    let recs = record::statement(ctx, &job, o, &plan, ucnt, stn);
    MONITORS.with(|m| {
        let mut due = false;
        for mon in m
            .borrow_mut()
            .iter_mut()
            .filter(|x| x.scope == Scope::All || x.scope == Scope::Job(job.number))
        {
            mon.pending.extend(recs.iter().cloned());
            let limit = if mon.frcrcd == 0 {
                CALC_RECORDS
            } else {
                usize::from(mon.frcrcd)
            };
            due |= mon.pending.len() >= limit;
        }
        due
    })
}

/// Take the records of every monitor that holds `limit` or more (all
/// with `force`), with the file each goes to.
fn take_due(force: bool) -> Vec<(String, String, Vec<Record>)> {
    MONITORS.with(|m| {
        m.borrow_mut()
            .iter_mut()
            .filter(|x| {
                let limit = if x.frcrcd == 0 {
                    CALC_RECORDS
                } else {
                    usize::from(x.frcrcd)
                };
                !x.pending.is_empty() && (force || x.pending.len() >= limit)
            })
            .map(|x| {
                (
                    x.lib.clone(),
                    x.file.clone(),
                    std::mem::take(&mut x.pending),
                )
            })
            .collect()
    })
}

/// A value for a `CHAR` column, blank when empty.
pub(crate) fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

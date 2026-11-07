// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Jobs` service (version 8, draft; docs/QAP.md "Service 10
//! `Jobs`") on a secondary thread: decoded, paged, the connection's own
//! job kept from `EndJob`; the host does what `WRKUSRJOB`, `DSPJOBLOG`,
//! `HLDJOB`, `RLSJOB` and `ENDJOB` do.

use ipc::Wire;
use qap_proto::ops::jobs::{EndJob, JobLog, JobLogEntries, JobRef, Jobs, ListJobs};
use qap_proto::types::JobId;
use qap_proto::{Envelope, Op, Status};

use crate::conn::Answer;
use crate::host::{Refusal, ThreadHost};
use crate::sql::page;

fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

fn ok(r: Result<impl Wire, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// Whether two job names are the same job.
fn same(a: &JobId, b: &JobId) -> bool {
    a.number.trim() == b.number.trim()
        && a.name.trim().eq_ignore_ascii_case(b.name.trim())
        && a.user.trim().eq_ignore_ascii_case(b.user.trim())
}

/// Answer one `Jobs` operation; `own` is the connection's job.
pub(crate) fn serve(host: &mut dyn ThreadHost, own: &JobId, op: Op, req: &Envelope) -> Answer {
    match op {
        Op::ListJobs => body(req, |b: ListJobs| {
            ok(host.list_jobs(&b).map(|all| {
                let (total, jobs) = page(all, b.start, b.max);
                Jobs { total, jobs }
            }))
        }),
        Op::JobLog => body(req, |b: JobLog| {
            ok(host.job_log(&b.job).map(|all| {
                let (total, entries) = page(all, b.start, b.max);
                JobLogEntries { total, entries }
            }))
        }),
        Op::HoldJob => body(req, |b: JobRef| done(host.hold_job(&b.job))),
        Op::ReleaseJob => body(req, |b: JobRef| done(host.release_job(&b.job))),
        // A client signs off rather than ending its own job.
        Op::EndJob => body(req, |b: EndJob| {
            if same(&b.job, own) {
                Answer::status(Status::InvalidRequest)
            } else {
                done(host.end_job(&b))
            }
        }),
        _ => Answer::status(Status::InvalidRequest),
    }
}

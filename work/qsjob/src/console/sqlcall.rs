// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL for a requester in the job (the database host server, the DDM
//! server, QAP, STRSQL, RUNSQL): its statements run in the storage
//! service over the job's link; a `CALL` one plans is made here, in the
//! job's runtime, built on the first call, and reported back
//! ([`Op::Called`]). The procedure's result sets stay in the
//! statement's scope as cursors `RS#1`, `RS#2`, ...

use qrpgrt::sqlcall::Outcome;
use sql::Value;
use sql::embed::{Desc, Embedded, Op, Reply, Request};
use sql::exec::routine::CallPlan;
use stg::Shared;
use stg::remote::RemoteSql;

use super::Console;

/// Run `req` over `link`; a call it plans is made by `perform`. The
/// reply, and the `OUT` and `INOUT` values by marker.
pub fn run(link: &Shared, req: &Request, perform: impl FnOnce(&CallPlan) -> Outcome) -> (Reply, Vec<(usize, Value)>) {
    let mut req = req.clone();
    req.desc.calls = true;
    let mut sql = RemoteSql::new(link.clone());
    let reply = sql.run(&req);
    let Some(plan) = reply.call.as_deref() else { return (reply, Vec::new()) };
    let out = perform(plan);
    let called = Request {
        scope: req.scope.clone(),
        desc: Desc { op: Op::Called, ..Desc::default() },
        inputs: out.called.inputs(),
    };
    let r = sql.run(&called);
    let outs = if r.sqlca.is_error() { Vec::new() } else { out.outs };
    (r, outs)
}

/// Make `plan`'s call in the console's job, as the profile it runs as
/// now (a server job runs as each client in turn).
pub fn perform(console: &mut Console, plan: &CallPlan) -> Outcome {
    console.job();
    console.swapped();
    qrpgrt::sqlcall::perform(console.job(), plan)
}

/// The job's SQL for a server: statements over the link, calls in the
/// job's runtime.
pub struct CallingSql {
    link: Shared,
    job: crate::work::hostsvr::job::Job,
    outs: Vec<(usize, Value)>,
}

impl CallingSql {
    /// Over `link`, calls made in `job`.
    pub fn new(link: Shared, job: crate::work::hostsvr::job::Job) -> Self {
        Self { link, job, outs: Vec::new() }
    }
}

impl Embedded for CallingSql {
    fn run(&mut self, req: &Request) -> Reply {
        let job = &self.job;
        let (reply, outs) = run(&self.link, req, |p| perform(&mut job.borrow_mut(), p));
        self.outs = outs;
        reply
    }

    fn take_outs(&mut self) -> Vec<(usize, Value)> {
        std::mem::take(&mut self.outs)
    }
}

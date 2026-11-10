// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! STRSQL's statements (`isql_run`) and the console's (`sql ...;`, or a
//! statement ended in SQL mode) take turns as a job's embedded
//! statements do: on a fiber, over the lent store, with the job's
//! console SQL context and the system's host. So the `QSYS2` services
//! that read the system (`NETSTAT_INFO`, `SERVER_SHARE_INFO`,
//! `ACTIVE_JOB_INFO` ...) answer there too, not SQL0443.

use std::cell::RefMut;
use std::rc::Rc;

use sql::embed::StoreLend as _;
use sql::{Prepared, SqlContext};
use stg::Request;
use stg::proto::{ExecReply, IsqlReply};
use store::Store;

use super::super::sqlhost::SqlHost;
use super::super::{isql, security, served};
use super::{Svc, Turn, answer, fiber, first_turn, park, unpark, yielder};
use crate::ipl::system::System;
use crate::svc::db::sqlcmd;
use crate::svc::ile::MainStore;

/// What runs on the fiber.
enum Work {
    /// STRSQL's statement, its rows cut at the maximum.
    Isql(Box<Prepared>, u32),
    /// The console's statements.
    Line(String),
}

/// What it answers.
enum Out {
    Isql(IsqlReply),
    Line(String),
}

/// Start an `isql_run` or a console line that completes a statement.
pub(super) fn start(mut svc: Box<Svc>, id: u64, i: usize, req: Request) -> Box<Svc> {
    let work = match &req {
        Request::IsqlRun {
            text,
            naming,
            max_rows,
            ..
        } => match isql::prepare(text, naming) {
            Ok(p) => Work::Isql(Box::new(p), *max_rows),
            Err(r) => {
                // A statement that does not parse is answered at once.
                svc.busy.retain(|b| *b != id);
                let _ = svc.conns[i].chan.send_raw(&ipc::encode(&*r));
                return svc;
            }
        },
        Request::Exec { line } => {
            let text = svc.conns[i]
                .ctx
                .as_ref()
                .and_then(|c| sqlcmd::completes(&c.svc, line));
            match text {
                Some(t) => Work::Line(t),
                None => return direct(svc, id, i, req),
            }
        }
        _ => unreachable!("long() said a console statement"),
    };
    let user = security::as_user(&req);
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("long() said a job");
    let number = ctx.job.number;
    super::super::trace::request(number, &req);
    *system.stats.aux_io.entry(number).or_default() += 1;
    system.current_job = number;
    // The job's console SQL context, made or brought up to date as the
    // console makes it.
    std::mem::swap(&mut system.svc, &mut ctx.svc);
    let made = crate::svc::db::db(system).map(|_| ());
    std::mem::swap(&mut system.svc, &mut ctx.svc);
    // No store or no stack to be had: it runs to its end, as before.
    let (Ok(()), Ok(stack)) = (made, fiber::stack()) else {
        return direct(svc, id, i, req);
    };
    if let Work::Line(_) = work
        && let crate::svc::db::Mode::Sql { buffer } = &mut ctx.svc.mode
    {
        // The statement leaves the buffer as the console's would.
        buffer.clear();
    }
    let mut sqlctx = ctx.svc.sql.take().expect("db() made it");
    let host = sqlctx.host.replace(Rc::new(SqlHost { job: number }));
    let session = Rc::clone(&ctx.session);
    security::freshen(system, &session);
    let entered = served::enter(system, ctx);
    let shared = Rc::clone(&system.lent);
    let turn = Turn::new(&mut svc, id, entered);
    let t2 = Rc::clone(&turn);
    sqlctx.yielder = Some(yielder(Rc::clone(&turn)));
    let body = move || {
        let lender = MainStore::new(shared);
        let get = || lender.store();
        let out = if user {
            sec::guard::as_job(&session, || run(&get, &mut sqlctx, work))
        } else {
            run(&get, &mut sqlctx, work)
        };
        drop(lender);
        sqlctx.yielder = None;
        sqlctx.host = host;
        park(finish(unpark(), &turn, sqlctx, out));
    };
    first_turn(svc, t2, fiber::Fiber::on(stack, body))
}

/// Answer the request in the ordinary way, not taking turns.
fn direct(mut svc: Box<Svc>, id: u64, i: usize, req: Request) -> Box<Svc> {
    svc.busy.retain(|b| *b != id);
    answer(&mut svc, i, req);
    svc
}

fn run<'a>(get: &'a dyn Fn() -> RefMut<'a, Store>, ctx: &mut SqlContext, work: Work) -> Out {
    match work {
        Work::Isql(p, max) => Out::Isql(isql::run_on(get, ctx, &p, max)),
        Work::Line(text) => Out::Line(sqlcmd::run_text_on(get, ctx, &text)),
    }
}

/// The statement ended: the context back with its job, the service as
/// between requests, the reply sent.
fn finish(mut svc: Box<Svc>, turn: &Turn, sqlctx: SqlContext, out: Out) -> Box<Svc> {
    svc.busy.retain(|b| *b != turn.id);
    let i = svc.index(turn.id).expect("a busy connection stays");
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("a job");
    ctx.svc.sql = Some(sqlctx);
    served::leave(system, turn.entered.get());
    let who = served::charged(ctx);
    security::flush(system, Some(who));
    let bytes = match out {
        Out::Isql(r) => ipc::encode(&r),
        Out::Line(text) => ipc::encode(&exec_reply(
            system,
            text,
            crate::svc::db::prompt_of(&ctx.svc),
        )),
    };
    let _ = conns[i].chan.send_raw(&bytes);
    svc
}

/// A console line's reply, as `exec` makes it.
fn exec_reply(system: &mut System, text: String, prompt: &str) -> ExecReply {
    let srcs = std::mem::take(&mut system.raised)
        .into_iter()
        .map(|s| s.code.to_string())
        .collect();
    if system.sysvals_changed {
        security::refresh(system);
    }
    let sysvals = if std::mem::take(&mut system.sysvals_changed) {
        system.sysvals.as_ref().map(sysval::SysVals::to_blob)
    } else {
        None
    };
    ExecReply {
        text,
        prompt: prompt.to_string(),
        srcs,
        sysvals,
    }
}

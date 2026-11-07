// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSQSRVR`, the SQL CLI's server-mode job: a prestart job of
//! `QSYSWRK` that takes one connection a job gives its pool, runs as
//! that job's user (or the one `SQLConnect` named), runs the
//! connection's statements with its own SQL and commitment control
//! (`CALL` made in its own runtime), then rolls back what is left, runs
//! as `QUSER` again and waits for the next, until `MAXUSE` connections
//! were served. Waiting in the pool it is `PSRW`, between a client's
//! requests `CNDW`, as IBM i shows it.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ipc::chan::Chan;
use sockets::relay;
use sql::embed::{Desc, Op, Request};
use sqlcli::server::proto::{FromServer, ToServer, done_message, request_of};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use crate::console::Console;
use crate::console::clisrv::PROGRAM;
use crate::console::sqlcall;
use crate::work::hostsvr::job::Job;

/// `MAXUSE(n)` of the request data the monitor passes; 0 for none.
fn maxuse(rqsdta: &str) -> u32 {
    rqsdta
        .split_once("MAXUSE(")
        .and_then(|(_, r)| r.split(')').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// Serve one connection until its job says goodbye or goes away.
fn serve(chan: &mut Chan, job: &Job, link: &Shared, me: &str) -> Result<(), String> {
    chan.set_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let Ok(ToServer::Hello {
        ticket,
        user,
        password,
    }) = chan.recv::<ToServer>()
    else {
        return Err("the connection did not say who it is".into());
    };
    chan.set_timeout(None).map_err(|e| e.to_string())?;
    match link
        .borrow_mut()
        .with(|l| l.sqlsrv_assume(ticket, user, password))
    {
        Ok(_client) => {}
        Err(r) => {
            let refused = FromServer::Refused {
                state: "08001".into(),
                code: -30082,
                text: format!("{}: {}", r.msgid, r.text),
            };
            return chan.send(&refused).map_err(|e| e.to_string());
        }
    }
    job.borrow_mut().swapped();
    chan.send(&FromServer::Ready {
        job: me.to_string(),
    })
    .map_err(|e| e.to_string())?;
    loop {
        stg::remote::status(link, Status::Cndw, "");
        let Ok(m) = chan.recv::<ToServer>() else {
            break;
        };
        stg::remote::status(link, Status::Run, "");
        let (scope, desc, inputs) = match m {
            ToServer::Run {
                scope,
                desc,
                inputs,
            } => (scope, desc, inputs),
            ToServer::Bye => break,
            ToServer::Hello { .. } => return Err("a second hello".into()),
        };
        let reply = if let Some(req) = request_of(&scope, &desc, &inputs) {
            let (reply, outs) =
                sqlcall::run(link, &req, |p| sqlcall::perform(&mut job.borrow_mut(), p));
            done_message(&reply, &outs)
        } else {
            let e = sql::SqlError::Unsupported {
                what: "statement".into(),
            };
            done_message(
                &sql::embed::Reply {
                    sqlca: sql::Sqlca::from_error(&e),
                    rows: Vec::new(),
                    call: None,
                },
                &[],
            )
        };
        if chan.send(&reply).is_err() {
            break;
        }
    }
    Ok(())
}

/// What a connection left: its changes rolled back, its cursors and
/// statements gone.
fn clean_up(link: &Shared) {
    let rollback = Request {
        scope: "CLI:".into(),
        desc: Desc {
            op: Op::Exec,
            text: "ROLLBACK".into(),
            ..Desc::default()
        },
        inputs: Vec::new(),
    };
    let _ = sqlcall::run(link, &rollback, |_| qrpgrt::sqlcall::Outcome {
        called: sql::exec::routine::Called::default(),
        outs: Vec::new(),
        answer: None,
    });
    link.borrow_mut().with(Client::sqlsrv_end);
}

/// Run the server job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = ctl.try_clone().ok();
    let job: Job = Rc::new(RefCell::new(Console::new(ctl, link, start)));
    job.borrow_mut().started();
    let pool = relay::pool(PROGRAM);
    // Idle as a member of the pool between its waits.
    let id = crate::work::hostsvr::daemon::internal_id(start);
    let me = format!("{:06}/{}/{}", start.number, start.user, start.name);
    let limit = maxuse(&start.rqsdta);
    let mut uses = 0;
    loop {
        stg::remote::status(link, Status::Psrw, "");
        if !job.borrow_mut().idle(Duration::ZERO) {
            break;
        }
        let taken = match relay::take_as(relay::SOCKET, &pool, Some(&id), None, 1000) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => {
                eprintln!("qsjob: {}: descriptor relay: {e:?}", start.name);
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        stg::remote::status(link, Status::Run, "");
        let mut chan = Chan::from_fd(taken.fd.into_owned());
        if let Err(e) = serve(&mut chan, &job, link, &me) {
            eprintln!("qsjob: {}: {e}", start.name);
        }
        drop(chan);
        clean_up(link);
        job.borrow_mut().swapped();
        uses += 1;
        if limit > 0 && uses >= limit {
            break;
        }
    }
    job.borrow_mut().ended(0);
    if let Some(mut c) = ending {
        let _ = c.send(&FromJob::Ending {
            reason: "prestart job ended".into(),
        });
    }
    std::process::exit(0)
}

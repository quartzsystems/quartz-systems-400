// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS daemon jobs (docs/NFS.md section 4): `QSYSWRK` routes
//! `QNFSRPCD` ... to `QZNFRPCD` ..., and each job runs [`lifecycle`]: it
//! says hello to the service's registry, starts serving (each daemon's
//! own module's `serve`: its sockets bound and registered with the
//! binder, its loop on a thread of its own), reports ready with what it
//! registered, then waits until `ENDNFSSVR`, `ENDJOB` or its
//! subsystem's end asks it to stop; the loop then finishes what is in
//! flight within a bound, the registrations go, and the job leaves the
//! registry. A loop that stops by itself is reported (`CPFA1B5`,
//! `CPFA1CB` for the binder) to `QSYSOPR`.

pub mod biod;
pub mod host;
pub mod mntd;
pub mod names;
pub mod nfsd;
pub mod nlmd;
pub mod nsmd;
pub mod rgyd;
pub mod rpcd;
pub mod running;
pub mod statd;
pub mod view;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use ifscmd::Msg;
use qir::MsgType;
use stg::nfs::{Daemon, NfsOp, NfsReply};
use stg::{Client, Shared};
use wm::job::Status;

use crate::console::Console;
use host::{Host, JobHost};
use running::{Fail, Running};

/// What a daemon was told when it registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Started {
    /// Its type.
    pub daemon: Daemon,
    /// Its number among its type's jobs, from 1.
    pub index: u16,
    /// The binder reads `/etc/RPCBTAB` back (`RTVRPCREG(*YES)`).
    pub restore: bool,
}

/// What a daemon's `serve` did.
#[derive(Debug)]
pub enum Serve {
    /// Serving, with what it registered with the binder: the job is
    /// ready.
    Ready(Running),
    /// It cannot serve, and why: the job ends without reporting ready.
    Unavailable(Fail),
}

/// What the lifecycle needs of the job, so it runs over a double.
pub trait DaemonEnv {
    /// One request to the service.
    fn op(&mut self, op: NfsOp) -> NfsReply;
    /// Wait a moment; `false` when the job or its subsystem is to end.
    fn idle(&mut self) -> bool;
    /// A message to the job log.
    fn log(&mut self, m: &Msg, kind: MsgType);
}

/// The message saying what failed, by its error number.
fn errno_msg(f: &Fail) -> Option<Msg> {
    (f.errno != 0).then(|| Msg::errno(ifs::errno::linux::from_linux(f.errno), ""))
}

/// Run a daemon job through its life; the job's end code.
pub fn lifecycle(
    env: &mut dyn DaemonEnv,
    daemon: Daemon,
    serve: &mut dyn FnMut(&mut dyn DaemonEnv, &Started) -> Serve,
) -> u16 {
    let started = match env.op(NfsOp::Hello { daemon }) {
        NfsReply::Hello { index, restore } => Started {
            daemon,
            index,
            restore,
        },
        NfsReply::Refused { msgid, data, .. } => {
            env.log(&Msg { id: msgid, data }, MsgType::Diag);
            return 20;
        }
        _ => return 20,
    };
    match serve(env, &started) {
        Serve::Unavailable(f) => {
            eprintln!("qsjob: {}: not started: {}", daemon.job_name(), f.what);
            if let Some(m) = errno_msg(&f) {
                env.log(&m, MsgType::Diag);
            }
            if daemon == Daemon::Rpc {
                env.log(&Msg::new("CPFA1CC").bin4(i64::from(f.errno)), MsgType::Diag);
            }
            env.log(
                &Msg::new("CPFA1BC").chars(daemon.keyword(), 8),
                MsgType::Diag,
            );
            env.op(NfsOp::Gone);
            20
        }
        Serve::Ready(mut r) => {
            env.op(NfsOp::Ready {
                ports: r.ports.clone(),
            });
            let mut failed = None;
            while env.idle() {
                if let Some(f) = r.failure() {
                    failed = Some(f);
                    break;
                }
                r.tick(env);
                if matches!(env.op(NfsOp::ShouldEnd), NfsReply::Flag { yes: true }) {
                    break;
                }
            }
            if let Some(f) = failed {
                eprintln!("qsjob: {}: ended: {}", daemon.job_name(), f.what);
                r.stop();
                if let Some(m) = errno_msg(&f) {
                    env.log(&m, MsgType::Diag);
                }
                let m = if daemon == Daemon::Rpc {
                    Msg::new("CPFA1CB").bin4(i64::from(f.errno))
                } else {
                    Msg::new("CPFA1B5")
                };
                env.log(&m, MsgType::Diag);
                // The service tells QSYSOPR and forgets the job.
                env.op(NfsOp::Failed { errno: f.errno });
                return 30;
            }
            if !r.stop() {
                eprintln!(
                    "qsjob: {}: calls still running at the end",
                    daemon.job_name()
                );
            }
            env.op(NfsOp::Gone);
            let done = if daemon == Daemon::Rpc {
                "CPCA1B5"
            } else {
                "CPCA1B2"
            };
            env.log(&Msg::new(done), MsgType::Comp);
            0
        }
    }
}

/// Start `daemon`'s service over `host`.
pub fn serve_with<H: Host>(env: &mut dyn DaemonEnv, started: Started, host: &Arc<H>) -> Serve {
    match started.daemon {
        Daemon::Rpc => rpcd::serve(env, started, host),
        Daemon::Bio => biod::serve(env, started, host),
        Daemon::Rgy => rgyd::serve(env, started, host),
        Daemon::Svr => nfsd::serve(env, started, host),
        Daemon::Mnt => mntd::serve(env, started, host),
        Daemon::Nsm => nsmd::serve(env, started, host),
        Daemon::Nlm => nlmd::serve(env, started, host),
        Daemon::Gss => Serve::Unavailable(Fail::new("Kerberos is not supported")),
    }
}

/// The routed job's environment.
struct JobEnv<'a> {
    c: &'a mut Console,
    link: &'a Shared,
    number: u32,
}

impl DaemonEnv for JobEnv<'_> {
    fn op(&mut self, op: NfsOp) -> NfsReply {
        self.link.borrow_mut().with(|l| l.nfs(op))
    }

    fn idle(&mut self) -> bool {
        if !self.c.idle(Duration::from_millis(500)) {
            return false;
        }
        let state = self.link.borrow_mut().with(Client::sbs_state);
        state.ending.is_none() && !state.end_jobs.iter().any(|(n, _)| *n == self.number)
    }

    fn log(&mut self, m: &Msg, kind: MsgType) {
        crate::command::log(self.c.job(), &m.id, &m.data, kind, None);
    }
}

/// The daemon `program` runs, as its routing entry calls it.
pub fn run(c: &mut Console, link: &Shared, daemon: Daemon, number: u32) -> u16 {
    stg::remote::status(link, Status::Run, &format!("PGM-{}", daemon.program()));
    let mut env = JobEnv { c, link, number };
    if daemon == Daemon::Gss {
        // Never submitted: Kerberos is not supported (docs/NFS.md
        // section 10).
        env.log(&Msg::new("CPI9C10"), MsgType::Info);
        env.log(&Msg::new("CPFA1BC").chars("*GSS", 8), MsgType::Diag);
        return 20;
    }
    let host = match JobHost::start(number, daemon.job_name()) {
        Ok(h) => Arc::new(h),
        Err(e) => {
            eprintln!("qsjob: {}: the link thread: {e}", daemon.job_name());
            env.log(
                &Msg::new("CPFA1BC").chars(daemon.keyword(), 8),
                MsgType::Diag,
            );
            return 20;
        }
    };
    let mut serve = |e: &mut dyn DaemonEnv, s: &Started| serve_with(e, *s, &host);
    stg::remote::status(link, Status::Selw, "");
    lifecycle(&mut env, daemon, &mut serve)
}

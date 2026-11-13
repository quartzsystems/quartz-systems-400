// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS daemon jobs (docs/NFS.md section 4): `QSYSWRK` routes
//! `QNFSRPCD` ... to `QZNFRPCD` ..., and each job runs [`lifecycle`]: it
//! says hello to the service's registry, serves (each daemon's own
//! module's `serve`), reports ready with what it registered, then waits
//! until `ENDNFSSVR`, `ENDJOB` or its subsystem's end asks it to stop,
//! and leaves the registry.

pub mod biod;
pub mod mntd;
pub mod nfsd;
pub mod nlmd;
pub mod nsmd;
pub mod rgyd;
pub mod rpcd;

use std::time::Duration;

use ifscmd::Msg;
use qir::MsgType;
use stg::nfs::{Daemon, NfsOp, NfsPort, NfsReply};
use stg::{Client, Shared};
use wm::job::Status;

use crate::console::Console;

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
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))] // Ready: the serving daemons of NFS wave 2
pub enum Serve {
    /// Serving, with what it registered with the binder: the job is
    /// ready.
    Ready(Vec<NfsPort>),
    /// It cannot serve: the job ends without reporting ready.
    Unavailable,
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
        Serve::Unavailable => {
            env.log(
                &Msg::new("CPI9C11").chars(daemon.keyword(), 4),
                MsgType::Info,
            );
            env.log(
                &Msg::new("CPFA1BC").chars(daemon.keyword(), 8),
                MsgType::Diag,
            );
            env.op(NfsOp::Gone);
            20
        }
        Serve::Ready(ports) => {
            env.op(NfsOp::Ready { ports });
            while env.idle() {
                if matches!(env.op(NfsOp::ShouldEnd), NfsReply::Flag { yes: true }) {
                    break;
                }
            }
            env.op(NfsOp::Gone);
            env.log(&Msg::new("CPCA1B2"), MsgType::Comp);
            0
        }
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
    let mut serve = |e: &mut dyn DaemonEnv, s: &Started| match daemon {
        Daemon::Rpc => rpcd::serve(e, *s),
        Daemon::Bio => biod::serve(e, *s),
        Daemon::Rgy => rgyd::serve(e, *s),
        Daemon::Svr => nfsd::serve(e, *s),
        Daemon::Mnt => mntd::serve(e, *s),
        Daemon::Nsm => nsmd::serve(e, *s),
        Daemon::Nlm | Daemon::Gss => nlmd::serve(e, *s),
    };
    stg::remote::status(link, Status::Selw, "");
    lifecycle(&mut env, daemon, &mut serve)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A registry of one job and a clock of pauses.
    struct Fake {
        ops: Vec<String>,
        log: Vec<String>,
        idles: u32,
        end_after: u32,
        refuse: bool,
    }

    impl DaemonEnv for Fake {
        fn op(&mut self, op: NfsOp) -> NfsReply {
            self.ops.push(
                format!("{op:?}")
                    .split([' ', '{'])
                    .next()
                    .unwrap_or("")
                    .to_string(),
            );
            match op {
                NfsOp::Hello { .. } if self.refuse => NfsReply::Refused {
                    msgid: "CPFA1B4".into(),
                    data: Vec::new(),
                    text: String::new(),
                },
                NfsOp::Hello { .. } => NfsReply::Hello {
                    index: 1,
                    restore: true,
                },
                NfsOp::ShouldEnd => NfsReply::Flag {
                    yes: self.idles >= self.end_after,
                },
                _ => NfsReply::Done,
            }
        }
        fn idle(&mut self) -> bool {
            self.idles += 1;
            true
        }
        fn log(&mut self, m: &Msg, _: MsgType) {
            self.log.push(m.id.clone());
        }
    }

    fn fake() -> Fake {
        Fake {
            ops: Vec::new(),
            log: Vec::new(),
            idles: 0,
            end_after: 3,
            refuse: false,
        }
    }

    /// A test-only serve that serves: ready, until ENDNFSSVR.
    #[test]
    fn a_serving_daemon_is_ready_until_asked_to_end() {
        let mut f = fake();
        let mut serve = |_: &mut dyn DaemonEnv, s: &Started| {
            assert!(s.restore && s.index == 1);
            Serve::Ready(vec![NfsPort {
                program: 100_000,
                version: 4,
                proto: "tcp".into(),
                port: 111,
            }])
        };
        assert_eq!(lifecycle(&mut f, Daemon::Rpc, &mut serve), 0);
        assert_eq!(f.ops.first().map(String::as_str), Some("Hello"));
        assert_eq!(f.ops.get(1).map(String::as_str), Some("Ready"));
        assert_eq!(f.ops.last().map(String::as_str), Some("Gone"));
        assert_eq!(f.idles, 3);
        assert_eq!(f.log, ["CPCA1B2"]);
    }

    /// Today's daemons: none reports ready, each ends saying why.
    #[test]
    fn every_daemon_today_ends_without_claiming_to_serve() {
        for d in Daemon::ALL {
            let mut f = fake();
            let mut serve = |e: &mut dyn DaemonEnv, s: &Started| match d {
                Daemon::Rpc => rpcd::serve(e, *s),
                Daemon::Bio => biod::serve(e, *s),
                Daemon::Rgy => rgyd::serve(e, *s),
                Daemon::Svr => nfsd::serve(e, *s),
                Daemon::Mnt => mntd::serve(e, *s),
                Daemon::Nsm => nsmd::serve(e, *s),
                Daemon::Nlm | Daemon::Gss => nlmd::serve(e, *s),
            };
            assert_eq!(lifecycle(&mut f, d, &mut serve), 20, "{d:?}");
            assert!(!f.ops.contains(&"Ready".to_string()), "{d:?}");
            assert_eq!(f.ops.last().map(String::as_str), Some("Gone"));
            assert_eq!(f.log, ["CPI9C11", "CPFA1BC"]);
        }
    }

    #[test]
    fn a_job_the_registry_refuses_ends() {
        let mut f = fake();
        f.refuse = true;
        let mut serve = |_: &mut dyn DaemonEnv, _: &Started| Serve::Ready(Vec::new());
        assert_eq!(lifecycle(&mut f, Daemon::Svr, &mut serve), 20);
        assert_eq!(f.ops, ["Hello"]);
    }
}

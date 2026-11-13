// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Nfs` service (version 11, draft) on a connection job's
//! secondary thread, as the signed-on profile: the export table, the
//! daemon registry and the NFS options through the storage service's
//! `nfs` request; `EXPORTFS`, `MOUNT` and `UNMOUNT` through the analyzer
//! to their processing programs in the service, `STRNFSSVR` and
//! `ENDNFSSVR` as the job runs them (`command::nfs`), waiting on this
//! thread's link.

use std::time::Duration;

use ifscmd::Msg;
use qap_proto::ops::nfs::{DaemonType, MountedFs, NfsOptions, Registration};
use qap_proto::{ServerMessage, Status};
use qapd::host::Refusal;
use qapd::nfs::{DaemonEntry, ExportEntry, NfsHost, status_of};
use qir::MsgType;
use stg::Client as _;
use stg::nfs::{Daemon, NfsOp, NfsReply};

use super::host::{JobThread, message, message_bytes};
use super::manage;
use crate::command::nfs::{Env, PAUSE_MS, endnfssvr, strnfssvr};
use stg::Shared;

/// The protocol's type of a registry's daemon; `None` for `*GSS`.
fn kind(d: Daemon) -> Option<DaemonType> {
    Some(match d {
        Daemon::Rpc => DaemonType::Rpc,
        Daemon::Bio => DaemonType::Bio,
        Daemon::Rgy => DaemonType::Rgy,
        Daemon::Svr => DaemonType::Svr,
        Daemon::Mnt => DaemonType::Mnt,
        Daemon::Nsm => DaemonType::Nsm,
        Daemon::Nlm => DaemonType::Nlm,
        Daemon::Gss => return None,
    })
}

/// `STRNFSSVR` and `ENDNFSSVR` over this thread's link.
struct ThreadEnv<'a> {
    shared: &'a Shared,
}

impl Env for ThreadEnv<'_> {
    fn op(&mut self, op: NfsOp) -> NfsReply {
        self.shared.borrow_mut().with(|l| l.nfs(op))
    }

    fn log(&mut self, _m: &Msg, _kind: MsgType) {}

    fn pause(&mut self) -> bool {
        std::thread::sleep(Duration::from_millis(PAUSE_MS));
        true
    }
}

impl JobThread {
    fn nfs_op(&mut self, op: NfsOp) -> Result<NfsReply, Refusal> {
        match self.link().with(|l| l.nfs(op)) {
            NfsReply::Refused { msgid, data, .. } => {
                let message = message_bytes(&mut self.link(), &msgid, &data);
                Err(Refusal {
                    status: status_of(&msgid),
                    message,
                })
            }
            r => Ok(r),
        }
    }

    /// A job-site daemon command, as `command::nfs::run` takes its
    /// parameters.
    fn daemon_command(&mut self, line: &str) -> Result<(), Refusal> {
        let r = manage::analyze(&self.cx(), line)?;
        let p = crate::command::parms_of(&r);
        let int = |k: &str, d: i64| p.int(k).unwrap_or(d);
        let secs = |k: &str| match int(k, 30) {
            -1 => None,
            n => Some(u32::try_from(n).unwrap_or(30)),
        };
        let count = |k: &str| u16::try_from(int(k, 1)).unwrap_or(1);
        let cx = self.cx();
        let mut env = ThreadEnv { shared: cx.shared };
        let ran = if r.name == "STRNFSSVR" {
            strnfssvr(
                &mut env,
                int("SERVER", -1),
                count("NBRSVR"),
                count("NBRBIO"),
                matches!(p.name("RTVRPCREG").as_str(), "Y" | "*YES"),
                secs("STRJOBTIMO"),
            )
        } else {
            endnfssvr(&mut env, int("SERVER", -1), secs("ENDJOBTIMO"))
        };
        ran.map_err(|f| Refusal {
            status: status_of(&f.msgid),
            message: message_bytes(&mut self.link(), &f.msgid, &f.data),
        })
    }
}

impl NfsHost for JobThread {
    fn exports(&mut self) -> Result<Vec<ExportEntry>, Refusal> {
        Ok(match self.nfs_op(NfsOp::Exports)? {
            NfsReply::Table { exports, .. } => exports
                .into_iter()
                .map(|e| ExportEntry {
                    id: e.id,
                    entry: e.entry,
                    anon_uid: e.anon_uid,
                    anon_user: e.anon_profile,
                    by: e.by,
                    since: e.since,
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    fn daemons(&mut self) -> Result<Vec<DaemonEntry>, Refusal> {
        Ok(match self.nfs_op(NfsOp::Daemons)? {
            NfsReply::Daemons { jobs } => jobs
                .into_iter()
                .filter_map(|j| {
                    Some(DaemonEntry {
                        daemon: kind(j.daemon)?,
                        job: j.job,
                        ready: j.ready,
                        ending: j.ending,
                        registrations: j
                            .ports
                            .into_iter()
                            .map(|p| Registration {
                                program: p.program,
                                version: p.version,
                                proto: p.proto,
                                port: p.port,
                            })
                            .collect(),
                        since: j.since,
                    })
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    // BLOCKED(rel): the IFS mount table (NFS-4): `IfsClient::mounts()`
    // over this thread's IFS client, each `MountInfo` as a `MountedFs`;
    // until then `TCP1A78`.
    fn mounts(&mut self) -> Result<Vec<MountedFs>, Refusal> {
        Err(Refusal {
            status: Status::Error,
            message: message(&mut self.link(), "TCP1A78", &[]),
        })
    }

    fn command(&mut self, line: &str) -> Result<(), Refusal> {
        let name = line.split_whitespace().next().unwrap_or("");
        if matches!(name, "STRNFSSVR" | "ENDNFSSVR") {
            return self.daemon_command(line);
        }
        let r = manage::analyze(&self.cx(), line)?;
        manage::service(&self.cx(), &r)
            .map(|_| ())
            .map_err(|mut e| {
                if e.status == Status::Error {
                    e.status = status_of(&e.message.id);
                }
                e
            })
    }

    fn options(&mut self) -> Result<NfsOptions, Refusal> {
        Ok(match self.nfs_op(NfsOp::Options)? {
            NfsReply::Options { options: o } => NfsOptions {
                flavors: o.flavors,
                domain: o.domain,
                max_threads: o.max_threads,
                handle_cache: o.handle_cache,
                utf8_validation: o.utf8_validation,
            },
            _ => NfsOptions::default(),
        })
    }

    fn set_options(&mut self, o: &NfsOptions) -> Result<(), Refusal> {
        if o.flavors.iter().any(|f| *f != 1) {
            // As QZNFNFSO says it: only AUTH_SYS.
            return Err(Refusal {
                status: Status::Error,
                message: message(&mut self.link(), "CPFA1BB", &[]),
            });
        }
        let options = stg::nfs::NfsOptions {
            flavors: o.flavors.clone(),
            domain: o.domain.clone(),
            max_threads: o.max_threads,
            handle_cache: o.handle_cache,
            utf8_validation: o.utf8_validation,
        };
        self.nfs_op(NfsOp::SetOptions { options }).map(|_| ())
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        message(&mut self.link(), id, data)
    }
}

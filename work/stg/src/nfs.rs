// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS over the link (docs/NFS.md sections 4, 6 and 8): one request
//! (`nfs`, tag 1540) carrying an [`NfsOp`] and answered by an
//! [`NfsReply`], and the APIs' bytes (`nfs_api`, tag 1541). The export
//! table, the daemon registry and the NFS options are the storage
//! service's; the daemon jobs report into the registry and read the
//! table, the commands start and end the daemons through it.

use ipc::{wire_enum, wire_struct};

/// A type of NFS daemon, in the order `STRNFSSVR SERVER(*ALL)` starts
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Daemon {
    /// `*RPC`: the binder.
    Rpc,
    /// `*BIO`: the client's block I/O daemons.
    Bio,
    /// `*RGY`: the `NFSv4` identity mapping.
    Rgy,
    /// `*SVR`: the server.
    Svr,
    /// `*MNT`: the mount daemon.
    Mnt,
    /// `*NSM`: the status monitor.
    Nsm,
    /// `*NLM`: the lock manager.
    Nlm,
    /// `*GSS`: Kerberos, which QS/400 does not have (refused).
    Gss,
}

wire_enum!(Daemon {
    0 => Rpc,
    1 => Bio,
    2 => Rgy,
    3 => Svr,
    4 => Mnt,
    5 => Nsm,
    6 => Nlm,
    7 => Gss,
});

impl Daemon {
    /// The types `SERVER(*ALL)` starts, in order; ending goes the other
    /// way. `*GSS` is not among them.
    pub const ALL: [Self; 7] = [
        Self::Rpc,
        Self::Bio,
        Self::Rgy,
        Self::Svr,
        Self::Mnt,
        Self::Nsm,
        Self::Nlm,
    ];

    /// The `SERVER` special value.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Rpc => "*RPC",
            Self::Bio => "*BIO",
            Self::Rgy => "*RGY",
            Self::Svr => "*SVR",
            Self::Mnt => "*MNT",
            Self::Nsm => "*NSM",
            Self::Nlm => "*NLM",
            Self::Gss => "*GSS",
        }
    }

    /// From `SERVER`'s value in the parameter block (IBM's mapped
    /// integers): `None` for `*ALL` (-1) and anything else.
    #[must_use]
    pub fn from_mapped(v: i64) -> Option<Self> {
        Some(match v {
            -2 => Self::Rpc,
            -3 => Self::Bio,
            -4 => Self::Svr,
            -5 => Self::Mnt,
            -6 => Self::Nsm,
            -7 => Self::Nlm,
            -8 => Self::Gss,
            -9 => Self::Rgy,
            _ => return None,
        })
    }

    /// The job's name and its routing data (`QSYSWRK`'s compare value).
    #[must_use]
    pub fn job_name(self) -> &'static str {
        match self {
            Self::Rpc => "QNFSRPCD",
            Self::Bio => "QNFSBIOD",
            Self::Rgy => "QNFSRGYD",
            Self::Svr => "QNFSNFSD",
            Self::Mnt => "QNFSMNTD",
            Self::Nsm => "QNFSNSMD",
            Self::Nlm => "QNFSNLMD",
            Self::Gss => "QNFSGSSD",
        }
    }

    /// The program the routing entry calls.
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            Self::Rpc => "QZNFRPCD",
            Self::Bio => "QZNFBIOD",
            Self::Rgy => "QZNFRGYD",
            Self::Svr => "QZNFNFSD",
            Self::Mnt => "QZNFMNTD",
            Self::Nsm => "QZNFNSMD",
            Self::Nlm => "QZNFNLMD",
            Self::Gss => "QZNFGSSD",
        }
    }

    /// The daemon a program runs.
    #[must_use]
    pub fn of_program(pgm: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Self::Gss])
            .find(|d| d.program().eq_ignore_ascii_case(pgm))
    }

    /// The daemon a job of that name is.
    #[must_use]
    pub fn of_job(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Self::Gss])
            .find(|d| d.job_name().eq_ignore_ascii_case(name))
    }

    /// Whether it registers with the binder, so needs it running.
    #[must_use]
    pub fn needs_binder(self) -> bool {
        matches!(self, Self::Svr | Self::Mnt | Self::Nsm | Self::Nlm)
    }

    /// The job description it is submitted with.
    #[must_use]
    pub fn jobd(self) -> &'static str {
        if self == Self::Rpc {
            "QP0LRPCD"
        } else {
            "QP0LNFSD"
        }
    }
}

/// One export of the table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NfsExport {
    /// Its export ID, the same for the same path through this IPL.
    pub id: u32,
    /// The entry as a line of `/etc/exports` (`nfs::exports::file`), the
    /// host options' `*ASCII` and `*JOBCCSID` resolved to numbers.
    pub entry: String,
    /// The anonymous UID: `ANON=` or the UID of `QNFSANON`; `None` when
    /// `ANON=-1` refuses unknown users.
    pub anon_uid: Option<u32>,
    /// The profile of that UID (`*NONE` when none has it).
    pub anon_profile: String,
    /// The job that exported it, qualified.
    pub by: String,
    /// When, seconds since the epoch.
    pub since: u64,
}

wire_struct!(NfsExport {
    id,
    entry,
    anon_uid,
    anon_profile,
    by,
    since
});

/// A program and version a daemon registered with the binder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NfsPort {
    /// The RPC program number.
    pub program: u32,
    /// Its version.
    pub version: u32,
    /// `tcp` or `udp`.
    pub proto: String,
    /// The port.
    pub port: u16,
}

wire_struct!(NfsPort {
    program,
    version,
    proto,
    port
});

/// One daemon job as the registry holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NfsDaemonJob {
    /// Its type.
    pub daemon: Daemon,
    /// Its number among that type's jobs, from 1.
    pub index: u16,
    /// The job, qualified (`000123/QSECOFR/QNFSNFSD`).
    pub job: String,
    /// It reported ready.
    pub ready: bool,
    /// `ENDNFSSVR` asked it to end.
    pub ending: bool,
    /// What it registered with the binder.
    pub ports: Vec<NfsPort>,
    /// When it said hello.
    pub since: u64,
}

wire_struct!(NfsDaemonJob {
    daemon,
    index,
    job,
    ready,
    ending,
    ports,
    since
});

/// The NFS options of `QZNFNFSO` (`NFOP0100` to `NFOP0300`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NfsOptions {
    /// The security flavors the server offers by default (1 `AUTH_SYS`).
    pub flavors: Vec<u32>,
    /// The `NFSv4` domain; empty is the TCP/IP domain name.
    pub domain: String,
    /// The most threads a server job runs; 0 is no limit.
    pub max_threads: u32,
    /// The client's file handle cache, in entries.
    pub handle_cache: u32,
    /// Path names from clients are checked to be UTF-8.
    pub utf8_validation: bool,
}

impl Default for NfsOptions {
    fn default() -> Self {
        Self {
            flavors: vec![1],
            domain: String::new(),
            max_threads: 0,
            handle_cache: 1024,
            utf8_validation: true,
        }
    }
}

wire_struct!(NfsOptions {
    flavors,
    domain,
    max_threads,
    handle_cache,
    utf8_validation
});

/// One request.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)] // the fields are the variants'
pub enum NfsOp {
    /// The export table and its generation.
    Exports,
    /// The table's generation alone: it changes with every export and
    /// unexport, so a daemon asks this to know when to read the table.
    Generation,
    /// The export of `path`, or of the directory that holds it nearest:
    /// the reply is [`NfsReply::Table`] with one entry or none.
    ExportOf { path: String },
    /// A daemon job starts: it is registered as the caller's job. Only
    /// a job named as the daemon's (`QNFSNFSD` ...) may.
    Hello { daemon: Daemon },
    /// The calling daemon job is ready, with what it registered.
    Ready { ports: Vec<NfsPort> },
    /// The calling daemon job ends: it leaves the registry.
    Gone,
    /// Whether the calling daemon job was asked to end.
    ShouldEnd,
    /// Every daemon job.
    Daemons,
    /// Submit `count` jobs of `daemon` (`STRNFSSVR`; one for every type
    /// but the server and block I/O daemons), `rtvrpcreg` for the
    /// binder: the reply says how many were submitted and how many ran
    /// or waited on the queue already; a count of 0 only asks that.
    /// `*IOSYSCFG`.
    Start {
        daemon: Daemon,
        count: u16,
        rtvrpcreg: bool,
    },
    /// Ask every job of `daemon` to end (`ENDNFSSVR`). `*IOSYSCFG`.
    End { daemon: Daemon },
    /// The NFS options.
    Options,
    /// Change the NFS options (`QZNFNFSO`): `*IOSYSCFG`.
    SetOptions { options: NfsOptions },
    /// From `QNFSNLMD` only: this system's status monitor says the
    /// server `name` (its `mon_name`) restarted with `state`; the NFS
    /// client's mounts of it reclaim their locks (docs/NFS.md 7.7).
    ServerRestarted { name: String, state: i32 },
}

wire_enum!(NfsOp {
    0 => Exports,
    1 => Generation,
    2 => ExportOf { path },
    3 => Hello { daemon },
    4 => Ready { ports },
    5 => Gone,
    6 => ShouldEnd,
    7 => Daemons,
    8 => Start { daemon, count, rtvrpcreg },
    9 => End { daemon },
    10 => Options,
    11 => SetOptions { options },
    12 => ServerRestarted { name, state },
});

/// The answer to an [`NfsOp`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum NfsReply {
    /// Done.
    Done,
    /// The export table.
    Table {
        generation: u64,
        exports: Vec<NfsExport>,
    },
    /// The table's generation.
    Generation { generation: u64 },
    /// A daemon job registered: its number among its type's, and for the
    /// binder whether to read `/etc/RPCBTAB` back (`RTVRPCREG(*YES)`).
    Hello { index: u16, restore: bool },
    /// A yes or no.
    Flag { yes: bool },
    /// Every daemon job.
    Daemons { jobs: Vec<NfsDaemonJob> },
    /// `Start`: jobs submitted now, and those running before.
    Started { submitted: u16, running: u16 },
    /// The NFS options.
    Options { options: NfsOptions },
    /// Refused with a message: its identifier, its substitution data and
    /// its first-level text.
    Refused {
        msgid: String,
        data: Vec<u8>,
        text: String,
    },
}

wire_enum!(NfsReply {
    0 => Done,
    1 => Table { generation, exports },
    2 => Generation { generation },
    3 => Hello { index, restore },
    4 => Flag { yes },
    5 => Daemons { jobs },
    6 => Started { submitted, running },
    7 => Options { options },
    8 => Refused { msgid, data, text },
});

/// A mounted file system's type as `/etc/vfs` names it, from the mount
/// table's kind (`*NFS`, `*UDFS`, or the file system's own name) and
/// its options (`vers=4` makes an NFS mount `nfs4`).
#[must_use]
pub fn vfs_name(kind: &str, options: &str) -> String {
    let v4 = options
        .split(',')
        .any(|o| o.trim().to_ascii_lowercase().starts_with("vers=4"));
    match kind {
        "*NFS" if v4 => "nfs4",
        "*NFS" => "nfs3",
        "*UDFS" => "udfs",
        "dev" => "root",
        "QSYS.LIB" => "qsys",
        "QNTC" => "ntc",
        "QFileSvr.400" => "rfs",
        other => return other.trim_start_matches('*').to_ascii_lowercase(),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfs_names() {
        assert_eq!(vfs_name("*NFS", "rw,vers=4.1,hard"), "nfs4");
        assert_eq!(vfs_name("*NFS", "rw,hard"), "nfs3");
        assert_eq!(vfs_name("*UDFS", "rw"), "udfs");
        assert_eq!(vfs_name("QOpenSys", "rw"), "qopensys");
        assert_eq!(vfs_name("QSYS.LIB", "rw"), "qsys");
        assert_eq!(vfs_name("dev", "rw"), "root");
        assert_eq!(vfs_name("QOPT", "ro"), "qopt");
    }

    #[test]
    fn requests_and_replies_cross_the_wire_whole() {
        for op in [
            NfsOp::Exports,
            NfsOp::ExportOf {
                path: "/home/a".into(),
            },
            NfsOp::Hello {
                daemon: Daemon::Nlm,
            },
            NfsOp::Ready {
                ports: vec![NfsPort {
                    program: 100_003,
                    version: 3,
                    proto: "tcp".into(),
                    port: 2049,
                }],
            },
            NfsOp::Start {
                daemon: Daemon::Svr,
                count: 4,
                rtvrpcreg: true,
            },
            NfsOp::SetOptions {
                options: NfsOptions::default(),
            },
            NfsOp::ServerRestarted {
                name: "nfssrv".into(),
                state: 7,
            },
        ] {
            let back: NfsOp = ipc::decode(&ipc::encode(&op)).expect("decodes");
            assert_eq!(back, op);
        }
        let r = NfsReply::Daemons {
            jobs: vec![NfsDaemonJob {
                daemon: Daemon::Svr,
                index: 2,
                job: "000123/QSECOFR/QNFSNFSD".into(),
                ready: true,
                ending: false,
                ports: Vec::new(),
                since: 9,
            }],
        };
        let back: NfsReply = ipc::decode(&ipc::encode(&r)).expect("decodes");
        assert_eq!(back, r);
        let t = NfsReply::Table {
            generation: 3,
            exports: vec![NfsExport {
                id: 1,
                entry: "/home -ro".into(),
                anon_uid: None,
                anon_profile: "*NONE".into(),
                by: "j".into(),
                since: 1,
            }],
        };
        assert_eq!(ipc::decode::<NfsReply>(&ipc::encode(&t)).unwrap(), t);
    }

    #[test]
    fn daemon_names() {
        assert_eq!(Daemon::from_mapped(-4), Some(Daemon::Svr));
        assert_eq!(Daemon::from_mapped(-9), Some(Daemon::Rgy));
        assert_eq!(Daemon::from_mapped(-1), None);
        assert_eq!(Daemon::of_program("qznfnfsd"), Some(Daemon::Svr));
        assert_eq!(Daemon::of_job("QNFSGSSD"), Some(Daemon::Gss));
        assert!(Daemon::Nlm.needs_binder() && !Daemon::Bio.needs_binder());
        assert_eq!(Daemon::Rpc.jobd(), "QP0LRPCD");
    }
}

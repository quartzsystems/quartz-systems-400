// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Nfs` (version 11, draft): the export table, the NFS daemons, the
//! mount table and the NFS options, each as the command or API a
//! program would use (`EXPORTFS`, `STRNFSSVR`, `ENDNFSSVR`, `MOUNT`,
//! `UNMOUNT`, `QZNFNFSO`) does it (docs/QAP.md, "Service 13 `Nfs`").

use ipc::{wire_enum, wire_struct};

use crate::types::JobId;

/// One host's options of an export (`HOSTOPT`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostOption {
    /// The host, address, `@network/length` or netgroup.
    pub host: String,
    /// The CCSID file data is converted to; 65535 no conversion
    /// (`*BINARY`).
    pub data_ccsid: u32,
    /// The CCSID of path names; 0 in a request is the job's ASCII CCSID
    /// (`*ASCII`).
    pub path_ccsid: u32,
    /// Writes reach the disk before the answer (`*SYNC`).
    pub sync: bool,
}

wire_struct!(HostOption {
    host,
    data_ccsid,
    path_ccsid,
    sync
});

/// The options of an export (`EXPORTFS -O`).
#[allow(clippy::struct_excessive_bools)] // the options are flags
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    /// `ro`.
    pub read_only: bool,
    /// `rw=`: with `read_only`, the hosts that may still write.
    pub rw_hosts: Vec<String>,
    /// `access=`: the only hosts that may mount; `None` every host.
    pub access_hosts: Option<Vec<String>>,
    /// `root=`: the hosts whose UID 0 is `QSECOFR`.
    pub root_hosts: Vec<String>,
    /// `anon=`: the UID of unknown users, -1 refusing them; `None` the
    /// UID of `QNFSANON`.
    pub anon: Option<i64>,
    /// `nosuid`.
    pub nosuid: bool,
    /// Offered in NFS version 3.
    pub v3: bool,
    /// Offered in NFS version 4.
    pub v4: bool,
    /// `public`.
    pub public: bool,
    /// `root`: the root of the `NFSv4` pseudo file system.
    pub pseudo_root: bool,
    /// `nfsroot=`; `None` is `/`.
    pub nfsroot: Option<String>,
    /// `exname=`; `None` is the path.
    pub exname: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            rw_hosts: Vec::new(),
            access_hosts: None,
            root_hosts: Vec::new(),
            anon: None,
            nosuid: false,
            v3: true,
            v4: true,
            public: false,
            pseudo_root: false,
            nfsroot: None,
            exname: None,
        }
    }
}

wire_struct!(ExportOptions {
    read_only,
    rw_hosts,
    access_hosts,
    root_hosts,
    anon,
    nosuid,
    v3,
    v4,
    public,
    pseudo_root,
    nfsroot,
    exname
});

/// An export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Export {
    /// Its export ID (ignored in requests).
    pub id: u32,
    /// The directory, from `/`.
    pub path: String,
    /// Its options.
    pub options: ExportOptions,
    /// Its host options, at most 10.
    pub hosts: Vec<HostOption>,
    /// The anonymous UID in effect; `None` when unknown users are
    /// refused (ignored in requests).
    pub anon_uid: Option<u32>,
    /// That UID's profile, when one has it (ignored in requests).
    pub anon_user: Option<String>,
    /// The job that exported it (ignored in requests).
    pub exported_by: Option<JobId>,
    /// When, milliseconds since 1970-01-01 UTC (ignored in requests).
    pub since: i64,
}

wire_struct!(Export {
    id,
    path,
    options,
    hosts,
    anon_uid,
    anon_user,
    exported_by,
    since
});

/// `ListExports`' request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListExports {
    /// The first to return, from 0.
    pub start: u32,
    /// At most this many; 0 for all.
    pub max: u32,
}

wire_struct!(ListExports { start, max });

/// `ListExports`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Exports {
    /// How many there are.
    pub total: u32,
    /// In the order they were exported.
    pub exports: Vec<Export>,
}

wire_struct!(Exports { total, exports });

/// A directory: `GetExport`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportRef {
    /// The directory.
    pub path: String,
}

wire_struct!(ExportRef { path });

/// `PutExport`'s request: export the directory with these options, or
/// export it again with them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PutExport {
    /// The export as it is to be.
    pub export: Export,
    /// Also put the entry in `/etc/exports` (`-F`).
    pub update_file: bool,
}

wire_struct!(PutExport {
    export,
    update_file
});

/// `RemoveExport`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoveExport {
    /// The directory.
    pub path: String,
    /// Also take the entry out of `/etc/exports` (`-I -F`).
    pub update_file: bool,
}

wire_struct!(RemoveExport { path, update_file });

/// A type of NFS daemon, in the order `STRNFSSVR SERVER(*ALL)` starts
/// them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DaemonType {
    /// `*RPC`: the binder.
    #[default]
    Rpc,
    /// `*BIO`: block I/O for this system's mounts.
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
}

wire_enum!(DaemonType {
    0 => Rpc,
    1 => Bio,
    2 => Rgy,
    3 => Svr,
    4 => Mnt,
    5 => Nsm,
    6 => Nlm,
});

impl DaemonType {
    /// Every type, in value order (`STRNFSSVR SERVER(*ALL)`'s).
    pub const ALL: [DaemonType; 7] = [
        DaemonType::Rpc,
        DaemonType::Bio,
        DaemonType::Rgy,
        DaemonType::Svr,
        DaemonType::Mnt,
        DaemonType::Nsm,
        DaemonType::Nlm,
    ];

    /// The `SERVER` value.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            DaemonType::Rpc => "*RPC",
            DaemonType::Bio => "*BIO",
            DaemonType::Rgy => "*RGY",
            DaemonType::Svr => "*SVR",
            DaemonType::Mnt => "*MNT",
            DaemonType::Nsm => "*NSM",
            DaemonType::Nlm => "*NLM",
        }
    }
}

/// A program a daemon registered with the binder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registration {
    /// The RPC program.
    pub program: u32,
    /// Its version.
    pub version: u32,
    /// `tcp` or `udp`.
    pub proto: String,
    /// The port.
    pub port: u16,
}

wire_struct!(Registration {
    program,
    version,
    proto,
    port
});

/// One daemon job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonJob {
    /// Its type.
    pub daemon: DaemonType,
    /// The job.
    pub job: JobId,
    /// It reported ready.
    pub ready: bool,
    /// It was asked to end.
    pub ending: bool,
    /// What it registered.
    pub registrations: Vec<Registration>,
    /// Since, milliseconds since 1970-01-01 UTC.
    pub since: i64,
}

wire_struct!(DaemonJob {
    daemon,
    job,
    ready,
    ending,
    registrations,
    since
});

/// `ListDaemons`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Daemons {
    /// By type, then job.
    pub jobs: Vec<DaemonJob>,
}

wire_struct!(Daemons { jobs });

/// `StartDaemons`' request (`STRNFSSVR`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StartDaemons {
    /// One type; `None` every type (`*ALL`).
    pub daemon: Option<DaemonType>,
    /// Server jobs (`NBRSVR`), 1 to 20; 0 the default, 1.
    pub servers: u16,
    /// Block I/O jobs (`NBRBIO`), 1 to 20; 0 the default, 1.
    pub block_io: u16,
    /// The binder reads its registrations back (`RTVRPCREG(*YES)`).
    pub retrieve_registrations: bool,
    /// Seconds to wait for the jobs to be ready; `None` no limit.
    pub timeout: Option<u32>,
}

wire_struct!(StartDaemons {
    daemon,
    servers,
    block_io,
    retrieve_registrations,
    timeout
});

/// `EndDaemons`' request (`ENDNFSSVR`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndDaemons {
    /// One type; `None` every type (`*ALL`).
    pub daemon: Option<DaemonType>,
    /// Seconds to wait for the jobs to end; `None` no limit.
    pub timeout: Option<u32>,
}

wire_struct!(EndDaemons { daemon, timeout });

/// A mounted file system.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountedFs {
    /// Its mount ID.
    pub id: u32,
    /// Its type as `/etc/vfs` names it (`root`, `udfs`, `nfs3`, `nfs4` ...).
    pub kind: String,
    /// The directory mounted over.
    pub over: String,
    /// What is mounted (`host:/path`, a UDFS); empty for none.
    pub source: String,
    /// Its options.
    pub options: String,
    /// Read only.
    pub read_only: bool,
    /// A remote file system's server answers; `None` for a local one.
    pub responding: Option<bool>,
}

wire_struct!(MountedFs {
    id,
    kind,
    over,
    source,
    options,
    read_only,
    responding
});

/// `ListMounts`' reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mounts {
    /// In mount order.
    pub mounts: Vec<MountedFs>,
}

wire_struct!(Mounts { mounts });

/// `MountNfs`' request (`MOUNT TYPE(*NFS)`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MountNfs {
    /// The server.
    pub host: String,
    /// The directory it exports.
    pub path: String,
    /// The directory here to mount over.
    pub over: String,
    /// The options; `None` the defaults (`*DFT`).
    pub options: Option<String>,
}

wire_struct!(MountNfs {
    host,
    path,
    over,
    options
});

/// What `Unmount` unmounts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnmountKind {
    /// An NFS mount (`TYPE(*NFS)`).
    #[default]
    Nfs,
    /// A user-defined file system (`TYPE(*UDFS)`).
    Udfs,
}

wire_enum!(UnmountKind {
    0 => Nfs,
    1 => Udfs,
});

/// `Unmount`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unmount {
    /// What kind.
    pub kind: UnmountKind,
    /// The directory mounted over.
    pub over: String,
}

wire_struct!(Unmount { kind, over });

/// The NFS options (`QZNFNFSO`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NfsOptions {
    /// The security flavors offered (1 `AUTH_SYS`).
    pub flavors: Vec<u32>,
    /// The `NFSv4` domain; empty the TCP/IP domain name.
    pub domain: String,
    /// The most threads a server job runs; 0 no limit.
    pub max_threads: u32,
    /// The client's file handle cache, in entries.
    pub handle_cache: u32,
    /// Path names from clients are checked to be UTF-8.
    pub utf8_validation: bool,
}

wire_struct!(NfsOptions {
    flavors,
    domain,
    max_threads,
    handle_cache,
    utf8_validation
});

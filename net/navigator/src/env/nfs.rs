// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS: the export table, the daemon jobs and the mount table, and
//! every change as the command itself (`EXPORTFS`, `STRNFSSVR`,
//! `ENDNFSSVR`, `MOUNT`, `UNMOUNT`) run as the signed-on user through
//! [`Nfs::run`]: the command checks the authority, validates and words
//! the refusal.

use qsweb::msg::Message;

/// One host option of an export (`HOSTOPT`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostOption {
    /// The host, address, network or netgroup.
    pub host: String,
    /// The data CCSID: `*BINARY` or a number.
    pub data: String,
    /// The path name CCSID, a number.
    pub path: String,
    /// Writes are synchronous (`*SYNC`).
    pub sync: bool,
}

/// One export of the table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the options are flags
pub struct Export {
    /// Its export ID.
    pub id: u32,
    /// The directory.
    pub path: String,
    /// `ro`.
    pub read_only: bool,
    /// `rw=`: the hosts that may write, `:` between; empty for none.
    pub rw_hosts: String,
    /// `access=`: the only hosts that may mount; empty for every host.
    pub access_hosts: String,
    /// `root=`: the hosts whose UID 0 is `QSECOFR`.
    pub root_hosts: String,
    /// `anon=` as given; empty for the default (`QNFSANON`'s UID).
    pub anon: String,
    /// The anonymous UID in effect; `None` when unknown users are refused.
    pub anon_uid: Option<u32>,
    /// That UID's profile, `*NONE` when none.
    pub anon_user: String,
    /// `nosuid`.
    pub nosuid: bool,
    /// `vers=`: `3:4`, `3` or `4`.
    pub versions: String,
    /// `public`.
    pub public: bool,
    /// `root`: the root of the `NFSv4` pseudo file system.
    pub pseudo_root: bool,
    /// `nfsroot=`; empty for `/`.
    pub nfsroot: String,
    /// `exname=`; empty for the path.
    pub exname: String,
    /// The host options.
    pub hosts: Vec<HostOption>,
    /// The job that exported it, qualified.
    pub by: String,
    /// When, seconds since 1970.
    pub since: u64,
}

/// One daemon job.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Daemon {
    /// Its type, the `SERVER` value (`*SVR`).
    pub server: String,
    /// The job, qualified.
    pub job: String,
    /// It reported ready.
    pub ready: bool,
    /// It was asked to end.
    pub ending: bool,
    /// What it registered with the binder: `2049/tcp` ...
    pub ports: Vec<String>,
    /// When it started, seconds since 1970.
    pub since: u64,
}

/// One mounted file system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mount {
    /// Its ID.
    pub id: u32,
    /// Its type as `/etc/vfs` names it (`root`, `udfs`, `nfs3` ...).
    pub kind: String,
    /// The directory mounted over.
    pub at: String,
    /// What is mounted (`host:/path`, a UDFS); empty for none.
    pub source: String,
    /// Its options.
    pub options: String,
    /// Read only.
    pub read_only: bool,
    /// A remote file system's server answers; `None` for a local one.
    pub responding: Option<bool>,
}

impl Mount {
    /// Whether `UNMOUNT` takes it (an NFS mount or a UDFS).
    #[must_use]
    pub fn unmountable(&self) -> bool {
        self.kind.starts_with("nfs") || self.kind == "udfs"
    }
}

/// What the NFS pages need. `user` is the signed-on user.
pub trait Nfs: Send + Sync {
    /// The export table, in the order it was exported.
    fn exports(&self, user: &str) -> Result<Vec<Export>, Message>;
    /// The daemon jobs.
    fn daemons(&self, user: &str) -> Vec<Daemon>;
    /// The mount table; `Err` when it cannot be read.
    fn mounts(&self, user: &str) -> Result<Vec<Mount>, Message>;
    /// Run CL `command` as `user`: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}

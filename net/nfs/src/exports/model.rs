// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An export entry: the directory, its options and its host options.

use std::net::IpAddr;

/// One host of a host list: a name (a host or a netgroup of
/// `/etc/netgroup`), an address, or a network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostSpec {
    /// A host name or a netgroup name; which one is decided when a
    /// client is matched (a netgroup of that name wins).
    Name(String),
    /// An IPv4 or IPv6 address.
    Addr(IpAddr),
    /// `@network/length` or `@network/mask`: every address of it.
    Net {
        /// The network's address.
        addr: IpAddr,
        /// The prefix length.
        len: u8,
    },
}

impl std::fmt::Display for HostSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(n) => f.write_str(n),
            Self::Addr(a) => write!(f, "{a}"),
            Self::Net { addr, len } => write!(f, "@{addr}/{len}"),
        }
    }
}

/// An RPC security flavor an export takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Flavor {
    /// `AUTH_SYS` (`sec=sys`), flavor 1.
    Sys,
}

impl Flavor {
    /// The flavor's number on the wire.
    #[must_use]
    pub fn number(self) -> u32 {
        match self {
            Self::Sys => 1,
        }
    }
}

/// The NFS versions an export is offered in (`vers=`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Versions {
    /// Version 3.
    pub v3: bool,
    /// Version 4 (4.0 and 4.1).
    pub v4: bool,
}

impl Default for Versions {
    fn default() -> Self {
        Self { v3: true, v4: true }
    }
}

/// The CCSID file data is converted to for a host (`HOSTOPT`'s second
/// element).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataCcsid {
    /// `*BINARY`: no conversion.
    Binary,
    /// `*ASCII`: the ASCII CCSID of the job that exported.
    Ascii,
    /// `*JOBCCSID`: the CCSID of the job that exported.
    Job,
    /// A CCSID.
    Ccsid(u16),
}

/// The CCSID of a host's path names (`HOSTOPT`'s third element).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathCcsid {
    /// `*ASCII`.
    Ascii,
    /// `*JOBCCSID`.
    Job,
    /// A CCSID.
    Ccsid(u16),
}

/// Options for some hosts of an export (`HOSTOPT`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostOpt {
    /// The host, address, network or netgroup.
    pub host: HostSpec,
    /// The CCSID file data is converted to.
    pub data: DataCcsid,
    /// The CCSID of path names.
    pub path: PathCcsid,
    /// Writes are on disk before the answer (`*SYNC`).
    pub sync: bool,
}

impl HostOpt {
    /// A host with the defaults: no conversion, ASCII names, synchronous.
    #[must_use]
    pub fn new(host: HostSpec) -> Self {
        Self {
            host,
            data: DataCcsid::Binary,
            path: PathCcsid::Ascii,
            sync: true,
        }
    }
}

/// The options of an export (`EXPORTFS -O`, `/etc/exports`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the options are flags
pub struct ExportOptions {
    /// `ro`: read only for every host but those of `rw=`.
    pub ro: bool,
    /// `rw=host:...`: the hosts that may write; `Some(vec![])` is `rw`
    /// alone (every host).
    pub rw: Option<Vec<HostSpec>>,
    /// `access=host:...`: the only hosts that may mount; `None` is every
    /// host.
    pub access: Option<Vec<HostSpec>>,
    /// `root=host:...`: the hosts whose UID 0 is `QSECOFR`.
    pub root_hosts: Vec<HostSpec>,
    /// `anon=uid`: the UID unknown users get; `-1` refuses them; `None`
    /// is the UID of `QNFSANON`.
    pub anon: Option<i64>,
    /// `nosuid`: set-user-ID and set-group-ID bits are cleared.
    pub nosuid: bool,
    /// `sec=`: the flavors; empty is the default, `sys`.
    pub sec: Vec<Flavor>,
    /// `vers=`: `None` is the default, 3 and 4.
    pub vers: Option<Versions>,
    /// `public`: the export is the `NFSv4` public file handle's.
    pub public: bool,
    /// `root` alone: the export is the root of the `NFSv4` pseudo file
    /// system.
    pub pseudo_root: bool,
    /// `nfsroot=path`: the directory the pseudo file system starts at.
    pub nfsroot: Option<String>,
    /// `exname=path`: the name `NFSv4` clients see the export by.
    pub exname: Option<String>,
}

impl ExportOptions {
    /// The flavors in effect.
    #[must_use]
    pub fn flavors(&self) -> Vec<Flavor> {
        if self.sec.is_empty() {
            vec![Flavor::Sys]
        } else {
            self.sec.clone()
        }
    }

    /// The versions in effect.
    #[must_use]
    pub fn versions(&self) -> Versions {
        self.vers.unwrap_or_default()
    }
}

/// An export entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    /// The directory, an absolute path.
    pub path: String,
    /// Its options.
    pub options: ExportOptions,
    /// Its host options.
    pub hosts: Vec<HostOpt>,
}

impl Export {
    /// An export of `path` with the default options.
    #[must_use]
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            options: ExportOptions::default(),
            hosts: Vec::new(),
        }
    }
}

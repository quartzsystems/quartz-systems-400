// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the server needs of an export (docs/NFS.md section 5):
//! [`ExportView`] answers for one export and a peer address,
//! [`ExportTable`] finds exports by ID and by path. The export table of
//! `EXPORTFS` implements them; [`MemExport`] and [`MemExports`] are an
//! in-memory version with hosts as addresses and networks.

use std::fmt::Debug;
use std::net::IpAddr;
use std::sync::{Arc, RwLock};

use ifs::ops::audit::FileId;

/// What the export does for a caller UID it cannot map, and for
/// `AUTH_NONE` and squashed root: `ANON=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anon {
    /// The system's anonymous profile (`QNFSANON`), the default.
    Default,
    /// `ANON=uid`: the profile with that UID.
    Uid(u32),
    /// `ANON=-1`: refused.
    Refuse,
}

/// A host's options (`HOSTOPT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostOpts {
    /// The data file CCSID: `None` is `*BINARY` (data as stored).
    pub data_ccsid: Option<u16>,
    /// The CCSID path names are in on the wire.
    pub path_ccsid: u16,
    /// Writes are made synchronous whatever the client asks (`*SYNC`).
    pub sync: bool,
}

impl Default for HostOpts {
    fn default() -> Self {
        Self {
            data_ccsid: None,
            path_ccsid: ccsid::conv::UTF8,
            sync: false,
        }
    }
}

/// One export as the server sees it.
pub trait ExportView: Send + Sync + Debug {
    /// The export's ID (in every handle).
    fn id(&self) -> u32;
    /// The exported directory's IFS path.
    fn path(&self) -> &str;
    /// The exported directory's file ID.
    fn root_id(&self) -> FileId;
    /// Whether `peer` may use the export at all (`ACCESS=`).
    fn allows(&self, peer: IpAddr) -> bool;
    /// Whether `peer` may change it (not `RO`, and in `RW=` if given).
    fn writable(&self, peer: IpAddr) -> bool;
    /// Whether UID 0 from `peer` is `QSECOFR` (`ROOT=`).
    fn root_access(&self, peer: IpAddr) -> bool;
    /// `ANON=`.
    fn anon(&self) -> Anon;
    /// `NOSUID`: set-user-ID and set-group-ID bits are cleared.
    fn nosuid(&self) -> bool;
    /// Whether NFS version `vers` may use it (`VERS=`).
    fn version_allowed(&self, vers: u32) -> bool;
    /// The RPC flavors it takes (`SEC=`; only `AUTH_SYS`, 1).
    fn flavors(&self) -> Vec<i32>;
    /// `HOSTOPT` for `peer`.
    fn host_opts(&self, peer: IpAddr) -> HostOpts;
    /// The host names `EXPORT` lists for it (none: everyone).
    fn groups(&self) -> Vec<String>;
    /// The name `NFSv4` clients see it by (`EXNAME=`, else its path).
    fn v4_name(&self) -> String {
        self.path().to_string()
    }
    /// Where the `NFSv4` pseudo file system starts for it (`NFSROOT=`).
    fn v4_nfsroot(&self) -> String {
        "/".to_string()
    }
    /// It is the root of the `NFSv4` pseudo file system (`ROOT`).
    fn v4_root(&self) -> bool {
        false
    }
    /// It holds the `NFSv4` public file handle (`PUBLIC`).
    fn v4_public(&self) -> bool {
        false
    }
}

/// The exports.
pub trait ExportTable: Send + Sync {
    /// The export with ID `id`.
    fn by_id(&self, id: u32) -> Option<Arc<dyn ExportView>>;
    /// Every export, in the table's order.
    fn all(&self) -> Vec<Arc<dyn ExportView>>;
    /// The export `path` lies in: the one with the longest path that
    /// holds it ([`within`]).
    fn for_path(&self, path: &str) -> Option<Arc<dyn ExportView>> {
        self.all()
            .into_iter()
            .filter(|e| within(e.path(), path).is_some())
            .max_by_key(|e| e.path().len())
    }
}

/// Whether names in `path` are matched without regard to case: every
/// file system but `/QOpenSys` and what is under it.
fn folds(path: &str) -> bool {
    let p = path.trim_start_matches('/');
    let top = p.split('/').next().unwrap_or("");
    !top.eq_ignore_ascii_case("QOpenSys")
}

/// The rest of `path` below `root` (empty for `root` itself), or `None`
/// when `path` is not `root` or under it. Compared by whole components,
/// without regard to case outside `/QOpenSys`.
#[must_use]
pub fn within<'a>(root: &str, path: &'a str) -> Option<&'a str> {
    let r = root.trim_end_matches('/');
    if r.is_empty() {
        return path.starts_with('/').then(|| path.trim_start_matches('/'));
    }
    let head = path.get(..r.len())?;
    let same = if folds(r) {
        head.eq_ignore_ascii_case(r)
    } else {
        head == r
    };
    if !same {
        return None;
    }
    let rest = &path[r.len()..];
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix('/')
    }
}

/// Hosts: addresses and networks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPat {
    /// One address.
    Addr(IpAddr),
    /// A network: an address and a prefix length.
    Net(IpAddr, u8),
}

impl HostPat {
    /// Whether `a` matches.
    #[must_use]
    pub fn matches(&self, a: IpAddr) -> bool {
        let a = canonical(a);
        match *self {
            Self::Addr(x) => canonical(x) == a,
            Self::Net(n, len) => match (canonical(n), a) {
                (IpAddr::V4(n), IpAddr::V4(a)) => {
                    let m = mask32(len);
                    u32::from(n) & m == u32::from(a) & m
                }
                (IpAddr::V6(n), IpAddr::V6(a)) => {
                    let m = mask128(len);
                    u128::from(n) & m == u128::from(a) & m
                }
                _ => false,
            },
        }
    }
}

fn canonical(a: IpAddr) -> IpAddr {
    match a {
        IpAddr::V6(v) => v.to_ipv4_mapped().map_or(a, IpAddr::V4),
        IpAddr::V4(_) => a,
    }
}

fn mask32(len: u8) -> u32 {
    if len == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(len.min(32)))
    }
}

fn mask128(len: u8) -> u128 {
    if len == 0 {
        0
    } else {
        u128::MAX << (128 - u32::from(len.min(128)))
    }
}

/// A host list; `None` is every host.
fn listed(list: Option<&[HostPat]>, a: IpAddr) -> bool {
    list.is_none_or(|l| l.iter().any(|p| p.matches(a)))
}

/// An export in memory.
#[derive(Debug, Clone)]
pub struct MemExport {
    /// The ID.
    pub id: u32,
    /// The path.
    pub path: String,
    /// The root's file ID.
    pub root: FileId,
    /// `RO`.
    pub ro: bool,
    /// `RW=`: `None` for every host allowed.
    pub rw: Option<Vec<HostPat>>,
    /// `ACCESS=`: `None` for every host.
    pub access: Option<Vec<HostPat>>,
    /// `ROOT=`.
    pub root_hosts: Vec<HostPat>,
    /// `ANON=`.
    pub anon: Anon,
    /// `NOSUID`.
    pub nosuid: bool,
    /// `VERS=`.
    pub versions: Vec<u32>,
    /// `HOSTOPT` by host, the first match winning.
    pub host_opts: Vec<(HostPat, HostOpts)>,
    /// The options of a host `host_opts` does not name.
    pub default_opts: HostOpts,
}

impl MemExport {
    /// An export of `path` (whose file ID is `root`) to everyone, read
    /// and write, versions 3 and 4.
    #[must_use]
    pub fn new(id: u32, path: &str, root: FileId) -> Self {
        Self {
            id,
            path: path.into(),
            root,
            ro: false,
            rw: None,
            access: None,
            root_hosts: Vec::new(),
            anon: Anon::Default,
            nosuid: false,
            versions: vec![3, 4],
            host_opts: Vec::new(),
            default_opts: HostOpts::default(),
        }
    }
}

impl ExportView for MemExport {
    fn id(&self) -> u32 {
        self.id
    }
    fn path(&self) -> &str {
        &self.path
    }
    fn root_id(&self) -> FileId {
        self.root
    }
    fn allows(&self, peer: IpAddr) -> bool {
        listed(self.access.as_deref(), peer)
    }
    fn writable(&self, peer: IpAddr) -> bool {
        !self.ro && listed(self.rw.as_deref(), peer)
    }
    fn root_access(&self, peer: IpAddr) -> bool {
        self.root_hosts.iter().any(|p| p.matches(peer))
    }
    fn anon(&self) -> Anon {
        self.anon
    }
    fn nosuid(&self) -> bool {
        self.nosuid
    }
    fn version_allowed(&self, vers: u32) -> bool {
        self.versions.contains(&vers)
    }
    fn flavors(&self) -> Vec<i32> {
        vec![1]
    }
    fn host_opts(&self, peer: IpAddr) -> HostOpts {
        self.host_opts
            .iter()
            .find(|(p, _)| p.matches(peer))
            .map_or(self.default_opts, |(_, o)| *o)
    }
    fn groups(&self) -> Vec<String> {
        self.access
            .iter()
            .flatten()
            .map(|p| match p {
                HostPat::Addr(a) => a.to_string(),
                HostPat::Net(a, l) => format!("@{a}/{l}"),
            })
            .collect()
    }
}

/// A table of [`MemExport`]s, changed while it is served.
#[derive(Debug, Default)]
pub struct MemExports {
    list: RwLock<Vec<Arc<MemExport>>>,
}

impl MemExports {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces (by ID) an export.
    pub fn put(&self, e: MemExport) {
        let mut l = self
            .list
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        l.retain(|x| x.id != e.id);
        l.push(Arc::new(e));
    }

    /// Removes the export with ID `id` (`EXPORTFS -U`).
    pub fn remove(&self, id: u32) {
        let mut l = self
            .list
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        l.retain(|x| x.id != id);
    }
}

impl ExportTable for MemExports {
    fn by_id(&self, id: u32) -> Option<Arc<dyn ExportView>> {
        let l = self
            .list
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        l.iter()
            .find(|e| e.id == id)
            .map(|e| Arc::clone(e) as Arc<dyn ExportView>)
    }
    fn all(&self) -> Vec<Arc<dyn ExportView>> {
        let l = self
            .list
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        l.iter()
            .map(|e| Arc::clone(e) as Arc<dyn ExportView>)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_within_an_export() {
        assert_eq!(within("/home", "/home"), Some(""));
        assert_eq!(within("/home/", "/HOME/a/b"), Some("a/b"));
        assert_eq!(within("/home", "/homer"), None);
        assert_eq!(within("/QOpenSys/x", "/QOpenSys/X"), None);
        assert_eq!(within("/QOpenSys/x", "/QOpenSys/x/y"), Some("y"));
        assert_eq!(within("/", "/a"), Some("a"));
    }

    #[test]
    fn hosts_and_networks() {
        let a: IpAddr = "10.1.2.3".parse().unwrap();
        assert!(HostPat::Net("10.1.0.0".parse().unwrap(), 16).matches(a));
        assert!(!HostPat::Net("10.2.0.0".parse().unwrap(), 16).matches(a));
        assert!(HostPat::Addr(a).matches("::ffff:10.1.2.3".parse().unwrap()));
        assert!(HostPat::Net("::".parse().unwrap(), 0).matches("::1".parse().unwrap()));
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The mounted file systems behind the one resolver, and the mount
//! table. Root, `/QOpenSys` and user-defined file systems are subtrees
//! of the one backend ([`ROOT_BASE`], [`QOPENSYS_BASE`], [`UDFS_BASE`];
//! the UDFS area is reachable by no path). The rest are views:
//! character devices ([`dev`]), the object store ([`qsys`]), optical
//! volumes ([`optical`]), and the file systems QS/400 does not have.

pub mod dev;
pub mod optical;
pub mod qsys;

use crate::path::fold;

/// The backend directory holding root.
pub const ROOT_BASE: &str = "root";
/// The backend directory holding `/QOpenSys`.
pub const QOPENSYS_BASE: &str = "QOpenSys";
/// The backend directory holding user-defined file systems, one
/// directory per ASP (`QASP01`) and one per UDFS in it (`NAME.udfs`).
pub const UDFS_BASE: &str = "UDFS";

/// A mounted file system's id (`st_vfs`, `st_dev`).
pub type FsId = u32;

/// What a mount is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsKind {
    /// `/`.
    Root,
    /// `/QOpenSys`.
    QOpenSys,
    /// A user-defined file system: its block special path.
    Udfs(String),
    /// Character special files (`/dev`, `/QOpenSys/dev`).
    Dev,
    /// `/QSYS.LIB`.
    Qsys,
    /// `/QOPT`.
    Qopt,
    /// A file system QS/400 does not provide (`/QNTC`, `/QFileSvr.400`,
    /// `/QDLS`): the directory exists, everything else is `ENOTSUP`.
    Unsupported,
}

/// One mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// Id.
    pub id: FsId,
    /// The mount point's canonical components.
    pub at: Vec<String>,
    /// What it is.
    pub kind: FsKind,
    /// The backend subtree (backend file systems only).
    pub base: String,
    /// Names compare exactly.
    pub case_sensitive: bool,
    /// Nothing may change.
    pub read_only: bool,
    /// The file system's name for displays (`QOpenSys`, `QSYS.LIB` ...).
    pub name: String,
}

impl Mount {
    /// Whether the backend holds it.
    #[must_use]
    pub fn is_backend(&self) -> bool {
        matches!(self.kind, FsKind::Root | FsKind::QOpenSys | FsKind::Udfs(_))
    }
}

/// The mounts.
#[derive(Debug, Clone)]
pub struct MountTable {
    mounts: Vec<Mount>,
    next: FsId,
}

/// Fixed ids of the standard file systems.
pub mod ids {
    use super::FsId;
    /// `/`.
    pub const ROOT: FsId = 1;
    /// `/QOpenSys`.
    pub const QOPENSYS: FsId = 2;
    /// `/dev`.
    pub const DEV: FsId = 3;
    /// `/QOpenSys/dev`.
    pub const QOPENSYS_DEV: FsId = 4;
    /// `/QSYS.LIB`.
    pub const QSYS: FsId = 5;
    /// `/QOPT`.
    pub const QOPT: FsId = 6;
    /// `/QNTC`.
    pub const QNTC: FsId = 7;
    /// `/QFileSvr.400`.
    pub const QFILESVR: FsId = 8;
    /// `/QDLS`.
    pub const QDLS: FsId = 9;
    /// The first id a `MOUNT` gets.
    pub const FIRST_UDFS: FsId = 100;
}

fn virt(
    id: FsId,
    at: &[&str],
    kind: FsKind,
    case_sensitive: bool,
    read_only: bool,
    name: &str,
) -> Mount {
    Mount {
        id,
        at: at.iter().map(|s| (*s).to_string()).collect(),
        kind,
        base: String::new(),
        case_sensitive,
        read_only,
        name: name.into(),
    }
}

impl Default for MountTable {
    fn default() -> Self {
        Self::standard()
    }
}

impl MountTable {
    /// The file systems every system has.
    #[must_use]
    pub fn standard() -> Self {
        let mut root = virt(ids::ROOT, &[], FsKind::Root, false, false, "root");
        root.base = ROOT_BASE.into();
        let mut qos = virt(
            ids::QOPENSYS,
            &["QOpenSys"],
            FsKind::QOpenSys,
            true,
            false,
            "QOpenSys",
        );
        qos.base = QOPENSYS_BASE.into();
        Self {
            mounts: vec![
                root,
                qos,
                virt(ids::DEV, &["dev"], FsKind::Dev, false, false, "dev"),
                virt(
                    ids::QOPENSYS_DEV,
                    &["QOpenSys", "dev"],
                    FsKind::Dev,
                    true,
                    false,
                    "dev",
                ),
                virt(
                    ids::QSYS,
                    &["QSYS.LIB"],
                    FsKind::Qsys,
                    false,
                    false,
                    "QSYS.LIB",
                ),
                virt(ids::QOPT, &["QOPT"], FsKind::Qopt, false, true, "QOPT"),
                virt(
                    ids::QNTC,
                    &["QNTC"],
                    FsKind::Unsupported,
                    false,
                    true,
                    "QNTC",
                ),
                virt(
                    ids::QFILESVR,
                    &["QFileSvr.400"],
                    FsKind::Unsupported,
                    false,
                    true,
                    "QFileSvr.400",
                ),
                virt(
                    ids::QDLS,
                    &["QDLS"],
                    FsKind::Unsupported,
                    false,
                    true,
                    "QDLS",
                ),
            ],
            next: ids::FIRST_UDFS,
        }
    }

    /// Every mount.
    #[must_use]
    pub fn all(&self) -> &[Mount] {
        &self.mounts
    }

    /// A mount by id.
    #[must_use]
    pub fn get(&self, id: FsId) -> Option<&Mount> {
        self.mounts.iter().find(|m| m.id == id)
    }

    /// The mount holding canonical components `comps` (the longest
    /// mount point that is a prefix).
    #[must_use]
    pub fn mount_of(&self, comps: &[String]) -> &Mount {
        self.mounts
            .iter()
            .filter(|m| comps.len() >= m.at.len() && comps[..m.at.len()] == m.at[..])
            .max_by_key(|m| m.at.len())
            .expect("root is mounted")
    }

    /// A mount whose mount point is `name` in directory `dir`; `fold`
    /// compares names case-insensitively.
    #[must_use]
    pub fn child(&self, dir: &[String], name: &str, folded: bool) -> Option<&Mount> {
        self.mounts
            .iter()
            .filter(|m| m.at.len() == dir.len() + 1 && m.at[..dir.len()] == dir[..])
            .filter(|m| {
                let last = &m.at[dir.len()];
                if folded {
                    fold::same(last, name)
                } else {
                    last == name
                }
            })
            .max_by_key(|m| m.id)
    }

    /// The mount points directly in `dir`.
    #[must_use]
    pub fn children(&self, dir: &[String]) -> Vec<&Mount> {
        let mut v: Vec<&Mount> = self
            .mounts
            .iter()
            .filter(|m| m.at.len() == dir.len() + 1 && m.at[..dir.len()] == dir[..])
            .collect();
        v.sort_by_key(|m| m.id);
        v.dedup_by(|a, b| a.at == b.at);
        v
    }

    /// Mount a backend subtree at `at`.
    pub fn add(
        &mut self,
        at: Vec<String>,
        kind: FsKind,
        base: String,
        case_sensitive: bool,
        read_only: bool,
    ) -> FsId {
        let id = self.next;
        self.next += 1;
        let name = match &kind {
            FsKind::Udfs(p) => p.clone(),
            _ => "udfs".into(),
        };
        self.mounts.push(Mount {
            id,
            at,
            kind,
            base,
            case_sensitive,
            read_only,
            name,
        });
        id
    }

    /// Take a mount out (only those [`MountTable::add`] made).
    pub fn remove(&mut self, id: FsId) -> Option<Mount> {
        if id < ids::FIRST_UDFS {
            return None;
        }
        let i = self.mounts.iter().position(|m| m.id == id)?;
        Some(self.mounts.remove(i))
    }

    /// Mounts under (or at) `comps`.
    #[must_use]
    pub fn under(&self, comps: &[String]) -> Vec<&Mount> {
        self.mounts
            .iter()
            .filter(|m| m.at.len() >= comps.len() && m.at[..comps.len()] == comps[..])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn longest_prefix() {
        let mut t = MountTable::standard();
        assert_eq!(t.mount_of(&c(&["home", "a"])).id, ids::ROOT);
        assert_eq!(t.mount_of(&c(&["QOpenSys", "usr"])).id, ids::QOPENSYS);
        assert_eq!(
            t.mount_of(&c(&["QOpenSys", "dev", "null"])).id,
            ids::QOPENSYS_DEV
        );
        assert_eq!(t.child(&[], "qopensys", true).unwrap().id, ids::QOPENSYS);
        assert!(t.child(&[], "qopensys", false).is_none());
        assert_eq!(t.children(&[]).len(), 7);
        let id = t.add(
            c(&["home", "u"]),
            FsKind::Udfs("/dev/QASP01/U.udfs".into()),
            "UDFS/QASP01/U.udfs".into(),
            false,
            false,
        );
        assert_eq!(t.mount_of(&c(&["home", "u", "x"])).id, id);
        assert!(t.remove(ids::ROOT).is_none());
        assert!(t.remove(id).is_some());
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The one resolver. A path walks component by component from root or
//! the owner's current directory, keeping the canonical path (stored
//! case, no links) as it goes: mount points are crossed by the mount
//! table, `..` pops the canonical path (so it leaves a mount and undoes
//! a followed link physically), symbolic links are spliced into the
//! remaining components (at most `SYMLOOP_MAX`, then `ELOOP`), names
//! fold per mount through the casefold [`index`], and every directory
//! passed needs `*X`.

pub mod index;

use sec::auth::bits::Auth;

use crate::auth::data::{RW, RX};
use crate::backend::{self, BStat, Backend, Kind};
use crate::errno::{Errno, Result};
use crate::lock::ObjKey;
use crate::meta::{self, Meta, Source};
use crate::ops::Ctx;
use crate::ops::flags as f;
use crate::path::{self, Comp, SYMLOOP_MAX};
use crate::vfs::dev::{DEVICES, DevKind, SYSTEM_ASP};
use crate::vfs::optical::{OptEntry, OpticalView};
use crate::vfs::qsys::{self, QsysEntry, QsysNode, QsysView};
use crate::vfs::{FsId, FsKind, Mount, MountTable, UDFS_BASE, ids};
use index::FoldIndex;

/// What a path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A node of a backend file system.
    Back {
        /// The mount.
        fs: FsId,
        /// Backend path.
        rel: String,
        /// Its attributes.
        st: BStat,
    },
    /// The root of a device directory (`/dev`, `/QOpenSys/dev`).
    DevDir {
        /// The mount.
        fs: FsId,
    },
    /// An ASP's directory of user-defined file systems (`/dev/QASP01`).
    AspDir {
        /// `QASP01` ...
        asp: String,
    },
    /// A user-defined file system's block special file.
    UdfsFile {
        /// ASP directory.
        asp: String,
        /// `NAME.udfs`.
        name: String,
        /// Its root in the backend.
        st: BStat,
    },
    /// A character special file.
    Dev {
        /// The mount.
        fs: FsId,
        /// Which.
        dev: DevKind,
    },
    /// An object or member in `/QSYS.LIB`.
    Qsys(QsysNode, QsysEntry),
    /// `/QOPT` itself.
    OptRoot,
    /// A volume, directory or file in `/QOPT`.
    Opt {
        /// Volume.
        vol: String,
        /// Path in it.
        path: Vec<String>,
        /// The entry.
        entry: OptEntry,
    },
    /// The root of a file system QS/400 does not have.
    Unsup {
        /// The mount.
        fs: FsId,
    },
}

/// What a node is, for `st_mode` and the checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// `*DIR`, `*LIB`, a `*FILE` with members.
    Dir,
    /// `*STMF`, `*MBR`, `*USRSPC`, `*SAVF`.
    File,
    /// `*SYMLNK`.
    Symlink,
    /// `*FIFO`.
    Fifo,
    /// `*SOCKET`.
    Socket,
    /// `*CHRSF`.
    Chr,
    /// `*BLKSF`.
    Blk,
    /// Another `/QSYS.LIB` object.
    Native,
}

impl NodeType {
    /// The `S_IF*` bits.
    #[must_use]
    pub fn mode_bits(self) -> u32 {
        match self {
            Self::Dir => f::S_IFDIR,
            Self::File => f::S_IFREG,
            Self::Symlink => f::S_IFLNK,
            Self::Fifo => f::S_IFIFO,
            Self::Socket => f::S_IFSOCK,
            Self::Chr => f::S_IFCHR,
            Self::Blk => f::S_IFBLK,
            Self::Native => f::S_IFNATIVE,
        }
    }
}

fn hash_ino(s: &str) -> u64 {
    // FNV-1a: stable across processes, unlike the std hasher.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h | 1 << 63
}

impl Node {
    /// The type.
    #[must_use]
    pub fn ntype(&self) -> NodeType {
        match self {
            Self::Back { st, .. } => match st.kind {
                Kind::File => NodeType::File,
                Kind::Dir => NodeType::Dir,
                Kind::Symlink => NodeType::Symlink,
                Kind::Fifo => NodeType::Fifo,
                Kind::Socket => NodeType::Socket,
                Kind::CharDev => NodeType::Chr,
            },
            Self::DevDir { .. } | Self::AspDir { .. } | Self::OptRoot | Self::Unsup { .. } => {
                NodeType::Dir
            }
            Self::UdfsFile { .. } => NodeType::Blk,
            Self::Dev { .. } => NodeType::Chr,
            Self::Qsys(n, e) => match n {
                QsysNode::Root | QsysNode::Lib(_) => NodeType::Dir,
                QsysNode::Mbr(..) => NodeType::File,
                QsysNode::Obj(_, _, t) => match t.as_str() {
                    "*FILE" if e.attr.starts_with("PF") => NodeType::Dir,
                    "*USRSPC" | "*SAVF" => NodeType::File,
                    _ => NodeType::Native,
                },
            },
            Self::Opt { entry, .. } => {
                if entry.kind == Kind::Dir {
                    NodeType::Dir
                } else {
                    NodeType::File
                }
            }
        }
    }

    /// Whether it is a directory.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.ntype() == NodeType::Dir
    }

    /// The file system.
    #[must_use]
    pub fn fs(&self) -> FsId {
        match self {
            Self::Back { fs, .. }
            | Self::DevDir { fs }
            | Self::Dev { fs, .. }
            | Self::Unsup { fs } => *fs,
            Self::AspDir { .. } | Self::UdfsFile { .. } => ids::DEV,
            Self::Qsys(..) => ids::QSYS,
            Self::OptRoot | Self::Opt { .. } => ids::QOPT,
        }
    }

    /// The file number.
    #[must_use]
    pub fn ino(&self) -> u64 {
        match self {
            Self::Back { st, .. } | Self::UdfsFile { st, .. } => st.ino,
            Self::DevDir { .. } | Self::OptRoot | Self::Unsup { .. } => 1,
            Self::AspDir { asp } => hash_ino(asp),
            Self::Dev { dev, .. } => dev.ino(),
            Self::Qsys(n, _) => {
                if *n == QsysNode::Root {
                    1
                } else {
                    hash_ino(&qsys::path_of(n))
                }
            }
            Self::Opt { vol, path, .. } => hash_ino(&format!("{vol}/{}", path.join("/"))),
        }
    }

    /// The lock key.
    #[must_use]
    pub fn key(&self) -> ObjKey {
        ObjKey {
            fs: self.fs(),
            ino: self.ino(),
        }
    }

    /// The IBM object type (`*STMF`, `*DIR` ...).
    #[must_use]
    pub fn objtype(&self) -> String {
        match self {
            Self::Qsys(n, e) => match n {
                QsysNode::Root => "*DIR".into(),
                QsysNode::Lib(_) => "*LIB".into(),
                QsysNode::Mbr(..) => "*MBR".into(),
                QsysNode::Obj(..) => e.objtype.clone(),
            },
            _ => match self.ntype() {
                NodeType::Dir | NodeType::Native => "*DIR",
                NodeType::File => "*STMF",
                NodeType::Symlink => "*SYMLNK",
                NodeType::Fifo => "*FIFO",
                NodeType::Socket => "*SOCKET",
                NodeType::Chr => "*CHRSF",
                NodeType::Blk => "*BLKSF",
            }
            .into(),
        }
    }

    /// The backend path, for backend nodes.
    #[must_use]
    pub fn rel(&self) -> Option<&str> {
        match self {
            Self::Back { rel, .. } => Some(rel),
            _ => None,
        }
    }
}

/// A resolved path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Res {
    /// Canonical components.
    pub comps: Vec<String>,
    /// The node.
    pub node: Node,
}

impl Res {
    /// The canonical path.
    #[must_use]
    pub fn path(&self) -> String {
        path::to_text(&self.comps)
    }
}

/// What resolution reaches: the backend, the views, the mounts and the
/// casefold index.
pub struct Sys<'a, B: Backend> {
    /// The backend.
    pub b: &'a B,
    /// `/QSYS.LIB`.
    pub qsys: &'a dyn QsysView,
    /// `/QOPT`.
    pub opt: &'a dyn OpticalView,
    /// The mounts.
    pub mounts: &'a MountTable,
    /// The index.
    pub index: &'a mut FoldIndex,
}

/// The record of a virtual node: owned by `QSYS` with `pub_auth` for
/// everyone.
fn virtual_meta(owner: &str, pub_auth: Auth) -> Meta {
    let mut m = Meta::new(owner, 65535, 0);
    m.public = pub_auth;
    m
}

impl<B: Backend> Sys<'_, B> {
    /// The node at canonical components, without checks or folding.
    pub fn locate(&mut self, comps: &[String]) -> Result<Node> {
        let m = self.mounts.mount_of(comps).clone();
        let rest = &comps[m.at.len()..];
        self.in_mount(&m, rest)
    }

    fn in_mount(&mut self, m: &Mount, rest: &[String]) -> Result<Node> {
        match &m.kind {
            FsKind::Root | FsKind::QOpenSys | FsKind::Udfs(_) => {
                let mut rel = m.base.clone();
                for c in rest {
                    rel = backend::join(&rel, c);
                }
                let st = self.b.stat(&rel)?;
                Ok(Node::Back { fs: m.id, rel, st })
            }
            FsKind::Dev => match rest {
                [] => Ok(Node::DevDir { fs: m.id }),
                [name] => {
                    if let Some(dev) = DevKind::by_name(name) {
                        Ok(Node::Dev { fs: m.id, dev })
                    } else if m.id == ids::DEV && self.asps()?.contains(name) {
                        Ok(Node::AspDir { asp: name.clone() })
                    } else {
                        Err(Errno::ENOENT)
                    }
                }
                [asp, name] if m.id == ids::DEV => {
                    let rel = backend::join(&backend::join(UDFS_BASE, asp), name);
                    let st = self.b.stat(&rel)?;
                    Ok(Node::UdfsFile {
                        asp: asp.clone(),
                        name: name.clone(),
                        st,
                    })
                }
                _ => Err(Errno::ENOENT),
            },
            FsKind::Qsys => {
                let n = qsys::node_of(rest)?;
                let e = self.qsys_entry(&n)?;
                Ok(Node::Qsys(n, e))
            }
            FsKind::Qopt => match rest {
                [] => Ok(Node::OptRoot),
                [vol, path @ ..] => {
                    // Volume names fold like the rest of /QOPT.
                    let vol = &self
                        .opt
                        .volumes()
                        .into_iter()
                        .find(|v| v.eq_ignore_ascii_case(vol))
                        .ok_or(Errno::ENOENT)?;
                    let entry = if path.is_empty() {
                        OptEntry {
                            name: vol.clone(),
                            kind: Kind::Dir,
                            size: 0,
                        }
                    } else {
                        self.opt.stat(vol, path)?
                    };
                    Ok(Node::Opt {
                        vol: vol.clone(),
                        path: path.to_vec(),
                        entry,
                    })
                }
            },
            FsKind::Unsupported => {
                if rest.is_empty() {
                    Ok(Node::Unsup { fs: m.id })
                } else {
                    Err(Errno::ENOTSUP)
                }
            }
        }
    }

    fn qsys_entry(&self, n: &QsysNode) -> Result<QsysEntry> {
        match n {
            QsysNode::Root => Ok(QsysEntry {
                name: "QSYS".into(),
                objtype: "*LIB".into(),
                owner: "QSYS".into(),
                ..QsysEntry::default()
            }),
            QsysNode::Lib(l) => self.qsys.object("QSYS", l, "*LIB"),
            QsysNode::Obj(l, o, t) => self.qsys.object(l, o, t),
            QsysNode::Mbr(l, file, m) => self
                .qsys
                .members(l, file)?
                .into_iter()
                .find(|e| e.name == *m)
                .ok_or(Errno::ENOENT),
        }
    }

    /// The ASP directories of `/dev`: the system ASP and any holding a
    /// user-defined file system.
    pub fn asps(&self) -> Result<Vec<String>> {
        let mut v = vec![SYSTEM_ASP.to_string()];
        if let Ok(ents) = self.b.readdir(UDFS_BASE) {
            for e in ents {
                if e.kind == Kind::Dir && !v.contains(&e.name) {
                    v.push(e.name);
                }
            }
        }
        v.sort();
        Ok(v)
    }

    /// A node's record: stored (or the fallback) for backend nodes,
    /// made up for the views (the view's answer for `/QSYS.LIB`).
    pub fn meta(&self, node: &Node, cx: Option<&Ctx<'_>>) -> Result<(Meta, Source)> {
        let stored = |m| Ok((m, Source::Stored));
        match node {
            Node::Back { rel, st, .. } => {
                let ids: &dyn meta::IdMap = match cx {
                    Some(c) => c.env,
                    None => &NoIds,
                };
                meta::read(self.b, rel, st, ids)
            }
            Node::UdfsFile { asp, name, st } => {
                let rel = backend::join(&backend::join(UDFS_BASE, asp), name);
                let ids: &dyn meta::IdMap = match cx {
                    Some(c) => c.env,
                    None => &NoIds,
                };
                meta::read(self.b, &rel, st, ids)
            }
            Node::Dev { .. } => stored(virtual_meta("QSYS", RW)),
            // A writable virtual volume: anyone may change it.
            Node::Opt { vol, .. } if self.opt.writable(vol) => {
                stored(virtual_meta("QSYS", Auth::ALL))
            }
            Node::DevDir { .. }
            | Node::AspDir { .. }
            | Node::OptRoot
            | Node::Opt { .. }
            | Node::Unsup { .. } => stored(virtual_meta("QSYS", RX)),
            Node::Qsys(qn, entry) => {
                let a = match (qn, cx) {
                    // Making or deleting a library is writing QSYS itself.
                    (QsysNode::Root, Some(c)) => self
                        .qsys
                        .authority("QSYS", "QSYS", "*LIB", c.principal)
                        .unwrap_or(RX),
                    (_, None) => Auth::ALL,
                    (QsysNode::Lib(lib), Some(c)) => {
                        self.qsys.authority("QSYS", lib, "*LIB", c.principal)?
                    }
                    (QsysNode::Obj(lib, obj, t), Some(c)) => {
                        self.qsys.authority(lib, obj, t, c.principal)?
                    }
                    (QsysNode::Mbr(lib, file, _), Some(c)) => {
                        self.qsys.authority(lib, file, "*FILE", c.principal)?
                    }
                };
                let owner = if entry.owner.is_empty() {
                    "QSYS"
                } else {
                    &entry.owner
                };
                let mut m = virtual_meta(owner, a);
                m.ccsid = entry.ccsid;
                if let Some(c) = cx
                    && c.principal.user == m.owner
                {
                    // The view decided for the owner too.
                    m.owner_auth = a;
                }
                stored(m)
            }
        }
    }

    /// Check `need` on a node for the caller (`None`: the system).
    pub fn check(
        &self,
        node: &Node,
        path: &str,
        cx: Option<&Ctx<'_>>,
        op: crate::auth::need::Op,
    ) -> Result<Meta> {
        let (m, _) = self.meta(node, cx)?;
        if let Some(c) = cx {
            c.require(&m, node, path, op)?;
        }
        Ok(m)
    }

    /// Resolve `p` from canonical `cwd`. `follow` follows a final
    /// symbolic link.
    pub fn resolve(
        &mut self,
        cwd: &[String],
        p: &str,
        follow: bool,
        cx: Option<&Ctx<'_>>,
    ) -> Result<Res> {
        let parsed = path::parse(p)?;
        let start = if parsed.absolute {
            Vec::new()
        } else {
            cwd.to_vec()
        };
        let r = self.walk(start, parsed.comps, follow, cx)?;
        if parsed.trailing_slash && !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        Ok(r)
    }

    /// Resolve the directory part of `p` and return it with the last
    /// name, for operations that make or remove a name.
    pub fn resolve_parent(
        &mut self,
        cwd: &[String],
        p: &str,
        cx: Option<&Ctx<'_>>,
    ) -> Result<(Res, String)> {
        let mut parsed = path::parse(p)?;
        let Some(Comp::Name(last)) = parsed.comps.pop() else {
            // `/`, `..`, `.`: nothing to name.
            return Err(if parsed.comps.is_empty() {
                Errno::EEXIST
            } else {
                Errno::EINVAL
            });
        };
        let start = if parsed.absolute {
            Vec::new()
        } else {
            cwd.to_vec()
        };
        let dir = self.walk(start, parsed.comps, true, cx)?;
        if !dir.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        Ok((dir, last))
    }

    /// Look `name` up in the directory `dir` (no checks): a mount point
    /// first, then the file system.
    pub fn child(&mut self, dir: &Res, name: &str) -> Result<Res> {
        let m = self.mounts.mount_of(&dir.comps).clone();
        if let Some(sub) = self.mounts.child(&dir.comps, name, !m.case_sensitive) {
            let comps = sub.at.clone();
            let sub = sub.clone();
            let node = self.in_mount(&sub, &[])?;
            return Ok(Res { comps, node });
        }
        let stored = match &dir.node {
            Node::Back { rel, .. } => self.index.lookup(self.b, rel, name, m.case_sensitive)?,
            Node::Qsys(..) => {
                // Names here are uppercased unless quoted: canonicalize.
                let (n, t) = qsys::parse_component(name)?;
                format!("{}.{}", qsys::show_name(&n), qsys::ext_of_type(&t))
            }
            Node::DevDir { .. } | Node::AspDir { .. } => {
                if m.case_sensitive || DevKind::by_name(name).is_some() {
                    name.to_string()
                } else {
                    let mut names: Vec<String> =
                        DEVICES.iter().map(|(n, _)| (*n).to_string()).collect();
                    names.extend(self.asps()?);
                    if let Node::AspDir { asp } = &dir.node {
                        names = self
                            .b
                            .readdir(&backend::join(UDFS_BASE, asp))?
                            .into_iter()
                            .map(|e| e.name)
                            .collect();
                    }
                    names
                        .into_iter()
                        .find(|n| crate::path::fold::same(n, name))
                        .ok_or(Errno::ENOENT)?
                }
            }
            Node::OptRoot | Node::Opt { .. } => name.to_string(),
            Node::Unsup { .. } => return Err(Errno::ENOTSUP),
            _ => return Err(Errno::ENOTDIR),
        };
        let mut comps = dir.comps.clone();
        comps.push(stored);
        let node = self.locate(&comps)?;
        Ok(Res { comps, node })
    }

    fn walk(
        &mut self,
        start: Vec<String>,
        comps: Vec<Comp>,
        follow: bool,
        cx: Option<&Ctx<'_>>,
    ) -> Result<Res> {
        let mut cur = Res {
            node: self.locate(&start)?,
            comps: start,
        };
        let mut todo: std::collections::VecDeque<Comp> = comps.into();
        let mut links = 0;
        while let Some(c) = todo.pop_front() {
            match c {
                Comp::Parent => {
                    if cur.comps.pop().is_some() {
                        cur.node = self.locate(&cur.comps)?;
                    }
                }
                Comp::Name(name) => {
                    if !cur.node.is_dir() {
                        return Err(Errno::ENOTDIR);
                    }
                    self.check(&cur.node, &cur.path(), cx, crate::auth::need::Op::Search)?;
                    let next = self.child(&cur, &name)?;
                    let last = todo.is_empty();
                    if next.node.ntype() == NodeType::Symlink && (!last || follow) {
                        links += 1;
                        if links > SYMLOOP_MAX {
                            return Err(Errno::ELOOP);
                        }
                        let rel = next.node.rel().ok_or(Errno::EINVAL)?;
                        let target = self.b.readlink(rel)?;
                        let parsed = path::parse(&target)?;
                        if parsed.absolute {
                            cur.comps.clear();
                            cur.node = self.locate(&[])?;
                        }
                        for t in parsed.comps.into_iter().rev() {
                            todo.push_front(t);
                        }
                        continue;
                    }
                    cur = next;
                }
            }
        }
        Ok(cur)
    }
}

/// No profiles: the fallback owner for every UID.
struct NoIds;

impl meta::IdMap for NoIds {
    fn profile_of_uid(&self, _uid: u32) -> Option<String> {
        None
    }
    fn profile_of_gid(&self, _gid: u32) -> Option<String> {
        None
    }
    fn uid_of(&self, _p: &str) -> Option<u32> {
        None
    }
    fn gid_of(&self, _p: &str) -> Option<u32> {
        None
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `stat()`, `lstat()`, `fstat()`, `access()`, `statvfs()` and
//! `pathconf()`, with IBM's `stat` fields.

use super::flags as f;
use super::{Ctx, Ifs, OpenKind};
use crate::auth::data::{R, W, X};
use crate::backend::{BFile, Backend, Nanos};
use crate::errno::{Errno, Result};
use crate::meta::{IdMap, Meta};
use crate::path;
use crate::resolve::{Node, NodeType};
use crate::vfs::FsId;

/// IBM's `struct stat`, field by field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stat {
    /// `st_mode`: type bits and the permission view.
    pub mode: u32,
    /// `st_ino`.
    pub ino: u64,
    /// `st_nlink`.
    pub nlink: u32,
    /// `st_uid`: the owner's UID.
    pub uid: u32,
    /// `st_gid`: the primary group's GID (0 for none).
    pub gid: u32,
    /// `st_size`.
    pub size: u64,
    /// `st_atime`.
    pub atime: Nanos,
    /// `st_mtime`.
    pub mtime: Nanos,
    /// `st_ctime`.
    pub ctime: Nanos,
    /// `st_dev`.
    pub dev: u32,
    /// `st_blksize`.
    pub blksize: u32,
    /// `st_allocsize`.
    pub allocsize: u64,
    /// `st_objtype`: `*STMF`, `*DIR` ...
    pub objtype: String,
    /// `st_codepage`.
    pub codepage: u16,
    /// `st_ccsid`.
    pub ccsid: u16,
    /// `st_rdev`.
    pub rdev: u64,
    /// `st_vfs`.
    pub vfs: u32,
    /// `st_ino_gen_id`.
    pub ino_gen: u32,
    /// The owner profile (not in `stat`; `Qp0lGetAttr` and displays).
    pub owner: String,
    /// The primary group profile.
    pub group: Option<String>,
    /// Creation time (not in `stat`).
    pub crtime: Nanos,
}

/// `struct statvfs`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatVfs {
    /// `f_bsize`.
    pub bsize: u64,
    /// `f_frsize`.
    pub frsize: u64,
    /// `f_blocks`.
    pub blocks: u64,
    /// `f_bfree`.
    pub bfree: u64,
    /// `f_bavail`.
    pub bavail: u64,
    /// `f_files`.
    pub files: u64,
    /// `f_ffree`.
    pub ffree: u64,
    /// `f_favail`.
    pub favail: u64,
    /// `f_fsid`.
    pub fsid: u32,
    /// `f_flag`: 1 read-only, 2 no set-ID.
    pub flag: u64,
    /// `f_namemax`.
    pub namemax: u64,
    /// The file system's name.
    pub name: String,
    /// Case sensitive.
    pub case_sensitive: bool,
}

/// Build the stat of a node from its record.
pub(crate) fn stat_of(node: &Node, m: &Meta, ids: &dyn IdMap) -> Stat {
    let mut s = Stat {
        mode: node.ntype().mode_bits() | crate::auth::data::mode_of(m),
        ino: node.ino(),
        nlink: 1,
        uid: ids.uid_of(&m.owner).unwrap_or(0),
        gid: m.group.as_deref().and_then(|g| ids.gid_of(g)).unwrap_or(0),
        dev: node.fs(),
        blksize: 4096,
        objtype: node.objtype(),
        codepage: m.codepage,
        ccsid: m.ccsid,
        vfs: node.fs(),
        ino_gen: m.generation,
        owner: m.owner.clone(),
        group: m.group.clone(),
        crtime: m.created,
        ..Stat::default()
    };
    if node.ntype() == NodeType::Symlink {
        s.mode = f::S_IFLNK | 0o777;
    }
    match node {
        Node::Back { st, .. } | Node::UdfsFile { st, .. } => {
            s.nlink = st.nlink;
            s.size = st.size;
            s.atime = st.atime;
            s.mtime = st.mtime;
            s.ctime = st.ctime;
            s.allocsize = st.alloc;
            s.blksize = st.blksize;
        }
        Node::Dev { dev, .. } => {
            s.rdev = dev.rdev();
            s.ccsid = 65535;
            s.codepage = 65535;
        }
        Node::Qsys(_, e) => {
            s.size = e.size;
            s.allocsize = e.size.div_ceil(4096) * 4096;
            s.mtime = e.changed;
            s.ctime = e.changed;
            s.atime = e.changed;
            s.crtime = e.created;
            s.codepage = ccsid::conv::codepage_of(e.ccsid);
        }
        Node::Opt { entry, .. } => s.size = entry.size,
        _ => {}
    }
    if node.is_dir() {
        s.nlink = s.nlink.max(2);
    }
    s
}

impl<B: Backend> Ifs<B> {
    fn stat_path(&self, cx: &Ctx<'_>, p: &str, follow: bool) -> Result<Stat> {
        let (r, m) = self.resolve_meta(cx, p, follow)?;
        Ok(stat_of(&r.node, &m, cx.env))
    }

    /// `stat()`.
    pub fn stat(&self, cx: &Ctx<'_>, p: &str) -> Result<Stat> {
        self.stat_path(cx, p, true)
    }

    /// `lstat()`: a final symbolic link itself.
    pub fn lstat(&self, cx: &Ctx<'_>, p: &str) -> Result<Stat> {
        self.stat_path(cx, p, false)
    }

    /// `fstat()`.
    pub fn fstat(&self, cx: &Ctx<'_>, fd: i32) -> Result<Stat> {
        let (path, fs, bst, len) = {
            let mut st = self.lock();
            let rest = &mut st.rest;
            let id = rest
                .owners
                .get(&cx.owner)
                .and_then(|o| o.fds.get(&fd))
                .ok_or(Errno::EBADF)?
                .open;
            let o = rest.opens.get(&id).ok_or(Errno::EBADF)?;
            let bst = match &o.kind {
                OpenKind::File(file) => Some(file.stat()?),
                _ => None,
            };
            let len = match &o.kind {
                OpenKind::Member { data, .. } | OpenKind::Stream { data, .. } => {
                    Some(data.len() as u64)
                }
                _ => None,
            };
            (o.path.clone(), o.key.fs, bst, len)
        };
        let Some(bst) = bst else {
            let mut s = self.lstat_system(cx, &path)?;
            if let Some(n) = len {
                s.size = n;
            }
            return Ok(s);
        };
        // The file may have been unlinked or renamed: trust the handle.
        let mut s = self.lstat_system(cx, &path).unwrap_or_default();
        if s.ino != bst.ino {
            s = Stat {
                mode: f::S_IFREG,
                objtype: "*STMF".into(),
                ino: bst.ino,
                vfs: fs,
                dev: fs,
                ..Stat::default()
            };
        }
        s.size = bst.size;
        s.nlink = bst.nlink;
        s.mtime = bst.mtime;
        s.atime = bst.atime;
        s.ctime = bst.ctime;
        s.allocsize = bst.alloc;
        Ok(s)
    }

    /// A stat with no authority checks (for descriptors already open).
    fn lstat_system(&self, cx: &Ctx<'_>, p: &str) -> Result<Stat> {
        let mut st = self.lock();
        let (mut sys, _) = self.split(&mut st);
        let r = sys.resolve(&[], p, false, None)?;
        let (m, _) = sys.meta(&r.node, None)?;
        Ok(stat_of(&r.node, &m, cx.env))
    }

    /// `access()`: `amode` is `F_OK` or a mix of `R_OK`, `W_OK`, `X_OK`.
    pub fn access(&self, cx: &Ctx<'_>, p: &str, amode: i32) -> Result<()> {
        if amode & !(f::R_OK | f::W_OK | f::X_OK) != 0 {
            return Err(Errno::EINVAL);
        }
        let (r, m) = self.resolve_meta(cx, p, true)?;
        let have = crate::auth::have(cx.principal, &m, cx.env);
        let mut need = sec::auth::bits::Auth::EXCLUDE;
        for (bit, a) in [(f::R_OK, R), (f::W_OK, W), (f::X_OK, X)] {
            if amode & bit != 0 {
                need = need.union(a);
            }
        }
        if !have.has(need) {
            return Err(Errno::EACCES);
        }
        if amode & f::W_OK != 0 {
            let st = self.lock();
            if st.mounts.get(r.node.fs()).is_some_and(|mt| mt.read_only) {
                return Err(Errno::EROFS);
            }
        }
        Ok(())
    }

    /// `statvfs()`.
    pub fn statvfs(&self, cx: &Ctx<'_>, p: &str) -> Result<StatVfs> {
        let (r, _) = self.resolve_meta(cx, p, true)?;
        let fs: FsId = r.node.fs();
        let (name, cs, ro, backed) = {
            let st = self.lock();
            let m = st.mounts.get(fs).ok_or(Errno::ENOENT)?;
            (
                m.name.clone(),
                m.case_sensitive,
                m.read_only,
                m.is_backend(),
            )
        };
        let mut v = StatVfs {
            fsid: fs,
            namemax: path::NAME_MAX as u64,
            name,
            case_sensitive: cs,
            flag: u64::from(ro),
            bsize: 4096,
            frsize: 4096,
            ..StatVfs::default()
        };
        if backed {
            let s = self.b.statfs()?;
            v.bsize = s.block_size;
            v.frsize = s.block_size;
            v.blocks = s.blocks;
            v.bfree = s.bfree;
            v.bavail = s.bavail;
            v.files = s.files;
            v.ffree = s.ffree;
            v.favail = s.ffree;
            v.namemax = s.name_max;
        }
        Ok(v)
    }

    /// `pathconf()`: `-1` (as `None`) where there is no limit.
    pub fn pathconf(&self, cx: &Ctx<'_>, p: &str, name: i32) -> Result<Option<i64>> {
        let (r, _) = self.resolve_meta(cx, p, true)?;
        let qsys = matches!(r.node, Node::Qsys(..));
        Ok(match name {
            f::PC_CHOWN_RESTRICTED | f::PC_NO_TRUNC | f::PC_THREAD_SAFE => Some(1),
            f::PC_LINK_MAX => Some(if qsys { 1 } else { i64::from(path::LINK_MAX) }),
            f::PC_NAME_MAX => Some(if qsys {
                16
            } else {
                i64::try_from(path::NAME_MAX).unwrap_or(255)
            }),
            f::PC_PATH_MAX => Some(i64::try_from(path::PATH_MAX).unwrap_or(i64::MAX)),
            f::PC_PIPE_BUF => Some(4096),
            f::PC_MAX_CANON | f::PC_MAX_INPUT | f::PC_VDISABLE => None,
            _ => return Err(Errno::EINVAL),
        })
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directories: `READDIR` (pseudo directories too), `READLINK`,
//! `CREATE`, `REMOVE`, `RENAME`, `LINK`. Pseudo directories are
//! read-only (`NFS4ERR_ROFS`). Cookies are the file system's (3 and
//! up; 1 and 2 are reserved, `NFS4ERR_BAD_COOKIE`); a verifier of
//! another directory is `NFS4ERR_NOT_SAME`. A change in the grace
//! period that could undo a reclaim (`REMOVE`, `RENAME`) is
//! `NFS4ERR_GRACE`.

use super::{R, vs};
use crate::server::vfs::{FileType, Special};
use crate::server::{FileHandle, Req, VfsError};
use crate::types::nfs4::{
    CREATE4args, CREATE4res, CREATE4resok, LINK4args, LINK4res, LINK4resok, READDIR4args,
    READDIR4res, READDIR4resok, READLINK4res, READLINK4resok, REMOVE4args, REMOVE4res,
    REMOVE4resok, RENAME4args, RENAME4res, RENAME4resok, change_info4, component4, createtype4,
    dirlist4, entry4, fattr4, nfs_resop4, nfsstat4, utf8string,
};
use crate::v4::Fh;
use crate::v4::attr::Bits;
use crate::v4::attr::get::{asks_write_only, rdattr_error};
use crate::v4::attr::set::decode_set;
use crate::v4::compound::{Cx, Res};
use crate::v4::pseudo::Child;
use crate::types::nfs4::FATTR4_RDATTR_ERROR;

/// Bytes of the status, verifier, list end and `eof`.
const HEAD: usize = 4 + 8 + 4 + 4;

fn padded(n: usize) -> usize {
    4 + n.div_ceil(4) * 4
}

fn cinfo(before: u64, after: u64) -> change_info4 {
    change_info4 {
        atomic: false,
        before,
        after,
    }
}

impl Cx<'_> {
    /// One entry's attributes, or `rdattr_error` when asked and they fail.
    fn entry_attrs(&mut self, fh: &Fh, want: &Bits) -> R<fattr4> {
        let r = self.attr_of(fh).and_then(|a| self.fattr(fh, &a, want));
        match r {
            Err(s) if want.has(FATTR4_RDATTR_ERROR) => Ok(rdattr_error(s)),
            r => r,
        }
    }

    /// The entries of a pseudo directory from `cookie`.
    fn pseudo_entries(&mut self, id: u64, cookie: u64) -> R<Vec<(u64, String, Fh)>> {
        let t = self.tree();
        let at = t.find(id).ok_or(nfsstat4::NFS4ERR_STALE)?;
        let kids: Vec<(String, Child)> = t.nodes[at].children.clone();
        let mut out = Vec::new();
        for (i, (name, c)) in kids.into_iter().enumerate() {
            let ck = 3 + i as u64;
            if ck <= cookie {
                continue;
            }
            match self.child_fh(&c) {
                Ok(f) => out.push((ck, name, f)),
                Err(nfsstat4::NFS4ERR_ACCESS) => {}
                Err(s) => return Err(s),
            }
        }
        Ok(out)
    }

    fn readdir(&mut self, a: &READDIR4args) -> R<READDIR4resok> {
        let dir = self.need()?;
        let at = self.attr_of(&dir)?;
        if at.ftype != FileType::Dir {
            return Err(nfsstat4::NFS4ERR_NOTDIR);
        }
        let want = Bits::from(&a.attr_request);
        if asks_write_only(&want) {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        if a.cookie == 1 || a.cookie == 2 {
            return Err(nfsstat4::NFS4ERR_BAD_COOKIE);
        }
        let max = a.maxcount as usize;
        let dirmax = if a.dircount == 0 {
            usize::MAX
        } else {
            a.dircount as usize
        };
        let (verf, list, eof): (_, Vec<(u64, Vec<u8>, Fh)>, bool) = match &dir {
            Fh::Pseudo(id) => {
                if a.cookie != 0 && a.cookieverf != [0; 8] {
                    return Err(nfsstat4::NFS4ERR_NOT_SAME);
                }
                let l = self.pseudo_entries(*id, a.cookie)?;
                (
                    [0; 8],
                    l.into_iter()
                        .map(|(c, n, f)| (c, n.into_bytes(), f))
                        .collect(),
                    true,
                )
            }
            Fh::Real(r, h) => {
                let n = (max / 24).clamp(1, 4096);
                let page = self
                    .vfs
                    .readdir(r, h, a.cookie, a.cookieverf, n, true)
                    .map_err(|e| match e {
                        VfsError::BadCookie => nfsstat4::NFS4ERR_NOT_SAME,
                        e => vs(e),
                    })?;
                let mut l = Vec::new();
                for e in page.entries {
                    let f = match e.node {
                        Some(n) => self.cross_entry(r, n.fh),
                        None => match self.vfs.lookup(r, h, &e.name) {
                            Ok((n, _)) => self.cross_entry(r, n.fh),
                            Err(_) => Fh::None,
                        },
                    };
                    l.push((e.cookie, e.name, f));
                }
                (page.verf, l, page.eof)
            }
            Fh::None => return Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
        };
        let mut size = HEAD;
        let mut info = 0usize;
        let mut entries: Vec<entry4> = Vec::new();
        let total = list.len();
        for (cookie, name, f) in list {
            let attrs = match f {
                Fh::None if want.has(FATTR4_RDATTR_ERROR) => rdattr_error(nfsstat4::NFS4ERR_STALE),
                Fh::None => return Err(nfsstat4::NFS4ERR_STALE),
                ref f => self.entry_attrs(f, &want)?,
            };
            let cost = 4 + 8 + padded(name.len()) + 4 + 4 * attrs.attrmask.0.len() + padded(attrs.attr_vals.0.len());
            let dcost = 8 + padded(name.len());
            if size + cost > max || info + dcost > dirmax {
                break;
            }
            size += cost;
            info += dcost;
            entries.push(entry4 {
                cookie,
                name: component4::from(utf8string(name)),
                attrs,
            });
        }
        if entries.is_empty() && total > 0 {
            return Err(nfsstat4::NFS4ERR_TOOSMALL);
        }
        let eof = eof && entries.len() == total;
        if size > max {
            return Err(nfsstat4::NFS4ERR_TOOSMALL);
        }
        Ok(READDIR4resok {
            cookieverf: verf,
            reply: dirlist4 { entries, eof },
        })
    }

    /// A directory entry: another export's root when it is one.
    fn cross_entry(&self, r: &Req, h: FileHandle) -> Fh {
        let peer = self.ctx.peer.ip();
        for e in self.srv.exports.all() {
            if e.id() != r.export.id()
                && e.root_id() == h.fid
                && e.allows(peer)
                && e.version_allowed(4)
                && let Ok(f) = self.export_root(&e)
            {
                return f;
            }
        }
        Fh::Real(r.clone(), h)
    }

    pub(super) fn op_readdir(&mut self, a: &READDIR4args) -> Res {
        match self.readdir(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_READDIR(READDIR4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_readlink(&mut self) -> Res {
        let r = (|| {
            let f = self.need()?;
            let at = self.attr_of(&f)?;
            match (at.ftype, &f) {
                (FileType::Lnk, Fh::Real(r, h)) => self.vfs.readlink(r, h).map(|x| x.0).map_err(vs),
                (FileType::Dir, _) => Err(nfsstat4::NFS4ERR_ISDIR),
                _ if self.minor >= 1 => Err(nfsstat4::NFS4ERR_WRONG_TYPE),
                _ => Err(nfsstat4::NFS4ERR_INVAL),
            }
        })();
        match r {
            Ok(t) => Res::Ok(nfs_resop4::OP_READLINK(READLINK4res::NFS4_OK(
                READLINK4resok {
                    link: utf8string(t),
                },
            ))),
            Err(s) => Res::Err(s),
        }
    }

    /// The current directory, an export's, writable.
    fn wdir(&self) -> R<(Req, FileHandle)> {
        let (r, h) = self.real(nfsstat4::NFS4ERR_ROFS)?;
        let at = self.vfs.getattr(&r, &h).map_err(vs)?;
        if at.ftype != FileType::Dir {
            return Err(nfsstat4::NFS4ERR_NOTDIR);
        }
        Ok((r, h))
    }

    fn create(&mut self, a: &CREATE4args) -> R<CREATE4resok> {
        let (r, h) = self.wdir()?;
        self.check_name(Some(&r), &a.objname.0)?;
        let (vals, bits) = decode_set(&a.createattrs, self.minor)?;
        Self::writable(&r)?;
        let s = self.to_setattr(&vals)?;
        let name = &a.objname.0;
        let before = self.change_of(&r, &h);
        let made = match &a.objtype {
            createtype4::NF4LNK(t) => {
                if t.0.is_empty() {
                    return Err(nfsstat4::NFS4ERR_INVAL);
                }
                self.vfs.symlink(&r, &h, name, &t.0, &s)
            }
            createtype4::NF4DIR => self.vfs.mkdir(&r, &h, name, &s),
            createtype4::NF4FIFO => self.vfs.mknod(&r, &h, name, Special::Fifo, &s),
            createtype4::NF4SOCK => self.vfs.mknod(&r, &h, name, Special::Sock, &s),
            createtype4::NF4CHR(d) => {
                self.vfs
                    .mknod(&r, &h, name, Special::Chr(d.specdata1, d.specdata2), &s)
            }
            createtype4::NF4BLK(d) => {
                self.vfs
                    .mknod(&r, &h, name, Special::Blk(d.specdata1, d.specdata2), &s)
            }
            createtype4::default(_) => return Err(nfsstat4::NFS4ERR_BADTYPE),
        }
        .map_err(|e| match e {
            VfsError::NotSupp => nfsstat4::NFS4ERR_BADTYPE,
            e => vs(e),
        })?;
        let node = made.node.ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
        let after = self.change_of(&r, &h);
        self.set_cur(Fh::Real(r, node.fh));
        Ok(CREATE4resok {
            cinfo: cinfo(before, after),
            attrset: bits.to_bitmap(),
        })
    }

    pub(super) fn op_create(&mut self, a: &CREATE4args) -> Res {
        match self.create(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_CREATE(CREATE4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    fn remove(&mut self, a: &REMOVE4args) -> R<change_info4> {
        let (r, h) = self.wdir()?;
        self.check_name(Some(&r), &a.target.0)?;
        Self::writable(&r)?;
        if self.srv.t().in_grace(self.now) {
            return Err(nfsstat4::NFS4ERR_GRACE);
        }
        let (n, _) = self.vfs.lookup(&r, &h, &a.target.0).map_err(vs)?;
        let me = self.client;
        self.deleg_conflict(n.fh.fid, me)?;
        let before = self.change_of(&r, &h);
        if n.attr.ftype == FileType::Dir {
            self.vfs.rmdir(&r, &h, &a.target.0)
        } else {
            self.vfs.remove(&r, &h, &a.target.0)
        }
        .map_err(vs)?;
        Ok(cinfo(before, self.change_of(&r, &h)))
    }

    pub(super) fn op_remove(&mut self, a: &REMOVE4args) -> Res {
        match self.remove(a) {
            Ok(c) => Res::Ok(nfs_resop4::OP_REMOVE(REMOVE4res::NFS4_OK(REMOVE4resok {
                cinfo: c,
            }))),
            Err(s) => Res::Err(s),
        }
    }

    /// The saved filehandle, an export's object.
    fn saved_real(&self) -> R<(Req, FileHandle)> {
        match &self.saved {
            Fh::None => Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
            Fh::Pseudo(_) => Err(nfsstat4::NFS4ERR_ROFS),
            Fh::Real(r, h) => Ok((r.clone(), *h)),
        }
    }

    fn rename(&mut self, a: &RENAME4args) -> R<RENAME4resok> {
        let (sr, sh) = self.saved_real()?;
        let (tr, th) = self.wdir()?;
        if sr.export.id() != tr.export.id() {
            return Err(nfsstat4::NFS4ERR_XDEV);
        }
        let sat = self.vfs.getattr(&sr, &sh).map_err(vs)?;
        if sat.ftype != FileType::Dir {
            return Err(nfsstat4::NFS4ERR_NOTDIR);
        }
        self.check_name(Some(&sr), &a.oldname.0)?;
        self.check_name(Some(&tr), &a.newname.0)?;
        Self::writable(&tr)?;
        if self.srv.t().in_grace(self.now) {
            return Err(nfsstat4::NFS4ERR_GRACE);
        }
        let (n, _) = self.vfs.lookup(&sr, &sh, &a.oldname.0).map_err(vs)?;
        let me = self.client;
        self.deleg_conflict(n.fh.fid, me)?;
        if let Ok((t, _)) = self.vfs.lookup(&tr, &th, &a.newname.0) {
            self.deleg_conflict(t.fh.fid, me)?;
        }
        let sb = self.change_of(&sr, &sh);
        let tb = self.change_of(&tr, &th);
        self.vfs
            .rename(&sr, &sh, &a.oldname.0, &th, &a.newname.0)
            .map_err(vs)?;
        Ok(RENAME4resok {
            source_cinfo: cinfo(sb, self.change_of(&sr, &sh)),
            target_cinfo: cinfo(tb, self.change_of(&tr, &th)),
        })
    }

    pub(super) fn op_rename(&mut self, a: &RENAME4args) -> Res {
        match self.rename(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_RENAME(RENAME4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    fn link(&mut self, a: &LINK4args) -> R<change_info4> {
        let (sr, sh) = self.saved_real()?;
        let (tr, th) = self.wdir()?;
        if sr.export.id() != tr.export.id() {
            return Err(nfsstat4::NFS4ERR_XDEV);
        }
        let sat = self.vfs.getattr(&sr, &sh).map_err(vs)?;
        if sat.ftype == FileType::Dir {
            return Err(nfsstat4::NFS4ERR_ISDIR);
        }
        self.check_name(Some(&tr), &a.newname.0)?;
        Self::writable(&tr)?;
        let before = self.change_of(&tr, &th);
        self.vfs.link(&tr, &sh, &th, &a.newname.0).map_err(vs)?;
        Ok(cinfo(before, self.change_of(&tr, &th)))
    }

    pub(super) fn op_link(&mut self, a: &LINK4args) -> Res {
        match self.link(a) {
            Ok(c) => Res::Ok(nfs_resop4::OP_LINK(LINK4res::NFS4_OK(LINK4resok {
                cinfo: c,
            }))),
            Err(s) => Res::Err(s),
        }
    }
}

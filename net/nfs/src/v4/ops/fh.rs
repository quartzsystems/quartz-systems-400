// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Filehandles and names: `PUTFH`, `PUTROOTFH`, `PUTPUBFH`, `GETFH`,
//! `SAVEFH`, `RESTOREFH`, `LOOKUP`, `LOOKUPP`, `SECINFO`,
//! `SECINFO_NO_NAME`. A `LOOKUP` that lands on another export's root
//! crosses into it (its own `fsid`); `LOOKUPP` from an export's root
//! goes back to the pseudo directory it is in, or to the directory of
//! the export around it.

use super::{R, vs};
use crate::server::text;
use crate::server::vfs::FileType;
use crate::v4::Fh;
use crate::v4::compound::{Cx, Res};
use crate::v4::pseudo::{self, Root};
use crate::types::nfs4::{
    GETFH4res, GETFH4resok, LOOKUP4args, LOOKUP4res, LOOKUPP4res, PUTFH4args, PUTFH4res,
    PUTPUBFH4res, PUTROOTFH4res, RESTOREFH4res, SAVEFH4res, SECINFO4args, SECINFO4res,
    SECINFO4resok, nfs_fh4, nfs_resop4, nfsstat4, secinfo_style4, secinfo4,
};

fn auth_sys() -> SECINFO4resok {
    SECINFO4resok(vec![secinfo4::default(crate::types::nfs4::AUTH_SYS)])
}

impl Cx<'_> {
    /// The filehandle of handle bytes.
    pub(super) fn fh_from(&mut self, b: &[u8]) -> R<Fh> {
        if let Some(id) = pseudo::parse(b) {
            return match self.tree().find(id) {
                Some(_) => Ok(Fh::Pseudo(id)),
                None => Err(nfsstat4::NFS4ERR_STALE),
            };
        }
        let (r, h) = self.gate().open(self.ctx, b, 4).map_err(vs)?;
        Ok(Fh::Real(r, h))
    }

    pub(super) fn op_putfh(&mut self, a: &PUTFH4args) -> Res {
        match self.fh_from(&a.object.0) {
            Ok(f) => {
                self.set_cur(f);
                Res::Ok(nfs_resop4::OP_PUTFH(PUTFH4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    /// `PUTROOTFH`'s filehandle.
    fn root_fh(&mut self) -> R<Fh> {
        match self.tree().root.clone() {
            Root::Pseudo => Ok(Fh::Pseudo(pseudo::node_id("/"))),
            Root::Export(e) => self.export_root(&e),
        }
    }

    pub(super) fn op_putrootfh(&mut self) -> Res {
        match self.root_fh() {
            Ok(f) => {
                self.set_cur(f);
                Res::Ok(nfs_resop4::OP_PUTROOTFH(PUTROOTFH4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_putpubfh(&mut self) -> Res {
        let f = match self.tree().public.clone() {
            Some(e) => self.export_root(&e),
            None => self.root_fh(),
        };
        match f {
            Ok(f) => {
                self.set_cur(f);
                Res::Ok(nfs_resop4::OP_PUTPUBFH(PUTPUBFH4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_getfh(&mut self) -> Res {
        match self.need() {
            Ok(f) => Res::Ok(nfs_resop4::OP_GETFH(GETFH4res::NFS4_OK(GETFH4resok {
                object: nfs_fh4(Self::fh_bytes(&f)),
            }))),
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_savefh(&mut self) -> Res {
        match self.need() {
            Ok(f) => {
                self.saved = f;
                self.saved_sid.clone_from(&self.cur_sid);
                Res::Ok(nfs_resop4::OP_SAVEFH(SAVEFH4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_restorefh(&mut self) -> Res {
        if matches!(self.saved, Fh::None) {
            return Res::Err(nfsstat4::NFS4ERR_RESTOREFH);
        }
        self.cur = self.saved.clone();
        self.cur_sid.clone_from(&self.saved_sid);
        Res::Ok(nfs_resop4::OP_RESTOREFH(RESTOREFH4res {
            status: nfsstat4::NFS4_OK,
        }))
    }

    /// `NFS4ERR_NOTDIR`, or `NFS4ERR_SYMLINK` for a link.
    fn not_dir(&self, f: &Fh) -> nfsstat4 {
        match self.attr_of(f) {
            Ok(a) if a.ftype == FileType::Lnk => nfsstat4::NFS4ERR_SYMLINK,
            Err(s) => s,
            Ok(_) => nfsstat4::NFS4ERR_NOTDIR,
        }
    }

    /// The object `name` in the current directory.
    pub(super) fn lookup_in(&mut self, dir: &Fh, name: &[u8]) -> R<Fh> {
        match dir {
            Fh::None => Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
            Fh::Pseudo(id) => {
                self.check_name(None, name)?;
                let n = std::str::from_utf8(name).map_err(|_| nfsstat4::NFS4ERR_NOENT)?;
                let t = self.tree();
                let at = t.find(*id).ok_or(nfsstat4::NFS4ERR_STALE)?;
                let c = t.lookup(at, n).cloned().ok_or(nfsstat4::NFS4ERR_NOENT)?;
                self.child_fh(&c)
            }
            Fh::Real(r, h) => {
                self.check_name(Some(r), name)?;
                match self.vfs.lookup(r, h, name) {
                    Ok((n, _)) => Ok(self.cross(r, n.fh)),
                    Err(crate::server::VfsError::NotDir) => Err(self.not_dir(dir)),
                    Err(e) => Err(vs(e)),
                }
            }
        }
    }

    /// An object found in export `r`: another export's root when it is one.
    fn cross(&self, r: &crate::server::Req, h: crate::server::FileHandle) -> Fh {
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

    pub(super) fn op_lookup(&mut self, a: &LOOKUP4args) -> Res {
        let dir = match self.need() {
            Ok(d) => d,
            Err(s) => return Res::Err(s),
        };
        match self.attr_of(&dir) {
            Ok(at) if at.ftype != FileType::Dir => return Res::Err(self.not_dir(&dir)),
            Err(s) => return Res::Err(s),
            Ok(_) => {}
        }
        match self.lookup_in(&dir, &a.objname.0) {
            Ok(f) => {
                self.set_cur(f);
                Res::Ok(nfs_resop4::OP_LOOKUP(LOOKUP4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    /// The parent of `f`.
    fn parent(&mut self, f: &Fh) -> R<Fh> {
        match f {
            Fh::None => Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
            Fh::Pseudo(id) => {
                let t = self.tree();
                let at = t.find(*id).ok_or(nfsstat4::NFS4ERR_STALE)?;
                match t.nodes[at].parent {
                    Some(p) => Ok(Fh::Pseudo(t.nodes[p].id)),
                    None => Err(nfsstat4::NFS4ERR_NOENT),
                }
            }
            Fh::Real(r, h) if h.fid == r.export.root_id() => {
                let id = r.export.id();
                if matches!(&self.tree().root, Root::Export(e) if e.id() == id) {
                    return Err(nfsstat4::NFS4ERR_NOENT);
                }
                if let Some(p) = self.tree().parent_of(id) {
                    return Ok(Fh::Pseudo(self.tree().nodes[p].id));
                }
                // Inside another export: that export's directory.
                let path = r.export.path().to_string();
                let up = crate::server::ifsvfs::parent(&path);
                let outer = self
                    .srv
                    .exports
                    .all()
                    .into_iter()
                    .filter(|e| e.id() != id)
                    .filter(|e| crate::server::export::within(e.path(), up).is_some())
                    .max_by_key(|e| e.path().len())
                    .ok_or(nfsstat4::NFS4ERR_NOENT)?;
                let ro = self
                    .gate()
                    .req(self.ctx, outer, 4)
                    .map_err(vs)?;
                let bytes = text::to_client(ro.opts.path_ccsid, up).map_err(vs)?;
                let n = self.vfs.lookup_path(&ro, &bytes).map_err(vs)?;
                Ok(Fh::Real(ro, n.fh))
            }
            Fh::Real(r, h) => {
                let (n, _) = self.vfs.lookup(r, h, b"..").map_err(vs)?;
                Ok(Fh::Real(r.clone(), n.fh))
            }
        }
    }

    pub(super) fn op_lookupp(&mut self) -> Res {
        let f = match self.need() {
            Ok(f) => f,
            Err(s) => return Res::Err(s),
        };
        match self.attr_of(&f) {
            Ok(at) if at.ftype != FileType::Dir => return Res::Err(self.not_dir(&f)),
            Err(s) => return Res::Err(s),
            Ok(_) => {}
        }
        match self.parent(&f) {
            Ok(p) => {
                self.set_cur(p);
                Res::Ok(nfs_resop4::OP_LOOKUPP(LOOKUPP4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_secinfo(&mut self, a: &SECINFO4args) -> Res {
        let dir = match self.need() {
            Ok(d) => d,
            Err(s) => return Res::Err(s),
        };
        match self.attr_of(&dir) {
            Ok(at) if at.ftype != FileType::Dir => return Res::Err(nfsstat4::NFS4ERR_NOTDIR),
            Err(s) => return Res::Err(s),
            Ok(_) => {}
        }
        if let Err(s) = self.lookup_in(&dir, &a.name.0) {
            return Res::Err(s);
        }
        if self.minor >= 1 {
            self.set_cur(Fh::None);
        }
        Res::Ok(nfs_resop4::OP_SECINFO(SECINFO4res::NFS4_OK(auth_sys())))
    }

    pub(super) fn op_secinfo_no_name(&mut self, style: secinfo_style4) -> Res {
        let f = match self.need() {
            Ok(f) => f,
            Err(s) => return Res::Err(s),
        };
        if style == secinfo_style4::SECINFO_STYLE4_PARENT {
            match self.attr_of(&f) {
                Ok(at) if at.ftype != FileType::Dir => {
                    return Res::Err(nfsstat4::NFS4ERR_NOTDIR);
                }
                Err(s) => return Res::Err(s),
                Ok(_) => {}
            }
            if let Err(s) = self.parent(&f) {
                return Res::Err(s);
            }
        }
        self.set_cur(Fh::None);
        Res::Ok(nfs_resop4::OP_SECINFO_NO_NAME(SECINFO4res::NFS4_OK(
            auth_sys(),
        )))
    }
}

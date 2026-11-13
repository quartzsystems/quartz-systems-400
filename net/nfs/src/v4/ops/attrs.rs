// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GETATTR`, `SETATTR`, `VERIFY`, `NVERIFY`, `ACCESS`.

use super::{R, vs};
use crate::registry::IdError;
use crate::server::vfs::{FileType, SetAttr};
use crate::types::nfs4::{
    ACCESS4_DELETE, ACCESS4_EXECUTE, ACCESS4_EXTEND, ACCESS4_LOOKUP, ACCESS4_MODIFY, ACCESS4_READ,
    ACCESS4args, ACCESS4res, ACCESS4resok, GETATTR4args, GETATTR4res, GETATTR4resok, NVERIFY4res,
    SETATTR4args, SETATTR4res, VERIFY4res, fattr4, nfs_resop4, nfsstat4,
};
use crate::v4::Fh;
use crate::v4::attr::Bits;
use crate::v4::attr::set::{SetVals, decode_set, decode_verify};
use crate::v4::compound::{Cx, Res};

/// Every `ACCESS4_*` bit.
const ALL: u32 = ACCESS4_READ
    | ACCESS4_LOOKUP
    | ACCESS4_MODIFY
    | ACCESS4_EXTEND
    | ACCESS4_DELETE
    | ACCESS4_EXECUTE;

impl Cx<'_> {
    pub(super) fn op_getattr(&mut self, a: &GETATTR4args) -> Res {
        let f = match self.need() {
            Ok(f) => f,
            Err(s) => return Res::Err(s),
        };
        let r = self
            .attr_of(&f)
            .and_then(|at| self.fattr(&f, &at, &Bits::from(&a.attr_request)));
        match r {
            Ok(obj_attributes) => Res::Ok(nfs_resop4::OP_GETATTR(GETATTR4res::NFS4_OK(
                GETATTR4resok { obj_attributes },
            ))),
            Err(s) => Res::Err(s),
        }
    }

    /// The UID or GID a sent string names.
    pub(super) fn id_of(&self, who: &[u8], group: bool) -> R<u32> {
        let reg = &self.srv.registry;
        let r = if group { reg.gid(who) } else { reg.uid(who) };
        r.map_err(|e| match e {
            IdError::BadOwner => nfsstat4::NFS4ERR_BADOWNER,
            IdError::BadChar if self.minor == 0 => nfsstat4::NFS4ERR_BADCHAR,
            IdError::BadChar => nfsstat4::NFS4ERR_INVAL,
        })
    }

    /// Sent values as the file system's [`SetAttr`].
    pub(super) fn to_setattr(&self, v: &SetVals) -> R<SetAttr> {
        Ok(SetAttr {
            mode: v.mode.map(|m| m & 0o7777),
            uid: v.owner.as_deref().map(|o| self.id_of(o, false)).transpose()?,
            gid: v.group.as_deref().map(|g| self.id_of(g, true)).transpose()?,
            size: v.size,
            atime: v.atime,
            mtime: v.mtime,
        })
    }

    fn setattr(&mut self, a: &SETATTR4args) -> R<Bits> {
        let (r, h) = self.real(nfsstat4::NFS4ERR_ROFS)?;
        let (vals, bits) = decode_set(&a.obj_attributes, self.minor)?;
        Self::writable(&r)?;
        let s = self.to_setattr(&vals)?;
        let me = self.client_of_sid(&a.stateid);
        self.deleg_conflict(h.fid, me)?;
        if s.size.is_some() {
            let at = self.vfs.getattr(&r, &h).map_err(vs)?;
            match at.ftype {
                FileType::Dir => return Err(nfsstat4::NFS4ERR_ISDIR),
                FileType::Reg => {}
                _ => return Err(nfsstat4::NFS4ERR_INVAL),
            }
            match self.io_via(&a.stateid, &h, true)? {
                super::io::Via::Fd(fd) => {
                    self.vfs
                        .setattr_open(&r, &h, fd, &s)
                        .map_err(super::super::conv::io_status)?;
                    return Ok(bits);
                }
                super::io::Via::Cache => {}
            }
        }
        self.vfs
            .setattr(&r, &h, &s, None)
            .map_err(super::super::conv::io_status)?;
        Ok(bits)
    }

    pub(super) fn op_setattr(&mut self, a: &SETATTR4args) -> Res {
        match self.setattr(a) {
            Ok(b) => Res::Ok(nfs_resop4::OP_SETATTR(SETATTR4res {
                status: nfsstat4::NFS4_OK,
                attrsset: b.to_bitmap(),
            })),
            Err(s) => Res::Err(s),
        }
    }

    /// Whether the object's attributes are those of `f`.
    fn same(&mut self, f: &fattr4) -> R<bool> {
        let cur = self.need()?;
        let bits = decode_verify(f, self.minor)?;
        let at = self.attr_of(&cur)?;
        let ours = self.fattr(&cur, &at, &bits)?;
        Ok(Bits::from(&ours.attrmask).to_bitmap() == Bits::from(&f.attrmask).to_bitmap()
            && ours.attr_vals.0 == f.attr_vals.0)
    }

    pub(super) fn op_verify(&mut self, f: &fattr4, verify: bool) -> Res {
        match self.same(f) {
            Ok(eq) => {
                let st = match (verify, eq) {
                    (true, true) | (false, false) => nfsstat4::NFS4_OK,
                    (true, false) => nfsstat4::NFS4ERR_NOT_SAME,
                    (false, true) => nfsstat4::NFS4ERR_SAME,
                };
                if st != nfsstat4::NFS4_OK {
                    return Res::Err(st);
                }
                Res::Ok(if verify {
                    nfs_resop4::OP_VERIFY(VERIFY4res { status: st })
                } else {
                    nfs_resop4::OP_NVERIFY(NVERIFY4res { status: st })
                })
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_access(&mut self, a: &ACCESS4args) -> Res {
        let f = match self.need() {
            Ok(f) => f,
            Err(s) => return Res::Err(s),
        };
        if a.access & !ALL != 0 {
            return Res::Err(nfsstat4::NFS4ERR_INVAL);
        }
        let at = match self.attr_of(&f) {
            Ok(at) => at,
            Err(s) => return Res::Err(s),
        };
        let dir = at.ftype == FileType::Dir;
        let supported = if dir {
            a.access & !ACCESS4_EXECUTE
        } else {
            a.access & !(ACCESS4_LOOKUP | ACCESS4_DELETE)
        };
        let access = match &f {
            Fh::Pseudo(_) => supported & (ACCESS4_READ | ACCESS4_LOOKUP),
            Fh::Real(r, h) => match self.vfs.access(r, h, supported) {
                Ok((got, _)) => got & supported,
                Err(e) => return Res::Err(vs(e)),
            },
            Fh::None => return Res::Err(nfsstat4::NFS4ERR_NOFILEHANDLE),
        };
        Res::Ok(nfs_resop4::OP_ACCESS(ACCESS4res::NFS4_OK(ACCESS4resok {
            supported,
            access,
        })))
    }
}


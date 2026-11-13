// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `READ`, `WRITE`, `COMMIT`, and what decides how I/O is done: an open
//! or lock stateid reads and writes through the open's own descriptor
//! (its access checked: `NFS4ERR_OPENMODE`); the anonymous and READ
//! bypass stateids, and a delegation's, through the file system's kept
//! descriptors, which another open's share deny refuses
//! (`NFS4ERR_LOCKED`). A change by one client to a file another client
//! of this job holds a read delegation on recalls it and answers
//! `NFS4ERR_DELAY`.

use super::{R, vs};
use crate::server::FileHandle;
use crate::server::vfs::{FileType, Stable};
use crate::types::nfs4::{
    COMMIT4args, COMMIT4res, COMMIT4resok, OPEN4_SHARE_ACCESS_READ, OPEN4_SHARE_ACCESS_WRITE,
    OPEN4_SHARE_DENY_READ, OPEN4_SHARE_DENY_WRITE, READ4args, READ4res, READ4resok, WRITE4args,
    WRITE4res, WRITE4resok, nfs_resop4, nfsstat4, stable_how4, stateid4,
};
use crate::v4::compound::{Cx, Res};
use crate::v4::conv::io_status;
use crate::v4::server::start_recall;
use crate::v4::state::{Kind, is_anon, is_bypass, is_special, split_other};

/// How a read or write is done.
pub(super) enum Via {
    /// Through this open of the file system.
    Fd(u64),
    /// Through the kept descriptors.
    Cache,
}

impl Cx<'_> {
    /// The client a stateid belongs to (4.1: the session's).
    pub(super) fn client_of_sid(&self, s: &stateid4) -> Option<u64> {
        if self.minor >= 1 {
            return self.client;
        }
        if is_special(s) {
            return None;
        }
        let (_, id) = split_other(&s.other);
        self.srv.t().sids.get(&id).map(|x| x.client)
    }

    /// Recalls the read delegations clients other than `me` hold on the
    /// file of `fid`: `NFS4ERR_DELAY` while any is out. A lapsed
    /// client's goes at once.
    pub(super) fn deleg_conflict(
        &mut self,
        fid: ifs::ops::audit::FileId,
        me: Option<u64>,
    ) -> R<()> {
        let lapsed = {
            let t = self.srv.t();
            let lease = self.srv.cfg.lease;
            t.sids.values().any(|s| {
                s.fh.fid == fid
                    && matches!(s.kind, Kind::Deleg(_))
                    && Some(s.client) != me
                    && t.clients
                        .get(&s.client)
                        .is_some_and(|c| c.lapsed(self.now, lease))
            })
        };
        if lapsed {
            self.srv.purge_lapsed_on(self.vfs, fid, None);
        }
        let mut t = self.srv.t();
        let ids: Vec<u64> = t
            .sids
            .values()
            .filter(|s| s.fh.fid == fid && matches!(s.kind, Kind::Deleg(_)) && Some(s.client) != me)
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        for id in ids {
            start_recall(&mut t, id, self.now);
        }
        Err(nfsstat4::NFS4ERR_DELAY)
    }

    /// How I/O with stateid `s` on `fh` is done.
    pub(super) fn io_via(&mut self, s: &stateid4, fh: &FileHandle, write: bool) -> R<Via> {
        let s = self.subst(s)?;
        let mut t = self.srv.t();
        if is_anon(&s) || is_bypass(&s) {
            if write && is_bypass(&s) {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            if write && t.in_grace(self.now) {
                return Err(nfsstat4::NFS4ERR_GRACE);
            }
            let deny = if write {
                OPEN4_SHARE_DENY_WRITE
            } else {
                OPEN4_SHARE_DENY_READ
            };
            let denied = t.sids.values().any(|x| {
                x.fh.fid == fh.fid && matches!(&x.kind, Kind::Open(o) if o.deny & deny != 0)
            });
            if denied {
                return Err(nfsstat4::NFS4ERR_LOCKED);
            }
            return Ok(Via::Cache);
        }
        if is_special(&s) {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        let id = self.find_sid(&mut t, &s)?;
        let sid = t
            .sids
            .get(&id)
            .cloned()
            .ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
        if sid.fh.fid != fh.fid {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        let need = if write {
            OPEN4_SHARE_ACCESS_WRITE
        } else {
            OPEN4_SHARE_ACCESS_READ
        };
        let open = match &sid.kind {
            Kind::Open(o) => o.clone(),
            Kind::Lock(l) => match t.sids.get(&l.open).map(|x| &x.kind) {
                Some(Kind::Open(o)) => o.clone(),
                _ => return Err(nfsstat4::NFS4ERR_BAD_STATEID),
            },
            Kind::Deleg(_) if write => return Err(nfsstat4::NFS4ERR_OPENMODE),
            Kind::Deleg(_) => return Ok(Via::Cache),
        };
        if self.minor == 0 && !t.open_owners.get(&open.owner).is_some_and(|o| o.confirmed) {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        if open.access & need == 0 {
            return Err(nfsstat4::NFS4ERR_OPENMODE);
        }
        Ok(Via::Fd(open.fd))
    }

    /// Checks the current object is a regular file.
    pub(super) fn regular(&self, t: FileType) -> R<()> {
        match t {
            FileType::Reg => Ok(()),
            FileType::Dir => Err(nfsstat4::NFS4ERR_ISDIR),
            FileType::Lnk if self.minor >= 1 => Err(nfsstat4::NFS4ERR_SYMLINK),
            _ if self.minor >= 1 => Err(nfsstat4::NFS4ERR_WRONG_TYPE),
            _ => Err(nfsstat4::NFS4ERR_INVAL),
        }
    }

    fn read(&mut self, a: &READ4args) -> R<READ4resok> {
        let (r, h) = self.real(nfsstat4::NFS4ERR_ISDIR)?;
        let at = self.vfs.getattr(&r, &h).map_err(vs)?;
        self.regular(at.ftype)?;
        let count = a.count.min(self.srv.cfg.max_io);
        let d = match self.io_via(&a.stateid, &h, false)? {
            Via::Fd(fd) => self.vfs.read_open(&r, &h, fd, a.offset, count),
            Via::Cache => self.vfs.read(&r, &h, a.offset, count),
        }
        .map_err(io_status)?;
        Ok(READ4resok {
            eof: d.eof,
            data: d.data,
        })
    }

    pub(super) fn op_read(&mut self, a: &READ4args) -> Res {
        match self.read(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_READ(READ4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    fn write(&mut self, a: &WRITE4args) -> R<WRITE4resok> {
        let (r, h) = self.real(nfsstat4::NFS4ERR_ISDIR)?;
        let at = self.vfs.getattr(&r, &h).map_err(vs)?;
        self.regular(at.ftype)?;
        Self::writable(&r)?;
        let me = self.client_of_sid(&a.stateid);
        let via = self.io_via(&a.stateid, &h, true)?;
        self.deleg_conflict(h.fid, me)?;
        let stable = match a.stable {
            stable_how4::UNSTABLE4 => Stable::Unstable,
            stable_how4::DATA_SYNC4 => Stable::DataSync,
            stable_how4::FILE_SYNC4 => Stable::FileSync,
        };
        let n = a.data.len().min(self.srv.cfg.max_io as usize);
        let data = &a.data[..n];
        let w = match via {
            Via::Fd(fd) => self.vfs.write_open(&r, &h, fd, a.offset, data, stable),
            Via::Cache => self.vfs.write(&r, &h, a.offset, data, stable),
        }
        .map_err(io_status)?;
        Ok(WRITE4resok {
            count: w.count,
            committed: match w.committed {
                Stable::Unstable => stable_how4::UNSTABLE4,
                Stable::DataSync => stable_how4::DATA_SYNC4,
                Stable::FileSync => stable_how4::FILE_SYNC4,
            },
            writeverf: w.verf,
        })
    }

    pub(super) fn op_write(&mut self, a: &WRITE4args) -> Res {
        match self.write(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_WRITE(WRITE4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_commit(&mut self, a: &COMMIT4args) -> Res {
        let r = (|| {
            let (r, h) = self.real(nfsstat4::NFS4ERR_ISDIR)?;
            let at = self.vfs.getattr(&r, &h).map_err(vs)?;
            self.regular(at.ftype)?;
            let (verf, _) = self.vfs.commit(&r, &h, a.offset, a.count).map_err(vs)?;
            Ok(verf)
        })();
        match r {
            Ok(writeverf) => Res::Ok(nfs_resop4::OP_COMMIT(COMMIT4res::NFS4_OK(COMMIT4resok {
                writeverf,
            }))),
            Err(s) => Res::Err(s),
        }
    }
}

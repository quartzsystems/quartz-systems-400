// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LOCK`, `LOCKT`, `LOCKU`, `RELEASE_LOCKOWNER`. A lock owner is an
//! IFS sub-owner of the job (from [`crate::v4::state::SUB_BASE`], the
//! start's number in it so two jobs' never meet), its locks taken
//! through the open's own descriptor: they conflict with other clients,
//! NLM's clients and local jobs, and upgrade, downgrade and split as
//! POSIX locks do. Blocking types (`READW_LT`, `WRITEW_LT`) are answered
//! at once: `NFS4ERR_DENIED`, and the client polls.

use super::{R, Seq, SeqGo, vs};
use crate::server::vfs::{Conflict, LockKind, LockOutcome};
use crate::server::{FileHandle, Req};
use crate::types::nfs4::{
    LOCK4args, LOCK4denied, LOCK4res, LOCK4resok, LOCKT4args, LOCKT4res, LOCKU4args, LOCKU4res,
    OPEN4_SHARE_ACCESS_READ, OPEN4_SHARE_ACCESS_WRITE, RELEASE_LOCKOWNER4args,
    RELEASE_LOCKOWNER4res, lock_owner4, locker4, nfs_lock_type4, nfs_resop4, nfsstat4, stateid4,
};
use crate::v4::compound::{Cx, Res, encode};
use crate::v4::state::{
    Kind, LockOwner, LockSt, OpenSt, OwnerKey, Sid, SUB_BASE, add_range, is_special, sub_range,
};

/// A range: the file system's length (0 to the end) and the end.
fn range(off: u64, len: u64) -> R<(u64, u64)> {
    if len == 0 {
        return Err(nfsstat4::NFS4ERR_INVAL);
    }
    if len == u64::MAX {
        return Ok((0, u64::MAX));
    }
    let end = off.checked_add(len).ok_or(nfsstat4::NFS4ERR_INVAL)?;
    Ok((len, end))
}

fn kind_of(t: nfs_lock_type4) -> LockKind {
    match t {
        nfs_lock_type4::READ_LT | nfs_lock_type4::READW_LT => LockKind::Read,
        nfs_lock_type4::WRITE_LT | nfs_lock_type4::WRITEW_LT => LockKind::Write,
    }
}

impl Cx<'_> {
    /// `LOCK4denied` of a conflicting lock.
    fn denied(&self, c: &Conflict) -> LOCK4denied {
        let t = self.srv.t();
        let owner = match t.owner_of_sub(c.sub) {
            Some((client, bytes)) => lock_owner4 {
                clientid: *client,
                owner: bytes.clone(),
            },
            None => lock_owner4 {
                clientid: 0,
                owner: format!("job {:x} owner {:x}", c.owner, c.sub).into_bytes(),
            },
        };
        LOCK4denied {
            offset: c.start,
            length: if c.len == 0 { u64::MAX } else { c.len },
            locktype: match c.kind {
                LockKind::Read => nfs_lock_type4::READ_LT,
                LockKind::Write => nfs_lock_type4::WRITE_LT,
            },
            owner,
        }
    }

    /// The current regular file of an export.
    fn lock_file(&self) -> R<(Req, FileHandle)> {
        let (r, h) = self.real(nfsstat4::NFS4ERR_ISDIR)?;
        let at = self.vfs.getattr(&r, &h).map_err(vs)?;
        self.regular(at.ftype)?;
        Ok((r, h))
    }

    pub(super) fn op_lock(&mut self, a: &LOCK4args) -> Res {
        // The seqid's owner and value (4.0).
        let seq = match &a.locker {
            locker4::TRUE(n) => {
                let Ok((id, _)) = self.open_sid(&n.open_stateid) else {
                    return self.lock_res(a, None);
                };
                let key = match self.srv.t().sids.get(&id).map(|s| &s.kind) {
                    Some(Kind::Open(o)) => o.owner.clone(),
                    _ => return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID),
                };
                Some((Seq::Open(key), n.open_seqid))
            }
            locker4::FALSE(e) => {
                let s = match self.subst(&e.lock_stateid) {
                    Ok(s) => s,
                    Err(st) => return Res::Err(st),
                };
                let (_, id) = crate::v4::state::split_other(&s.other);
                let key = match self.srv.t().sids.get(&id).map(|x| &x.kind) {
                    Some(Kind::Lock(l)) => l.owner.clone(),
                    _ => return self.lock_res(a, None),
                };
                Some((Seq::Lock(key), e.lock_seqid))
            }
        };
        self.lock_res(a, seq)
    }

    fn lock_res(&mut self, a: &LOCK4args, seq: Option<(Seq, u32)>) -> Res {
        if let Some((who, seqid)) = &seq {
            let go = {
                let t = self.srv.t();
                self.seq_begin(&t, who, *seqid)
            };
            match go {
                Ok(SeqGo::Replay(r)) => return self.replay(r),
                Ok(SeqGo::New) => {}
                Err(s) => return Res::Err(s),
            }
        }
        let res = match self.lock(a) {
            Ok(st) => {
                self.cur_sid = Some(st.clone());
                Res::Ok(nfs_resop4::OP_LOCK(LOCK4res::NFS4_OK(LOCK4resok {
                    lock_stateid: st,
                })))
            }
            Err(Denied::Status(s)) => Res::Err(s),
            Err(Denied::By(d)) => Res::Body(
                nfsstat4::NFS4ERR_DENIED,
                nfs_resop4::OP_LOCK(LOCK4res::NFS4ERR_DENIED(d)),
            ),
        };
        let Some((who, seqid)) = seq else { return res };
        if self.minor >= 1 {
            return res;
        }
        let (st, bytes) = encode(12, res);
        let fh = match &self.cur {
            crate::v4::Fh::Real(_, h) => Some(*h),
            _ => None,
        };
        let mut t = self.srv.t();
        self.seq_end(&mut t, &who, seqid, st, &bytes, fh);
        // A new lock owner starts at its own seqid.
        if let (locker4::TRUE(n), Seq::Open(_)) = (&a.locker, &who)
            && st == nfsstat4::NFS4_OK
        {
            let key = (n.lock_owner.clientid, n.lock_owner.owner.clone());
            if let Some(lo) = t.lock_owners.get_mut(&key) {
                lo.seq = Some(n.lock_seqid);
            }
        }
        Res::Raw(st, bytes)
    }

    /// The open behind a locker, its lock owner and the lock stateid (none yet).
    fn locker(&mut self, a: &LOCK4args, h: &FileHandle) -> R<(u64, OpenSt, OwnerKey, Option<u64>)> {
        match &a.locker {
            locker4::TRUE(n) => {
                let (oid, _) = self.open_sid(&n.open_stateid)?;
                let mut t = self.srv.t();
                let o = match t.sids.get(&oid).map(|s| &s.kind) {
                    Some(Kind::Open(o)) => o.clone(),
                    _ => return Err(nfsstat4::NFS4ERR_BAD_STATEID),
                };
                let client = o.owner.0;
                if self.minor == 0 && n.lock_owner.clientid != client {
                    return Err(nfsstat4::NFS4ERR_INVAL);
                }
                let key = (client, n.lock_owner.owner.clone());
                let existing = t.sids.values().find_map(|s| match &s.kind {
                    Kind::Lock(l) if l.owner == key && s.fh.fid == h.fid => Some(s.id),
                    _ => None,
                });
                if existing.is_some() && self.minor == 0 {
                    return Err(nfsstat4::NFS4ERR_BAD_SEQID);
                }
                if !t.lock_owners.contains_key(&key) {
                    let sub = t.next_sub();
                    t.lock_owners.insert(
                        key.clone(),
                        LockOwner {
                            sub,
                            seq: None,
                            last: None,
                            busy: false,
                            sids: Vec::new(),
                        },
                    );
                }
                Ok((oid, o, key, existing))
            }
            locker4::FALSE(e) => {
                let s = self.subst(&e.lock_stateid)?;
                if is_special(&s) {
                    return Err(nfsstat4::NFS4ERR_BAD_STATEID);
                }
                let mut t = self.srv.t();
                let id = self.find_sid(&mut t, &s)?;
                let x = t.sids.get(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
                let Kind::Lock(l) = &x.kind else {
                    return Err(nfsstat4::NFS4ERR_BAD_STATEID);
                };
                if x.fh.fid != h.fid {
                    return Err(nfsstat4::NFS4ERR_BAD_STATEID);
                }
                let (oid, owner) = (l.open, l.owner.clone());
                let o = match t.sids.get(&oid).map(|s| &s.kind) {
                    Some(Kind::Open(o)) => o.clone(),
                    _ => return Err(nfsstat4::NFS4ERR_BAD_STATEID),
                };
                Ok((oid, o, owner, Some(id)))
            }
        }
    }

    fn lock(&mut self, a: &LOCK4args) -> Result<stateid4, Denied> {
        let (r, h) = self.lock_file()?;
        let (vlen, end) = range(a.offset, a.length)?;
        let kind = kind_of(a.locktype);
        let (oid, open, key, lsid) = self.locker(a, &h)?;
        {
            let mut t = self.srv.t();
            let grace = t.in_grace(self.now);
            let owner = t
                .clients
                .get(&key.0)
                .map(|c| c.owner.clone())
                .ok_or(nfsstat4::NFS4ERR_STALE_CLIENTID)?;
            let reclaimer = t.may_reclaim(&owner);
            let c = t
                .clients
                .get_mut(&key.0)
                .ok_or(nfsstat4::NFS4ERR_STALE_CLIENTID)?;
            if a.reclaim {
                let may = reclaimer && !c.reclaim_done;
                if !grace || !may {
                    return Err(nfsstat4::NFS4ERR_NO_GRACE.into());
                }
                if !open.reclaimed {
                    return Err(nfsstat4::NFS4ERR_RECLAIM_BAD.into());
                }
            } else if grace {
                if c.minor == 0 {
                    c.reclaim_done = true;
                }
                return Err(nfsstat4::NFS4ERR_GRACE.into());
            }
        }
        let need = match kind {
            LockKind::Read => OPEN4_SHARE_ACCESS_READ,
            LockKind::Write => OPEN4_SHARE_ACCESS_WRITE,
        };
        if open.access & need == 0 {
            return Err(nfsstat4::NFS4ERR_OPENMODE.into());
        }
        let sub = self
            .srv
            .t()
            .lock_owners
            .get(&key)
            .map(|o| o.sub)
            .ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
        let mut out = self
            .vfs
            .lock_open(&r, &h, open.fd, sub, kind, a.offset, vlen)
            .map_err(vs)?;
        if let LockOutcome::Denied(c) = out
            && self.srv.purge_lapsed_on(self.vfs, h.fid, Some(c.sub))
        {
            out = self
                .vfs
                .lock_open(&r, &h, open.fd, sub, kind, a.offset, vlen)
                .map_err(vs)?;
        }
        match out {
            LockOutcome::Granted => {}
            LockOutcome::Denied(c) => return Err(Denied::By(self.denied(&c))),
            LockOutcome::Queued(tok) => {
                let _ = self.vfs.cancel_wait(tok);
                return Err(nfsstat4::NFS4ERR_DELAY.into());
            }
        }
        let mut t = self.srv.t();
        let id = match lsid {
            Some(id) => id,
            None => {
                let id = t.next_id();
                t.sids.insert(
                    id,
                    Sid {
                        id,
                        seq: 0,
                        client: key.0,
                        fh: h,
                        kind: Kind::Lock(LockSt {
                            owner: key.clone(),
                            open: oid,
                            ranges: Vec::new(),
                        }),
                    },
                );
                if let Some(Kind::Open(o)) = t.sids.get_mut(&oid).map(|s| &mut s.kind) {
                    o.locks.push(id);
                }
                if let Some(lo) = t.lock_owners.get_mut(&key) {
                    lo.sids.push(id);
                }
                id
            }
        };
        let x = t.sids.get_mut(&id).ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
        if let Kind::Lock(l) = &mut x.kind {
            add_range(&mut l.ranges, a.offset, end);
        }
        x.seq = x.seq.wrapping_add(1);
        let seq = x.seq;
        self.srv.record_state(&mut t, key.0);
        Ok(self.srv.wire_sid(id, seq))
    }

    pub(super) fn op_lockt(&mut self, a: &LOCKT4args) -> Res {
        let r = (|| -> Result<(), Denied> {
            let (r, h) = self.lock_file()?;
            let client = self.owner_client(a.owner.clientid)?;
            if self.srv.t().in_grace(self.now) {
                return Err(nfsstat4::NFS4ERR_GRACE.into());
            }
            let (vlen, _) = range(a.offset, a.length)?;
            let key = (client, a.owner.owner.clone());
            let sub = self
                .srv
                .t()
                .lock_owners
                .get(&key)
                .map_or(SUB_BASE, |o| o.sub);
            let kind = kind_of(a.locktype);
            let mut c = self
                .vfs
                .test_lock(&r, &h, sub, kind, a.offset, vlen)
                .map_err(vs)?;
            if let Some(x) = c
                && self.srv.purge_lapsed_on(self.vfs, h.fid, Some(x.sub))
            {
                c = self
                    .vfs
                    .test_lock(&r, &h, sub, kind, a.offset, vlen)
                    .map_err(vs)?;
            }
            match c {
                None => Ok(()),
                Some(c) => Err(Denied::By(self.denied(&c))),
            }
        })();
        match r {
            Ok(()) => Res::Ok(nfs_resop4::OP_LOCKT(LOCKT4res::NFS4_OK)),
            Err(Denied::Status(s)) => Res::Err(s),
            Err(Denied::By(d)) => Res::Body(
                nfsstat4::NFS4ERR_DENIED,
                nfs_resop4::OP_LOCKT(LOCKT4res::NFS4ERR_DENIED(d)),
            ),
        }
    }

    pub(super) fn op_locku(&mut self, a: &LOCKU4args) -> Res {
        let s = match self.subst(&a.lock_stateid) {
            Ok(s) if !is_special(&s) => s,
            Ok(_) => return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID),
            Err(e) => return Res::Err(e),
        };
        let (_, id) = crate::v4::state::split_other(&s.other);
        let key = match self.srv.t().sids.get(&id).map(|x| &x.kind) {
            Some(Kind::Lock(l)) => Some(l.owner.clone()),
            _ => None,
        };
        let who = key.map(Seq::Lock);
        if let Some(w) = &who {
            let go = {
                let t = self.srv.t();
                self.seq_begin(&t, w, a.seqid)
            };
            match go {
                Ok(SeqGo::Replay(r)) => return self.replay(r),
                Ok(SeqGo::New) => {}
                Err(e) => return Res::Err(e),
            }
        }
        let res = match self.locku(a, &s) {
            Ok(st) => {
                self.cur_sid = Some(st.clone());
                Res::Ok(nfs_resop4::OP_LOCKU(LOCKU4res::NFS4_OK(st)))
            }
            Err(e) => Res::Err(e),
        };
        let Some(who) = who else { return res };
        if self.minor >= 1 {
            return res;
        }
        let (st, bytes) = encode(14, res);
        let mut t = self.srv.t();
        self.seq_end(&mut t, &who, a.seqid, st, &bytes, None);
        Res::Raw(st, bytes)
    }

    fn locku(&mut self, a: &LOCKU4args, s: &stateid4) -> R<stateid4> {
        let (r, h) = self.lock_file()?;
        let (vlen, end) = range(a.offset, a.length)?;
        let (id, fd, sub) = {
            let mut t = self.srv.t();
            let id = self.find_sid(&mut t, s)?;
            let x = t.sids.get(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
            let Kind::Lock(l) = &x.kind else {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            };
            if x.fh.fid != h.fid {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            let fd = match t.sids.get(&l.open).map(|s| &s.kind) {
                Some(Kind::Open(o)) => o.fd,
                _ => return Err(nfsstat4::NFS4ERR_BAD_STATEID),
            };
            let sub = t
                .lock_owners
                .get(&l.owner)
                .map(|o| o.sub)
                .ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
            (id, fd, sub)
        };
        self.vfs
            .unlock_open(&r, &h, fd, sub, a.offset, vlen)
            .map_err(vs)?;
        let mut t = self.srv.t();
        let x = t.sids.get_mut(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
        if let Kind::Lock(l) = &mut x.kind {
            sub_range(&mut l.ranges, a.offset, end);
        }
        x.seq = x.seq.wrapping_add(1);
        Ok(self.srv.wire_sid(id, x.seq))
    }

    pub(super) fn op_release_lockowner(&mut self, a: &RELEASE_LOCKOWNER4args) -> Res {
        if let Err(s) = self.client40(a.lock_owner.clientid) {
            return Res::Err(s);
        }
        let key = (a.lock_owner.clientid, a.lock_owner.owner.clone());
        let mut t = self.srv.t();
        if let Some(lo) = t.lock_owners.get(&key) {
            let held = lo.sids.iter().any(|s| {
                matches!(t.sids.get(s).map(|x| &x.kind), Some(Kind::Lock(l)) if !l.ranges.is_empty())
            });
            if held {
                return Res::Err(nfsstat4::NFS4ERR_LOCKS_HELD);
            }
            let sids = lo.sids.clone();
            for s in sids {
                t.sids.remove(&s);
                for o in t.sids.values_mut() {
                    if let Kind::Open(op) = &mut o.kind {
                        op.locks.retain(|x| *x != s);
                    }
                }
            }
            t.lock_owners.remove(&key);
        }
        Res::Ok(nfs_resop4::OP_RELEASE_LOCKOWNER(RELEASE_LOCKOWNER4res {
            status: nfsstat4::NFS4_OK,
        }))
    }
}

/// Why a lock was not had.
enum Denied {
    Status(nfsstat4),
    By(LOCK4denied),
}

impl From<nfsstat4> for Denied {
    fn from(s: nfsstat4) -> Self {
        Self::Status(s)
    }
}

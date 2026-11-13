// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OPEN`, `OPEN_CONFIRM`, `OPEN_DOWNGRADE`, `CLOSE`.
//!
//! An open is one open of the file system with its share reservation
//! ([`crate::server::Vfs::open`]), so share conflicts with other
//! clients, other jobs and local programs are the IFS's
//! (`NFS4ERR_SHARE_DENIED`). A second `OPEN` of one owner on one file
//! upgrades its stateid: the file system's open is made again with the
//! union of access and deny. Claims: `CLAIM_NULL`, `CLAIM_PREVIOUS` (in
//! the grace period, by a client the stable records name),
//! `CLAIM_DELEGATE_CUR`, 4.1's `CLAIM_FH` and `CLAIM_DELEG_CUR_FH`;
//! `CLAIM_DELEGATE_PREV` and `CLAIM_DELEG_PREV_FH` are
//! `NFS4ERR_NOTSUPP` (delegations do not outlive a client's restart).
//! A read delegation is offered to an open for reading alone when the
//! client's callback path works and no other open of this job writes
//! the file, as an IFS read lease.

use super::{R, Seq, SeqGo, vs};
use crate::server::vfs::{CreateHow, FileType, ShareAccess, ShareDeny};
use crate::server::{FileHandle, Req, VfsError};
use crate::types::nfs4::{
    CLOSE4args, CLOSE4res, FATTR4_TIME_ACCESS, FATTR4_TIME_MODIFY, OPEN_CONFIRM4args,
    OPEN_CONFIRM4res, OPEN_CONFIRM4resok, OPEN_DOWNGRADE4args, OPEN_DOWNGRADE4res,
    OPEN_DOWNGRADE4resok, OPEN4_RESULT_CONFIRM, OPEN4_RESULT_LOCKTYPE_POSIX,
    OPEN4_SHARE_ACCESS_WANT_ANY_DELEG, OPEN4_SHARE_ACCESS_WANT_CANCEL,
    OPEN4_SHARE_ACCESS_WANT_DELEG_MASK, OPEN4_SHARE_ACCESS_WANT_NO_DELEG,
    OPEN4_SHARE_ACCESS_WANT_PUSH_DELEG_WHEN_UNCONTENDED, OPEN4_SHARE_ACCESS_WANT_READ_DELEG,
    OPEN4_SHARE_ACCESS_WANT_SIGNAL_DELEG_WHEN_RESRC_AVAIL, OPEN4_SHARE_ACCESS_WANT_WRITE_DELEG,
    OPEN4_SHARE_ACCESS_WRITE, OPEN4args, OPEN4res, OPEN4resok, change_info4, createhow4,
    nfs_resop4, nfsace4, nfsstat4, open_claim4, open_delegation_type4, open_delegation4,
    open_none_delegation4, open_read_delegation4, openflag4, stateid4, utf8string,
    why_no_delegation4,
};
use crate::v4::Fh;
use crate::v4::attr::set::decode_set;
use crate::v4::attr::{Bits, EXCLCREAT};
use crate::v4::compound::{Cx, Res, encode};
use crate::v4::state::{
    CbPath, DelegSt, Kind, OpenOwner, OpenSt, OwnerKey, Sid, client_boot, invalid, is_special,
};

/// The want bits 4.1 allows in `share_access`.
const WANTS: u32 = OPEN4_SHARE_ACCESS_WANT_DELEG_MASK
    | OPEN4_SHARE_ACCESS_WANT_SIGNAL_DELEG_WHEN_RESRC_AVAIL
    | OPEN4_SHARE_ACCESS_WANT_PUSH_DELEG_WHEN_UNCONTENDED;

fn access_of(a: u32) -> ShareAccess {
    match a & 3 {
        1 => ShareAccess::Read,
        2 => ShareAccess::Write,
        _ => ShareAccess::Both,
    }
}

fn deny_of(d: u32) -> ShareDeny {
    match d & 3 {
        0 => ShareDeny::None,
        1 => ShareDeny::Read,
        2 => ShareDeny::Write,
        _ => ShareDeny::Both,
    }
}

/// What a claim names.
enum Claim {
    /// `name` in the current directory.
    Name(Vec<u8>),
    /// The current filehandle.
    Fh,
}

impl Cx<'_> {
    /// A 4.0 client ID checked (confirmed, of this start) and renewed.
    pub(super) fn client40(&self, id: u64) -> R<()> {
        if client_boot(id) != self.srv.boot {
            return Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        }
        let mut t = self.srv.t();
        match t.clients.get_mut(&id) {
            Some(c) if c.confirmed => {
                c.renewed = self.now;
                Ok(())
            }
            _ => Err(nfsstat4::NFS4ERR_STALE_CLIENTID),
        }
    }

    /// The client an owner's request is for: 4.1's session's, or 4.0's
    /// `clientid` checked.
    pub(super) fn owner_client(&self, clientid: u64) -> R<u64> {
        if self.minor >= 1 {
            return self.client.ok_or(nfsstat4::NFS4ERR_OP_NOT_IN_SESSION);
        }
        self.client40(clientid)?;
        Ok(clientid)
    }

    pub(super) fn op_open(&mut self, a: &OPEN4args) -> Res {
        let client = match self.owner_client(a.owner.clientid) {
            Ok(c) => c,
            Err(s) => return Res::Err(s),
        };
        let key: OwnerKey = (client, a.owner.owner.clone());
        let who = Seq::Open(key.clone());
        let go = {
            let t = self.srv.t();
            self.seq_begin(&t, &who, a.seqid)
        };
        match go {
            Ok(SeqGo::Replay(r)) => return self.replay(r),
            Ok(SeqGo::New) => {}
            Err(s) => return Res::Err(s),
        }
        let res = match self.open(a, &key) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_OPEN(OPEN4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        };
        if self.minor >= 1 {
            return res;
        }
        let (st, bytes) = encode(18, res);
        let fh = match &self.cur {
            Fh::Real(_, h) if st == nfsstat4::NFS4_OK => Some(*h),
            _ => None,
        };
        let mut t = self.srv.t();
        self.seq_end(&mut t, &who, a.seqid, st, &bytes, fh);
        Res::Raw(st, bytes)
    }

    /// Whether the client's callback path can carry a recall.
    fn cb_ok(&self, client: u64) -> bool {
        let t = self.srv.t();
        let Some(c) = t.clients.get(&client) else {
            return false;
        };
        if c.minor == 0 {
            return c.cb.is_some() && c.path == CbPath::Up;
        }
        c.sessions.iter().any(|s| {
            t.sessions.get(s).is_some_and(|s| {
                s.cb_sec.is_some() && s.back_conn.as_ref().is_some_and(|b| b.is_open())
            })
        })
    }

    #[allow(clippy::too_many_lines)]
    fn open(&mut self, a: &OPEN4args, key: &OwnerKey) -> R<OPEN4resok> {
        let client = key.0;
        let wants = a.share_access & WANTS;
        let acc = a.share_access & !WANTS;
        if (self.minor == 0 && wants != 0) || acc == 0 || acc > 3 || a.share_deny > 3 {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        let creating = matches!(a.openhow, openflag4::OPEN4_CREATE(_));
        let (grace, may_reclaim, owner_bytes) = {
            let t = self.srv.t();
            let c = t
                .clients
                .get(&client)
                .ok_or(nfsstat4::NFS4ERR_STALE_CLIENTID)?;
            (
                t.in_grace(self.now),
                // A 4.0 client's completion is only a guess: it may still reclaim.
                t.may_reclaim(&c.owner) && !(c.minor >= 1 && c.reclaim_done),
                c.owner.clone(),
            )
        };
        let _ = owner_bytes;
        let mut reclaim = false;
        let mut deleg_claim: Option<stateid4> = None;
        let claim = match &a.claim {
            open_claim4::CLAIM_NULL(n) => Claim::Name(n.0.clone()),
            open_claim4::CLAIM_FH if self.minor >= 1 => Claim::Fh,
            open_claim4::CLAIM_DELEGATE_CUR(d) => {
                deleg_claim = Some(d.delegate_stateid.clone());
                Claim::Name(d.file.0.clone())
            }
            open_claim4::CLAIM_DELEG_CUR_FH(s) if self.minor >= 1 => {
                deleg_claim = Some(s.clone());
                Claim::Fh
            }
            open_claim4::CLAIM_PREVIOUS(dt) => {
                if !grace || !may_reclaim {
                    return Err(nfsstat4::NFS4ERR_NO_GRACE);
                }
                if *dt == open_delegation_type4::OPEN_DELEGATE_WRITE {
                    return Err(nfsstat4::NFS4ERR_RECLAIM_BAD);
                }
                reclaim = true;
                Claim::Fh
            }
            open_claim4::CLAIM_DELEGATE_PREV(_) => return Err(nfsstat4::NFS4ERR_NOTSUPP),
            open_claim4::CLAIM_DELEG_PREV_FH if self.minor >= 1 => {
                return Err(nfsstat4::NFS4ERR_NOTSUPP);
            }
            _ => return Err(nfsstat4::NFS4ERR_INVAL),
        };
        if !reclaim && deleg_claim.is_none() && grace {
            // A 4.0 client asking for new state is done reclaiming.
            if let Some(c) = self.srv.t().clients.get_mut(&client)
                && c.minor == 0
            {
                c.reclaim_done = true;
            }
            return Err(nfsstat4::NFS4ERR_GRACE);
        }
        if creating && !matches!(claim, Claim::Name(_)) {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        // The file.
        let (r, h, cinfo, attrset) = match claim {
            Claim::Fh => {
                let (r, h) = self.real(nfsstat4::NFS4ERR_ISDIR)?;
                let c = self.change_of(&r, &h);
                (
                    r,
                    h,
                    change_info4 {
                        atomic: false,
                        before: c,
                        after: c,
                    },
                    Bits::default(),
                )
            }
            Claim::Name(name) => self.open_name(a, &name)?,
        };
        let at = self.vfs.getattr(&r, &h).map_err(vs)?;
        match at.ftype {
            FileType::Reg => {}
            FileType::Dir => return Err(nfsstat4::NFS4ERR_ISDIR),
            FileType::Lnk => return Err(nfsstat4::NFS4ERR_SYMLINK),
            _ if self.minor >= 1 => return Err(nfsstat4::NFS4ERR_WRONG_TYPE),
            _ => return Err(nfsstat4::NFS4ERR_INVAL),
        }
        if acc & OPEN4_SHARE_ACCESS_WRITE != 0 {
            Self::writable(&r)?;
        }
        if let Some(ds) = &deleg_claim {
            let mut t = self.srv.t();
            let id = self.find_sid(&mut t, ds)?;
            let s = t.sids.get(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
            if s.client != client || s.fh.fid != h.fid || !matches!(s.kind, Kind::Deleg(_)) {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
        }
        if acc & OPEN4_SHARE_ACCESS_WRITE != 0 {
            self.deleg_conflict(h.fid, Some(client))?;
        }
        let (sid, confirm) = self.open_state(key, &r, &h, acc, a.share_deny, reclaim)?;
        let delegation = self.delegate(&r, &h, client, acc, wants, &a.claim);
        {
            let mut t = self.srv.t();
            self.srv.record_state(&mut t, client);
        }
        let st = self.srv.wire_sid(sid.0, sid.1);
        self.set_cur(Fh::Real(r, h));
        self.cur_sid = Some(st.clone());
        let mut rflags = OPEN4_RESULT_LOCKTYPE_POSIX;
        if confirm {
            rflags |= OPEN4_RESULT_CONFIRM;
        }
        Ok(OPEN4resok {
            stateid: st,
            cinfo,
            rflags,
            attrset: attrset.to_bitmap(),
            delegation,
        })
    }

    /// `CLAIM_NULL`'s file: looked up, or made.
    fn open_name(
        &mut self,
        a: &OPEN4args,
        name: &[u8],
    ) -> R<(Req, FileHandle, change_info4, Bits)> {
        let dir = self.need()?;
        let dat = self.attr_of(&dir)?;
        if dat.ftype != FileType::Dir {
            return Err(if dat.ftype == FileType::Lnk {
                nfsstat4::NFS4ERR_SYMLINK
            } else {
                nfsstat4::NFS4ERR_NOTDIR
            });
        }
        let openflag4::OPEN4_CREATE(how) = &a.openhow else {
            let f = self.lookup_in(&dir, name)?;
            let Fh::Real(r, h) = f else {
                return Err(nfsstat4::NFS4ERR_ISDIR);
            };
            let c = self.change_of(&r, &h);
            return Ok((
                r,
                h,
                change_info4 {
                    atomic: false,
                    before: c,
                    after: c,
                },
                Bits::default(),
            ));
        };
        let Fh::Real(r, dh) = dir else {
            return Err(nfsstat4::NFS4ERR_ROFS);
        };
        self.check_name(Some(&r), name)?;
        Self::writable(&r)?;
        let times = Bits::of(&[FATTR4_TIME_ACCESS, FATTR4_TIME_MODIFY]);
        let (create, then, bits) = match how {
            createhow4::UNCHECKED4(f) | createhow4::GUARDED4(f) => {
                let (v, b) = decode_set(f, self.minor)?;
                let s = self.to_setattr(&v)?;
                let c = if matches!(how, createhow4::UNCHECKED4(_)) {
                    CreateHow::Unchecked(s)
                } else {
                    CreateHow::Guarded(s)
                };
                (c, None, b)
            }
            createhow4::EXCLUSIVE4(v) => (CreateHow::Exclusive(*v), None, times),
            createhow4::EXCLUSIVE4_1(cva) if self.minor >= 1 => {
                let (v, b) = decode_set(&cva.cva_attrs, self.minor)?;
                if !b.minus(&Bits::of(EXCLCREAT)).is_empty() {
                    return Err(nfsstat4::NFS4ERR_INVAL);
                }
                let s = self.to_setattr(&v)?;
                let mut all = b;
                all.set(FATTR4_TIME_ACCESS);
                all.set(FATTR4_TIME_MODIFY);
                (CreateHow::Exclusive(cva.cva_verf), Some(s), all)
            }
            createhow4::EXCLUSIVE4_1(_) => return Err(nfsstat4::NFS4ERR_INVAL),
        };
        let before = self.change_of(&r, &dh);
        let made = self
            .vfs
            .create(&r, &dh, name, create)
            .map_err(|e| match e {
                VfsError::IsDir => nfsstat4::NFS4ERR_ISDIR,
                e => vs(e),
            })?;
        let node = made.node.ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
        if let Some(s) = then
            && s != crate::server::vfs::SetAttr::default()
        {
            self.vfs.setattr(&r, &node.fh, &s, None).map_err(vs)?;
        }
        let after = self.change_of(&r, &dh);
        Ok((
            r,
            node.fh,
            change_info4 {
                atomic: false,
                before,
                after,
            },
            bits,
        ))
    }

    /// Opens (or upgrades the owner's open of) the file: the stateid's
    /// number and seqid, and whether the owner must confirm.
    fn open_state(
        &mut self,
        key: &OwnerKey,
        r: &Req,
        h: &FileHandle,
        acc: u32,
        deny: u32,
        reclaim: bool,
    ) -> R<((u64, u32), bool)> {
        let existing = {
            let t = self.srv.t();
            t.sids.values().find_map(|s| match &s.kind {
                Kind::Open(o) if &o.owner == key && s.fh.fid == h.fid => Some((s.id, o.clone())),
                _ => None,
            })
        };
        if let Some((id, o)) = existing {
            let (na, nd) = (o.access | acc, o.deny | deny);
            let fd = if (na, nd) == (o.access, o.deny) {
                o.fd
            } else {
                let _ = self.vfs.close(o.fd);
                match self.reopen(r, h, na, nd) {
                    Ok(fd) => fd,
                    Err(s) => {
                        let back = self.vfs.open(r, h, access_of(o.access), deny_of(o.deny));
                        let mut t = self.srv.t();
                        if let (Ok(fd), Some(x)) = (back, t.sids.get_mut(&id))
                            && let Kind::Open(op) = &mut x.kind
                        {
                            op.fd = fd;
                        }
                        return Err(s);
                    }
                }
            };
            let mut t = self.srv.t();
            let confirmed = t.open_owners.get(key).is_some_and(|ow| ow.confirmed);
            let x = t.sids.get_mut(&id).ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
            if let Kind::Open(op) = &mut x.kind {
                op.access = na;
                op.deny = nd;
                op.fd = fd;
                op.used_access |= 1 << acc;
                op.used_deny |= 1 << deny;
            }
            x.seq = x.seq.wrapping_add(1);
            return Ok(((id, x.seq), !confirmed));
        }
        let fd = self.reopen(r, h, acc, deny)?;
        let mut t = self.srv.t();
        let id = t.next_id();
        let v41 = self.minor >= 1;
        let ow = t
            .open_owners
            .entry(key.clone())
            .or_insert_with(|| OpenOwner {
                confirmed: v41,
                ..OpenOwner::default()
            });
        ow.opens.push(id);
        ow.used = Some(self.now);
        let confirm = !ow.confirmed;
        t.sids.insert(
            id,
            Sid {
                id,
                seq: 1,
                client: key.0,
                fh: *h,
                kind: Kind::Open(OpenSt {
                    owner: key.clone(),
                    access: acc,
                    deny,
                    used_access: 1 << acc,
                    used_deny: 1 << deny,
                    fd,
                    reclaimed: reclaim,
                    locks: Vec::new(),
                }),
            },
        );
        Ok(((id, 1), confirm))
    }

    /// One open of the file system; a lapsed client's open in the way goes.
    fn reopen(&mut self, r: &Req, h: &FileHandle, acc: u32, deny: u32) -> R<u64> {
        match self.vfs.open(r, h, access_of(acc), deny_of(deny)) {
            Ok(fd) => Ok(fd),
            Err(VfsError::Busy) if self.srv.purge_lapsed_on(self.vfs, h.fid, None) => self
                .vfs
                .open(r, h, access_of(acc), deny_of(deny))
                .map_err(vs),
            Err(e) => Err(vs(e)),
        }
    }

    /// The delegation to give with an open, if any.
    fn delegate(
        &mut self,
        r: &Req,
        h: &FileHandle,
        client: u64,
        acc: u32,
        wants: u32,
        claim: &open_claim4,
    ) -> open_delegation4 {
        let want = wants & OPEN4_SHARE_ACCESS_WANT_DELEG_MASK;
        let none = |why: why_no_delegation4| {
            if want == 0 {
                open_delegation4::OPEN_DELEGATE_NONE
            } else {
                open_delegation4::OPEN_DELEGATE_NONE_EXT(match why {
                    why_no_delegation4::WND4_CONTENTION => {
                        open_none_delegation4::WND4_CONTENTION(false)
                    }
                    why_no_delegation4::WND4_RESOURCE => {
                        open_none_delegation4::WND4_RESOURCE(false)
                    }
                    w => open_none_delegation4::default(w),
                })
            }
        };
        match want {
            OPEN4_SHARE_ACCESS_WANT_NO_DELEG => return none(why_no_delegation4::WND4_NOT_WANTED),
            OPEN4_SHARE_ACCESS_WANT_CANCEL => return none(why_no_delegation4::WND4_CANCELLED),
            OPEN4_SHARE_ACCESS_WANT_WRITE_DELEG => {
                return none(why_no_delegation4::WND4_RESOURCE);
            }
            0 | OPEN4_SHARE_ACCESS_WANT_READ_DELEG | OPEN4_SHARE_ACCESS_WANT_ANY_DELEG => {}
            _ => return none(why_no_delegation4::WND4_NOT_WANTED),
        }
        let asked_back = matches!(
            claim,
            open_claim4::CLAIM_PREVIOUS(open_delegation_type4::OPEN_DELEGATE_READ)
        );
        if matches!(claim, open_claim4::CLAIM_PREVIOUS(_)) && !asked_back {
            return none(why_no_delegation4::WND4_NOT_WANTED);
        }
        if acc != 1 || !self.cb_ok(client) {
            return none(why_no_delegation4::WND4_RESOURCE);
        }
        let (tag, busy) = {
            let mut t = self.srv.t();
            let busy = t.sids.values().any(|s| {
                s.fh.fid == h.fid
                    && match &s.kind {
                        Kind::Open(o) => o.access & OPEN4_SHARE_ACCESS_WRITE != 0,
                        Kind::Deleg(_) => s.client == client,
                        Kind::Lock(_) => false,
                    }
            });
            (t.next_id(), busy)
        };
        if busy {
            return none(why_no_delegation4::WND4_CONTENTION);
        }
        let mut key = [0u8; 32];
        key[..8].copy_from_slice(b"NFS4DELG");
        key[8..16].copy_from_slice(&tag.to_be_bytes());
        key[16..20].copy_from_slice(&self.srv.boot.to_be_bytes());
        let rh = ifs::lease::state::R | ifs::lease::state::H;
        match self.vfs.lease(r, h, tag, key, rh) {
            Ok(g) if g & ifs::lease::state::R != 0 => {}
            _ => return none(why_no_delegation4::WND4_CONTENTION),
        }
        let _ = self.vfs.lease_report(tag, &[(key, rh)]);
        let mut t = self.srv.t();
        t.sids.insert(
            tag,
            Sid {
                id: tag,
                seq: 1,
                client,
                fh: *h,
                kind: Kind::Deleg(DelegSt { tag, recall: None }),
            },
        );
        open_delegation4::OPEN_DELEGATE_READ(open_read_delegation4 {
            stateid: self.srv.wire_sid(tag, 1),
            recall: false,
            permissions: nfsace4 {
                r#type: 0,
                flag: 0,
                access_mask: 0,
                who: utf8string(b"EVERYONE@".to_vec()),
            },
        })
    }

    /// An open stateid of the current file, checked.
    pub(super) fn open_sid(&self, s: &stateid4) -> R<(u64, FileHandle)> {
        let s = self.subst(s)?;
        if is_special(&s) {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        let (_, h) = self.real(nfsstat4::NFS4ERR_BAD_STATEID)?;
        let mut t = self.srv.t();
        let id = self.find_sid(&mut t, &s)?;
        let x = t.sids.get(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
        if x.fh.fid != h.fid || !matches!(x.kind, Kind::Open(_)) {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        }
        Ok((id, h))
    }

    /// The owner of open stateid `id`.
    fn owner_of(&self, id: u64) -> Option<OwnerKey> {
        match self.srv.t().sids.get(&id).map(|s| &s.kind) {
            Some(Kind::Open(o)) => Some(o.owner.clone()),
            _ => None,
        }
    }

    /// Runs a seqid op of an open owner: the result kept for a
    /// retransmission (4.0).
    fn seq_op(
        &mut self,
        op: u32,
        s: &stateid4,
        seqid: u32,
        f: impl FnOnce(&mut Self, u64, FileHandle) -> Res,
    ) -> Res {
        let (id, h) = match self.open_sid(s) {
            Ok(x) => x,
            Err(nfsstat4::NFS4ERR_OLD_STATEID | nfsstat4::NFS4ERR_BAD_STATEID)
                if self.minor == 0 =>
            {
                // A retransmission whose stateid moved on: its reply.
                if let Some(r) = self.replay_of(s, seqid) {
                    return self.replay(r);
                }
                return Res::Err(
                    self.open_sid(s)
                        .err()
                        .unwrap_or(nfsstat4::NFS4ERR_BAD_STATEID),
                );
            }
            Err(e) => return Res::Err(e),
        };
        let Some(key) = self.owner_of(id) else {
            return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID);
        };
        let who = Seq::Open(key);
        let go = {
            let t = self.srv.t();
            self.seq_begin(&t, &who, seqid)
        };
        match go {
            Ok(SeqGo::Replay(r)) => return self.replay(r),
            Ok(SeqGo::New) => {}
            Err(e) => return Res::Err(e),
        }
        let res = f(self, id, h);
        if self.minor >= 1 {
            return res;
        }
        let (st, bytes) = encode(op, res);
        let mut t = self.srv.t();
        self.seq_end(&mut t, &who, seqid, st, &bytes, Some(h));
        Res::Raw(st, bytes)
    }

    /// The reply kept for `seqid` of the owner of stateid `s`.
    fn replay_of(&self, s: &stateid4, seqid: u32) -> Option<crate::v4::state::Replay> {
        let (_, id) = crate::v4::state::split_other(&s.other);
        let t = self.srv.t();
        let key = match t.sids.get(&id).map(|x| &x.kind) {
            Some(Kind::Open(o)) => o.owner.clone(),
            _ => t.closed.get(&id)?.clone(),
        };
        t.open_owners
            .get(&key)
            .and_then(|o| o.last.clone())
            .filter(|r| r.seqid == seqid)
    }

    pub(super) fn op_open_confirm(&mut self, a: &OPEN_CONFIRM4args) -> Res {
        self.seq_op(20, &a.open_stateid, a.seqid, |cx, id, _| {
            let mut t = cx.srv.t();
            let Some(key) = t.sids.get(&id).and_then(|s| match &s.kind {
                Kind::Open(o) => Some(o.owner.clone()),
                _ => None,
            }) else {
                return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID);
            };
            match t.open_owners.get_mut(&key) {
                Some(o) if !o.confirmed => o.confirmed = true,
                _ => return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID),
            }
            let Some(s) = t.sids.get_mut(&id) else {
                return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID);
            };
            s.seq = s.seq.wrapping_add(1);
            let st = cx.srv.wire_sid(id, s.seq);
            Res::Ok(nfs_resop4::OP_OPEN_CONFIRM(OPEN_CONFIRM4res::NFS4_OK(
                OPEN_CONFIRM4resok { open_stateid: st },
            )))
        })
    }

    pub(super) fn op_open_downgrade(&mut self, a: &OPEN_DOWNGRADE4args) -> Res {
        let (acc, deny) = (a.share_access, a.share_deny);
        self.seq_op(21, &a.open_stateid, a.seqid, |cx, id, h| {
            match cx.downgrade(id, &h, acc, deny) {
                Ok(st) => {
                    cx.cur_sid = Some(st.clone());
                    Res::Ok(nfs_resop4::OP_OPEN_DOWNGRADE(OPEN_DOWNGRADE4res::NFS4_OK(
                        OPEN_DOWNGRADE4resok { open_stateid: st },
                    )))
                }
                Err(s) => Res::Err(s),
            }
        })
    }

    fn downgrade(&mut self, id: u64, h: &FileHandle, acc: u32, deny: u32) -> R<stateid4> {
        let (r, _) = self.real(nfsstat4::NFS4ERR_BAD_STATEID)?;
        let Some(Kind::Open(o)) = self.srv.t().sids.get(&id).map(|s| s.kind.clone()) else {
            return Err(nfsstat4::NFS4ERR_BAD_STATEID);
        };
        if acc == 0
            || acc > 3
            || deny > 3
            || acc & !o.access != 0
            || deny & !o.deny != 0
            || o.used_access & (1 << acc) == 0
        {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        let fd = if (acc, deny) == (o.access, o.deny) {
            o.fd
        } else {
            let _ = self.vfs.close(o.fd);
            match self.vfs.open(&r, h, access_of(acc), deny_of(deny)) {
                Ok(fd) => fd,
                Err(e) => {
                    let back = self.vfs.open(&r, h, access_of(o.access), deny_of(o.deny));
                    let mut t = self.srv.t();
                    if let (Ok(fd), Some(x)) = (back, t.sids.get_mut(&id))
                        && let Kind::Open(op) = &mut x.kind
                    {
                        op.fd = fd;
                    }
                    return Err(vs(e));
                }
            }
        };
        let mut t = self.srv.t();
        let x = t.sids.get_mut(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
        if let Kind::Open(op) = &mut x.kind {
            op.access = acc;
            op.deny = deny;
            op.fd = fd;
            op.used_access = 1 << acc;
            op.used_deny = 1 << deny;
        }
        x.seq = x.seq.wrapping_add(1);
        Ok(self.srv.wire_sid(id, x.seq))
    }

    pub(super) fn op_close(&mut self, a: &CLOSE4args) -> Res {
        self.seq_op(4, &a.open_stateid, a.seqid, |cx, id, h| {
            match cx.close(id, &h) {
                Ok(st) => Res::Ok(nfs_resop4::OP_CLOSE(CLOSE4res::NFS4_OK(st))),
                Err(s) => Res::Err(s),
            }
        })
    }

    fn close(&mut self, id: u64, h: &FileHandle) -> R<stateid4> {
        let (r, _) = self.real(nfsstat4::NFS4ERR_BAD_STATEID)?;
        let (o, seq) = match self.srv.t().sids.get(&id) {
            Some(Sid {
                kind: Kind::Open(o),
                seq,
                ..
            }) => (o.clone(), *seq),
            _ => return Err(nfsstat4::NFS4ERR_BAD_STATEID),
        };
        let locks: Vec<(u64, u64)> = {
            let t = self.srv.t();
            o.locks
                .iter()
                .filter_map(|l| match t.sids.get(l).map(|s| &s.kind) {
                    Some(Kind::Lock(ls)) if !ls.ranges.is_empty() => {
                        t.lock_owners.get(&ls.owner).map(|lo| (*l, lo.sub))
                    }
                    _ => None,
                })
                .collect()
        };
        if !locks.is_empty() {
            if self.minor == 0 {
                return Err(nfsstat4::NFS4ERR_LOCKS_HELD);
            }
            for (_, sub) in &locks {
                let _ = self.vfs.unlock_open(&r, h, o.fd, *sub, 0, 0);
            }
        }
        let _ = self.vfs.close(o.fd);
        let mut t = self.srv.t();
        t.sids.remove(&id);
        if t.closed.len() > 4096 {
            t.closed.clear();
        }
        t.closed.insert(id, o.owner.clone());
        for l in &o.locks {
            t.sids.remove(l);
            for lo in t.lock_owners.values_mut() {
                lo.sids.retain(|x| x != l);
            }
        }
        if let Some(ow) = t.open_owners.get_mut(&o.owner) {
            ow.opens.retain(|x| *x != id);
            ow.used = Some(self.now);
        }
        Ok(if self.minor >= 1 {
            invalid()
        } else {
            self.srv.wire_sid(id, seq.wrapping_add(1))
        })
    }
}

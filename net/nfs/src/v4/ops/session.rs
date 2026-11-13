// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 4.1's client IDs and sessions (RFC 8881 sections 2.10 and 18.33 to
//! 18.51): `EXCHANGE_ID` (the cases of section 18.35.4, `SP4_NONE`
//! only), `CREATE_SESSION` (its own sequence and kept reply),
//! `SEQUENCE` (slots, exactly-once replies), `DESTROY_SESSION`,
//! `BIND_CONN_TO_SESSION`, `BACKCHANNEL_CTL`, `DESTROY_CLIENTID`,
//! `FREE_STATEID`, `TEST_STATEID`, `RECLAIM_COMPLETE`, `SET_SSV`
//! (`NFS4ERR_INVAL`: no SSV was ever negotiated).

use super::R;
use crate::rpc::Credential;
use crate::types::nfs4::{
    BACKCHANNEL_CTL4args, BACKCHANNEL_CTL4res, BIND_CONN_TO_SESSION4args, BIND_CONN_TO_SESSION4res,
    BIND_CONN_TO_SESSION4resok, CREATE_SESSION4_FLAG_CONN_BACK_CHAN,
    CREATE_SESSION4_FLAG_CONN_RDMA, CREATE_SESSION4_FLAG_PERSIST, CREATE_SESSION4args,
    CREATE_SESSION4res, CREATE_SESSION4resok, DESTROY_CLIENTID4args, DESTROY_CLIENTID4res,
    DESTROY_SESSION4args, DESTROY_SESSION4res, EXCHANGE_ID4args, EXCHANGE_ID4res,
    EXCHANGE_ID4resok, EXCHGID4_FLAG_BIND_PRINC_STATEID, EXCHGID4_FLAG_CONFIRMED_R,
    EXCHGID4_FLAG_MASK_PNFS, EXCHGID4_FLAG_SUPP_MOVED_MIGR, EXCHGID4_FLAG_SUPP_MOVED_REFER,
    EXCHGID4_FLAG_UPD_CONFIRMED_REC_A, EXCHGID4_FLAG_USE_NON_PNFS, FREE_STATEID4args,
    FREE_STATEID4res, RECLAIM_COMPLETE4args, RECLAIM_COMPLETE4res, SEQ4_STATUS_CB_PATH_DOWN,
    SEQ4_STATUS_CB_PATH_DOWN_SESSION, SEQUENCE4args, SEQUENCE4res, SEQUENCE4resok, SET_SSV4args,
    TEST_STATEID4args, TEST_STATEID4res, TEST_STATEID4resok, callback_sec_parms4, channel_attrs4,
    channel_dir_from_client4, channel_dir_from_server4, nfs_impl_id4, nfs_resop4, nfsstat4,
    nfstime4, server_owner4, state_protect4_a, state_protect4_r, utf8string,
};
use crate::v4::compound::{Cx, Res, encode};
use crate::v4::server::Cleanup;
use crate::v4::state::{
    Cached, CbPath, Client, Gone, Kind, Session, Slot, client_boot, client_id, is_special,
    split_other,
};

/// The largest request and reply a session takes (a 1 MiB `WRITE` or
/// `READ` and its operations).
const MAX_MSG: u32 = (1 << 20) + 16 * 1024;

/// The largest reply kept in a slot.
const MAX_CACHED: u32 = 64 * 1024;

/// The flags a client may send in `EXCHANGE_ID`.
const CLIENT_FLAGS: u32 = EXCHGID4_FLAG_SUPP_MOVED_REFER
    | EXCHGID4_FLAG_SUPP_MOVED_MIGR
    | EXCHGID4_FLAG_BIND_PRINC_STATEID
    | EXCHGID4_FLAG_MASK_PNFS
    | EXCHGID4_FLAG_UPD_CONFIRMED_REC_A;

/// A callback credential the server can send.
fn usable(p: &[callback_sec_parms4]) -> Option<Credential> {
    p.iter().find_map(|s| match s {
        callback_sec_parms4::AUTH_NONE => Some(Credential::None),
        callback_sec_parms4::AUTH_SYS(a) => {
            Some(Credential::sys(&a.machinename, a.uid, a.gid, &a.gids))
        }
        callback_sec_parms4::RPCSEC_GSS(_) => None,
    })
}

impl Cx<'_> {
    fn exchange_id(&mut self, a: &EXCHANGE_ID4args) -> R<EXCHANGE_ID4resok> {
        if a.eia_flags & !CLIENT_FLAGS != 0 {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        match a.eia_state_protect {
            state_protect4_a::SP4_NONE => {}
            // Machine credentials need RPCSEC_GSS; no SSV algorithm is offered.
            state_protect4_a::SP4_MACH_CRED(_) => return Err(nfsstat4::NFS4ERR_INVAL),
            state_protect4_a::SP4_SSV(_) => return Err(nfsstat4::NFS4ERR_ENCR_ALG_UNSUPP),
        }
        let who = self.principal();
        let owner = a.eia_clientowner.co_ownerid.clone();
        let verf = a.eia_clientowner.co_verifier;
        let lease = self.srv.cfg.lease;
        let mut t = self.srv.t();
        let conf = t
            .clients
            .values()
            .find(|c| c.minor == 1 && c.confirmed && c.owner == owner)
            .map(|c| (c.id, c.principal, c.verf, c.lapsed(self.now, lease)));
        let found = if a.eia_flags & EXCHGID4_FLAG_UPD_CONFIRMED_REC_A != 0 {
            match conf {
                None => return Err(nfsstat4::NFS4ERR_NOENT),
                Some((_, p, _, _)) if p != who => return Err(nfsstat4::NFS4ERR_PERM),
                Some((_, _, v, _)) if v != verf => return Err(nfsstat4::NFS4ERR_NOT_SAME),
                Some((id, ..)) => Some(id),
            }
        } else {
            match conf {
                Some((id, p, v, _)) if p == who && v == verf => Some(id),
                Some((id, p, _, lapsed)) if p != who && !lapsed && t.holds_state(id) => {
                    return Err(nfsstat4::NFS4ERR_CLID_INUSE);
                }
                _ => None,
            }
        };
        let id = if let Some(id) = found {
            id
        } else {
            let old: Vec<u64> = t
                .clients
                .values()
                .filter(|c| c.minor == 1 && !c.confirmed && c.owner == owner)
                .map(|c| c.id)
                .collect();
            for o in old {
                t.clients.remove(&o);
            }
            let n = t.next_id();
            let id = client_id(self.srv.boot, n);
            t.clients.insert(
                id,
                Client {
                    id,
                    minor: 1,
                    owner,
                    verf,
                    principal: who,
                    confirmed: false,
                    confirm: [0; 8],
                    cb: None,
                    path: CbPath::Unknown,
                    renewed: self.now,
                    cs_seq: 0,
                    cs_reply: None,
                    sessions: Vec::new(),
                    recorded: false,
                    reclaim_done: false,
                    revoked: 0,
                    pending_cb: None,
                },
            );
            id
        };
        let c = t
            .clients
            .get_mut(&id)
            .ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
        c.renewed = self.now;
        let mut flags = EXCHGID4_FLAG_USE_NON_PNFS;
        if c.confirmed {
            flags |= EXCHGID4_FLAG_CONFIRMED_R;
        }
        Ok(EXCHANGE_ID4resok {
            eir_clientid: id,
            eir_sequenceid: c.cs_seq.wrapping_add(1),
            eir_flags: flags,
            eir_state_protect: state_protect4_r::SP4_NONE,
            eir_server_owner: server_owner4 {
                so_minor_id: 0,
                so_major_id: self.srv.cfg.system.clone(),
            },
            eir_server_scope: self.srv.cfg.system.clone(),
            eir_server_impl_id: vec![nfs_impl_id4 {
                nii_domain: utf8string(b"quartzsystems.net".to_vec()),
                nii_name: utf8string(self.srv.cfg.impl_name.clone().into_bytes()),
                nii_date: nfstime4 {
                    seconds: 0,
                    nseconds: 0,
                },
            }],
        })
    }

    pub(super) fn op_exchange_id(&mut self, a: &EXCHANGE_ID4args) -> Res {
        match self.exchange_id(a) {
            Ok(ok) => Res::Ok(nfs_resop4::OP_EXCHANGE_ID(EXCHANGE_ID4res::NFS4_OK(ok))),
            Err(s) => Res::Err(s),
        }
    }

    /// The fore channel's attributes as granted.
    fn fore(&self, c: &channel_attrs4) -> channel_attrs4 {
        channel_attrs4 {
            ca_headerpadsize: 0,
            ca_maxrequestsize: c.ca_maxrequestsize.min(MAX_MSG),
            ca_maxresponsesize: c.ca_maxresponsesize.min(MAX_MSG),
            ca_maxresponsesize_cached: c.ca_maxresponsesize_cached.min(MAX_CACHED),
            ca_maxoperations: c.ca_maxoperations.min(self.srv.cfg.max_ops),
            ca_maxrequests: c.ca_maxrequests.clamp(1, self.srv.cfg.slots),
            ca_rdma_ird: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn create_session(&mut self, a: &CREATE_SESSION4args) -> R<Res> {
        if client_boot(a.csa_clientid) != self.srv.boot {
            return Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        }
        if a.csa_flags
            & !(CREATE_SESSION4_FLAG_PERSIST
                | CREATE_SESSION4_FLAG_CONN_BACK_CHAN
                | CREATE_SESSION4_FLAG_CONN_RDMA)
            != 0
        {
            return Err(nfsstat4::NFS4ERR_INVAL);
        }
        let who = self.principal();
        let mut t = self.srv.t();
        let c = t
            .clients
            .get(&a.csa_clientid)
            .filter(|c| c.minor == 1)
            .ok_or(nfsstat4::NFS4ERR_STALE_CLIENTID)?;
        if c.principal != who {
            return Err(nfsstat4::NFS4ERR_CLID_INUSE);
        }
        if a.csa_sequence == c.cs_seq
            && let Some(b) = &c.cs_reply
        {
            let st = if b.get(4..8) == Some(&[0, 0, 0, 0]) {
                nfsstat4::NFS4_OK
            } else {
                nfsstat4::NFS4ERR_SERVERFAULT
            };
            return Ok(Res::Raw(st, b.clone()));
        }
        if a.csa_sequence != c.cs_seq.wrapping_add(1) {
            return Err(nfsstat4::NFS4ERR_SEQ_MISORDERED);
        }
        if a.csa_fore_chan_attrs.ca_maxrequests == 0 || a.csa_fore_chan_attrs.ca_maxoperations == 0
        {
            return Err(nfsstat4::NFS4ERR_TOOSMALL);
        }
        let (confirmed, owner) = (c.confirmed, c.owner.clone());
        let mut cleanup = Cleanup::default();
        if !confirmed {
            let old: Vec<u64> = t
                .clients
                .values()
                .filter(|o| o.minor == 1 && o.confirmed && o.owner == owner)
                .map(|o| o.id)
                .collect();
            for id in old {
                let x = self.srv.drop_client(&mut t, id, Gone::Expired);
                cleanup.closes.extend(x.closes);
                cleanup.subs.extend(x.subs);
                cleanup.leases.extend(x.leases);
            }
        }
        let fore = self.fore(&a.csa_fore_chan_attrs);
        let back = channel_attrs4 {
            ca_maxrequests: 1,
            ca_rdma_ird: Vec::new(),
            ..a.csa_back_chan_attrs.clone()
        };
        let n = t.next_id();
        let mut sid = [0u8; 16];
        sid[..4].copy_from_slice(&self.srv.boot.to_be_bytes());
        sid[4..8].copy_from_slice(&a.csa_clientid.to_be_bytes()[4..]);
        sid[8..].copy_from_slice(&n.to_be_bytes());
        let back_conn = (a.csa_flags & CREATE_SESSION4_FLAG_CONN_BACK_CHAN != 0)
            .then(|| self.back.cloned())
            .flatten();
        let mut flags = 0;
        if back_conn.is_some() {
            flags |= CREATE_SESSION4_FLAG_CONN_BACK_CHAN;
        }
        let slots = vec![
            Slot {
                seq: 0,
                busy: false,
                reply: None,
                digest: 0,
            };
            fore.ca_maxrequests as usize
        ];
        t.sessions.insert(
            sid,
            Session {
                id: sid,
                client: a.csa_clientid,
                fore: fore.clone(),
                back: back.clone(),
                slots,
                cb_prog: a.csa_cb_program,
                cb_sec: usable(&a.csa_sec_parms),
                back_conn,
                conns: std::iter::once(self.ctx.peer).collect(),
                cb_seq: 0,
            },
        );
        let res = Res::Ok(nfs_resop4::OP_CREATE_SESSION(CREATE_SESSION4res::NFS4_OK(
            CREATE_SESSION4resok {
                csr_sessionid: sid,
                csr_sequence: a.csa_sequence,
                csr_flags: flags,
                csr_fore_chan_attrs: fore,
                csr_back_chan_attrs: back,
            },
        )));
        let (_, bytes) = encode(43, res);
        let c = t
            .clients
            .get_mut(&a.csa_clientid)
            .ok_or(nfsstat4::NFS4ERR_SERVERFAULT)?;
        c.confirmed = true;
        c.renewed = self.now;
        c.sessions.push(sid);
        c.cs_seq = a.csa_sequence;
        c.cs_reply = Some(bytes.clone());
        drop(t);
        cleanup.run(self.vfs);
        Ok(Res::Raw(nfsstat4::NFS4_OK, bytes))
    }

    pub(super) fn op_create_session(&mut self, a: &CREATE_SESSION4args) -> Res {
        match self.create_session(a) {
            Ok(r) => r,
            Err(s) => Res::Err(s),
        }
    }

    fn sequence(&mut self, a: &SEQUENCE4args) -> R<Res> {
        let peer = self.ctx.peer;
        let mut t = self.srv.t();
        let s = t
            .sessions
            .get_mut(&a.sa_sessionid)
            .ok_or(nfsstat4::NFS4ERR_BADSESSION)?;
        let i = a.sa_slotid as usize;
        if i >= s.slots.len() {
            return Err(nfsstat4::NFS4ERR_BADSLOT);
        }
        if self.req_len > s.fore.ca_maxrequestsize as usize {
            return Err(nfsstat4::NFS4ERR_REQ_TOO_BIG);
        }
        if self.nops > s.fore.ca_maxoperations {
            return Err(nfsstat4::NFS4ERR_TOO_MANY_OPS);
        }
        let digest = self.digest;
        let slot = &mut s.slots[i];
        if a.sa_sequenceid == slot.seq && slot.seq != 0 {
            if slot.busy {
                return Err(nfsstat4::NFS4ERR_DELAY);
            }
            return match &slot.reply {
                Some(Cached::Reply(b)) if slot.digest == digest => {
                    Ok(Res::Raw(nfsstat4::NFS4_OK, b.clone()))
                }
                Some(Cached::Reply(_)) => Err(nfsstat4::NFS4ERR_SEQ_FALSE_RETRY),
                Some(Cached::Uncached) => Err(nfsstat4::NFS4ERR_RETRY_UNCACHED_REP),
                None => Err(nfsstat4::NFS4ERR_SEQ_MISORDERED),
            };
        }
        if a.sa_sequenceid != slot.seq.wrapping_add(1) {
            return Err(nfsstat4::NFS4ERR_SEQ_MISORDERED);
        }
        if slot.busy {
            return Err(nfsstat4::NFS4ERR_DELAY);
        }
        slot.busy = true;
        slot.seq = a.sa_sequenceid;
        slot.digest = digest;
        slot.reply = None;
        s.conns.insert(peer);
        let nslots = u32::try_from(s.slots.len()).unwrap_or(1);
        let here = s.back_conn.as_ref().is_some_and(|b| b.is_open()) && s.cb_sec.is_some();
        let client = s.client;
        let sessions = t
            .clients
            .get(&client)
            .map(|c| c.sessions.clone())
            .unwrap_or_default();
        let any = sessions.iter().any(|x| {
            t.sessions.get(x).is_some_and(|s| {
                s.back_conn.as_ref().is_some_and(|b| b.is_open()) && s.cb_sec.is_some()
            })
        });
        let c = t
            .clients
            .get_mut(&client)
            .ok_or(nfsstat4::NFS4ERR_BADSESSION)?;
        c.renewed = self.now;
        let mut flags = c.revoked;
        if !here {
            flags |= SEQ4_STATUS_CB_PATH_DOWN_SESSION;
        }
        if !any {
            flags |= SEQ4_STATUS_CB_PATH_DOWN;
        }
        self.session = Some((a.sa_sessionid, a.sa_slotid));
        self.client = Some(client);
        self.cachethis = a.sa_cachethis;
        Ok(Res::Ok(nfs_resop4::OP_SEQUENCE(SEQUENCE4res::NFS4_OK(
            SEQUENCE4resok {
                sr_sessionid: a.sa_sessionid,
                sr_sequenceid: a.sa_sequenceid,
                sr_slotid: a.sa_slotid,
                sr_highest_slotid: nslots - 1,
                sr_target_highest_slotid: nslots - 1,
                sr_status_flags: flags,
            },
        ))))
    }

    pub(super) fn op_sequence(&mut self, a: &SEQUENCE4args) -> Res {
        match self.sequence(a) {
            Ok(r) => r,
            Err(s) => Res::Err(s),
        }
    }

    /// The compound's reply goes in its slot (4.1).
    pub(crate) fn finish(&mut self, reply: Option<&Vec<u8>>) {
        let Some((sid, i)) = self.session else { return };
        let mut t = self.srv.t();
        if let Some(s) = t.sessions.get_mut(&sid)
            && let Some(slot) = s.slots.get_mut(i as usize)
        {
            slot.busy = false;
            slot.reply = Some(match reply {
                Some(b) if self.cachethis => Cached::Reply(b.clone()),
                _ => Cached::Uncached,
            });
        }
    }

    pub(super) fn op_destroy_session(&mut self, a: &DESTROY_SESSION4args) -> Res {
        let mut t = self.srv.t();
        let Some(s) = t.sessions.get(&a.dsa_sessionid) else {
            return Res::Err(nfsstat4::NFS4ERR_BADSESSION);
        };
        if !s.conns.contains(&self.ctx.peer) {
            return Res::Err(nfsstat4::NFS4ERR_CONN_NOT_BOUND_TO_SESSION);
        }
        if self.session.is_some_and(|(x, _)| x == a.dsa_sessionid) && self.index + 1 != self.nops {
            return Res::Err(nfsstat4::NFS4ERR_NOT_ONLY_OP);
        }
        let client = s.client;
        t.sessions.remove(&a.dsa_sessionid);
        if let Some(c) = t.clients.get_mut(&client) {
            c.sessions.retain(|x| *x != a.dsa_sessionid);
        }
        Res::Ok(nfs_resop4::OP_DESTROY_SESSION(DESTROY_SESSION4res {
            dsr_status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_bind_conn(&mut self, a: &BIND_CONN_TO_SESSION4args) -> Res {
        use channel_dir_from_client4 as C;
        let back = self.back.cloned();
        let mut t = self.srv.t();
        let Some(s) = t.sessions.get_mut(&a.bctsa_sessid) else {
            return Res::Err(nfsstat4::NFS4ERR_BADSESSION);
        };
        let dir = match (a.bctsa_dir, back) {
            (C::CDFC4_FORE, _) | (C::CDFC4_FORE_OR_BOTH, None) => {
                s.conns.insert(self.ctx.peer);
                channel_dir_from_server4::CDFS4_FORE
            }
            (C::CDFC4_BACK | C::CDFC4_BACK_OR_BOTH, None) => {
                return Res::Err(nfsstat4::NFS4ERR_INVAL);
            }
            (C::CDFC4_BACK, Some(b)) => {
                s.back_conn = Some(b);
                channel_dir_from_server4::CDFS4_BACK
            }
            (C::CDFC4_FORE_OR_BOTH | C::CDFC4_BACK_OR_BOTH, Some(b)) => {
                s.conns.insert(self.ctx.peer);
                s.back_conn = Some(b);
                channel_dir_from_server4::CDFS4_BOTH
            }
        };
        let client = s.client;
        if let Some(c) = t.clients.get_mut(&client) {
            c.renewed = self.now;
        }
        Res::Ok(nfs_resop4::OP_BIND_CONN_TO_SESSION(
            BIND_CONN_TO_SESSION4res::NFS4_OK(BIND_CONN_TO_SESSION4resok {
                bctsr_sessid: a.bctsa_sessid,
                bctsr_dir: dir,
                bctsr_use_conn_in_rdma_mode: false,
            }),
        ))
    }

    pub(super) fn op_backchannel_ctl(&mut self, a: &BACKCHANNEL_CTL4args) -> Res {
        let Some((sid, _)) = self.session else {
            return Res::Err(nfsstat4::NFS4ERR_OP_NOT_IN_SESSION);
        };
        let Some(cred) = usable(&a.bca_sec_parms) else {
            return Res::Err(nfsstat4::NFS4ERR_INVAL);
        };
        let mut t = self.srv.t();
        if let Some(s) = t.sessions.get_mut(&sid) {
            s.cb_prog = a.bca_cb_program;
            s.cb_sec = Some(cred);
        }
        Res::Ok(nfs_resop4::OP_BACKCHANNEL_CTL(BACKCHANNEL_CTL4res {
            bcr_status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_destroy_clientid(&mut self, a: &DESTROY_CLIENTID4args) -> Res {
        if client_boot(a.dca_clientid) != self.srv.boot {
            return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        }
        let mut t = self.srv.t();
        let Some(c) = t.clients.get(&a.dca_clientid).filter(|c| c.minor == 1) else {
            return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        };
        if !c.sessions.is_empty()
            || t.holds_state(a.dca_clientid)
            || self.client == Some(a.dca_clientid)
        {
            return Res::Err(nfsstat4::NFS4ERR_CLIENTID_BUSY);
        }
        let x = self.srv.drop_client(&mut t, a.dca_clientid, Gone::Expired);
        drop(t);
        x.run(self.vfs);
        Res::Ok(nfs_resop4::OP_DESTROY_CLIENTID(DESTROY_CLIENTID4res {
            dcr_status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_free_stateid(&mut self, a: &FREE_STATEID4args) -> Res {
        let s = match self.subst(&a.fsa_stateid) {
            Ok(s) if !is_special(&s) => s,
            Ok(_) => return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID),
            Err(e) => return Res::Err(e),
        };
        let (_, id) = split_other(&s.other);
        let mut t = self.srv.t();
        if let Some((_, client)) = t.gone.get(&id).copied() {
            if self.client != Some(client) {
                return Res::Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            t.gone.remove(&id);
            let left = t.gone.values().any(|(_, c)| *c == client);
            if !left && let Some(c) = t.clients.get_mut(&client) {
                c.revoked = 0;
            }
        } else {
            let id = match self.find_sid(&mut t, &s) {
                Ok(id) => id,
                Err(e) => return Res::Err(e),
            };
            match t.sids.get(&id).map(|x| &x.kind) {
                Some(Kind::Lock(l)) if l.ranges.is_empty() => {
                    let (owner, open) = (l.owner.clone(), l.open);
                    t.sids.remove(&id);
                    if let Some(Kind::Open(o)) = t.sids.get_mut(&open).map(|x| &mut x.kind) {
                        o.locks.retain(|x| *x != id);
                    }
                    if let Some(lo) = t.lock_owners.get_mut(&owner) {
                        lo.sids.retain(|x| *x != id);
                    }
                }
                _ => return Res::Err(nfsstat4::NFS4ERR_LOCKS_HELD),
            }
        }
        Res::Ok(nfs_resop4::OP_FREE_STATEID(FREE_STATEID4res {
            fsr_status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_test_stateid(&mut self, a: &TEST_STATEID4args) -> Res {
        let mut codes = Vec::new();
        for s in &a.ts_stateids {
            let st = if is_special(s) {
                nfsstat4::NFS4ERR_BAD_STATEID
            } else {
                let mut t = self.srv.t();
                match self.find_sid(&mut t, s) {
                    Ok(_) => nfsstat4::NFS4_OK,
                    Err(e) => e,
                }
            };
            codes.push(st);
        }
        Res::Ok(nfs_resop4::OP_TEST_STATEID(TEST_STATEID4res::NFS4_OK(
            TEST_STATEID4resok {
                tsr_status_codes: codes,
            },
        )))
    }

    pub(super) fn op_reclaim_complete(&mut self, a: &RECLAIM_COMPLETE4args) -> Res {
        let Some(client) = self.client else {
            return Res::Err(nfsstat4::NFS4ERR_OP_NOT_IN_SESSION);
        };
        if a.rca_one_fs {
            // One file system's reclaims done: the grace period is the
            // whole server's, so only the client's own completion counts.
            if let Err(s) = self.need() {
                return Res::Err(s);
            }
            return Res::Ok(nfs_resop4::OP_RECLAIM_COMPLETE(RECLAIM_COMPLETE4res {
                rcr_status: nfsstat4::NFS4_OK,
            }));
        }
        let mut t = self.srv.t();
        let Some(c) = t.clients.get_mut(&client) else {
            return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        };
        if c.reclaim_done {
            return Res::Err(nfsstat4::NFS4ERR_COMPLETE_ALREADY);
        }
        c.reclaim_done = true;
        self.srv.check_grace(&mut t, self.now);
        Res::Ok(nfs_resop4::OP_RECLAIM_COMPLETE(RECLAIM_COMPLETE4res {
            rcr_status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_set_ssv(a: &SET_SSV4args) -> Res {
        let _ = a;
        Res::Err(nfsstat4::NFS4ERR_INVAL)
    }
}

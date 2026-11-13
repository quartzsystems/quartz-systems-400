// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 4.0's client IDs (RFC 7530 section 16.33 and 16.34): `SETCLIENTID`
//! and `SETCLIENTID_CONFIRM` with the verifier and principal rules of
//! section 9.1.1, `RENEW`; and the delegations of both minor versions:
//! `DELEGRETURN`, `DELEGPURGE` (`NFS4ERR_NOTSUPP`: there is no
//! `CLAIM_DELEGATE_PREV`).

use crate::rpc::Credential;
use crate::types::nfs4::{
    DELEGPURGE4args, DELEGRETURN4args, DELEGRETURN4res, RENEW4args, RENEW4res, SETCLIENTID4args,
    SETCLIENTID4res, SETCLIENTID4resok, SETCLIENTID_CONFIRM4args, SETCLIENTID_CONFIRM4res,
    netaddr4, nfs_resop4, nfsstat4,
};
use crate::v4::compound::{Cx, Res};
use crate::v4::server::Cleanup;
use crate::v4::state::{Cb40, CbPath, Client, Gone, Kind, Principal, client_boot, client_id, is_special};

impl Cx<'_> {
    /// Who sent the request.
    pub(super) fn principal(&self) -> Principal {
        match &self.ctx.cred {
            Credential::Sys(p) => Principal::Sys(p.uid),
            Credential::None => Principal::None,
        }
    }

    /// A verifier no other confirmation has.
    pub(super) fn new_verf(&self, n: u64) -> [u8; 8] {
        let t = self.now.duration_since(self.srv.started).as_nanos();
        let x = (u64::from(self.srv.boot) << 32) ^ n.rotate_left(17) ^ (t as u64);
        x.to_be_bytes()
    }

    pub(super) fn op_setclientid(&mut self, a: &SETCLIENTID4args) -> Res {
        let who = self.principal();
        let owner = a.client.id.clone();
        let cb = Cb40 {
            prog: a.callback.cb_program,
            netid: String::from_utf8_lossy(&a.callback.cb_location.na_r_netid).into_owned(),
            addr: String::from_utf8_lossy(&a.callback.cb_location.na_r_addr).into_owned(),
            ident: a.callback_ident,
        };
        let mut t = self.srv.t();
        let lease = self.srv.cfg.lease;
        let confirmed = t
            .clients
            .values()
            .find(|c| c.minor == 0 && c.confirmed && c.owner == owner)
            .map(|c| (c.id, c.principal, c.verf, c.cb.clone(), c.lapsed(self.now, lease)));
        if let Some((id, p, _, ccb, lapsed)) = &confirmed
            && *p != who
            && !lapsed
            && t.holds_state(*id)
        {
            let using = ccb.clone().map_or(
                netaddr4 {
                    na_r_netid: Vec::new(),
                    na_r_addr: Vec::new(),
                },
                |c| netaddr4 {
                    na_r_netid: c.netid.into_bytes(),
                    na_r_addr: c.addr.into_bytes(),
                },
            );
            return Res::Body(
                nfsstat4::NFS4ERR_CLID_INUSE,
                nfs_resop4::OP_SETCLIENTID(SETCLIENTID4res::NFS4ERR_CLID_INUSE(using)),
            );
        }
        // An unconfirmed record of the owner is replaced.
        let old: Vec<u64> = t
            .clients
            .values()
            .filter(|c| c.minor == 0 && !c.confirmed && c.owner == owner)
            .map(|c| c.id)
            .collect();
        for id in old {
            t.clients.remove(&id);
        }
        let n = t.next_id();
        let confirm = self.new_verf(n);
        // The same client changing its callback keeps its ID.
        if let Some((id, p, verf, _, _)) = confirmed
            && p == who
            && verf == a.client.verifier
        {
            if let Some(c) = t.clients.get_mut(&id) {
                c.pending_cb = Some((confirm, cb));
            }
            return Res::Ok(nfs_resop4::OP_SETCLIENTID(SETCLIENTID4res::NFS4_OK(
                SETCLIENTID4resok {
                    clientid: id,
                    setclientid_confirm: confirm,
                },
            )));
        }
        let id = client_id(self.srv.boot, n);
        t.clients.insert(
            id,
            Client {
                id,
                minor: 0,
                owner,
                verf: a.client.verifier,
                principal: who,
                confirmed: false,
                confirm,
                cb: Some(cb),
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
        Res::Ok(nfs_resop4::OP_SETCLIENTID(SETCLIENTID4res::NFS4_OK(
            SETCLIENTID4resok {
                clientid: id,
                setclientid_confirm: confirm,
            },
        )))
    }

    pub(super) fn op_setclientid_confirm(&mut self, a: &SETCLIENTID_CONFIRM4args) -> Res {
        if client_boot(a.clientid) != self.srv.boot {
            return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        }
        let who = self.principal();
        let mut t = self.srv.t();
        let Some(c) = t.clients.get_mut(&a.clientid) else {
            return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
        };
        let mut cleanup = Cleanup::default();
        if c.confirmed {
            match c.pending_cb.take() {
                Some((v, cb)) if v == a.setclientid_confirm => {
                    if c.principal != who {
                        c.pending_cb = Some((v, cb));
                        return Res::Err(nfsstat4::NFS4ERR_CLID_INUSE);
                    }
                    c.cb = Some(cb);
                    c.confirm = v;
                    c.path = CbPath::Unknown;
                    c.renewed = self.now;
                    t.to_probe.push(a.clientid);
                }
                p => {
                    c.pending_cb = p;
                    if c.confirm != a.setclientid_confirm {
                        return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
                    }
                    c.renewed = self.now;
                }
            }
        } else {
            if c.confirm != a.setclientid_confirm {
                return Res::Err(nfsstat4::NFS4ERR_STALE_CLIENTID);
            }
            if c.principal != who {
                return Res::Err(nfsstat4::NFS4ERR_CLID_INUSE);
            }
            c.confirmed = true;
            c.renewed = self.now;
            let owner = c.owner.clone();
            // The client restarted: what its old ID held is gone.
            let old: Vec<u64> = t
                .clients
                .values()
                .filter(|o| o.minor == 0 && o.confirmed && o.owner == owner && o.id != a.clientid)
                .map(|o| o.id)
                .collect();
            for id in old {
                let x = self.srv.drop_client(&mut t, id, Gone::Expired);
                cleanup.closes.extend(x.closes);
                cleanup.subs.extend(x.subs);
                cleanup.leases.extend(x.leases);
            }
            t.to_probe.push(a.clientid);
        }
        drop(t);
        cleanup.run(self.vfs);
        Res::Ok(nfs_resop4::OP_SETCLIENTID_CONFIRM(SETCLIENTID_CONFIRM4res {
            status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_renew(&mut self, a: &RENEW4args) -> Res {
        if let Err(s) = self.client40(a.clientid) {
            return Res::Err(s);
        }
        let t = self.srv.t();
        let delegs = t
            .sids
            .values()
            .any(|s| s.client == a.clientid && matches!(s.kind, Kind::Deleg(_)));
        let down = t
            .clients
            .get(&a.clientid)
            .is_some_and(|c| c.path == CbPath::Down);
        if delegs && down {
            return Res::Err(nfsstat4::NFS4ERR_CB_PATH_DOWN);
        }
        Res::Ok(nfs_resop4::OP_RENEW(RENEW4res {
            status: nfsstat4::NFS4_OK,
        }))
    }

    pub(super) fn op_delegreturn(&mut self, a: &DELEGRETURN4args) -> Res {
        let r = (|| {
            let s = self.subst(&a.deleg_stateid)?;
            if is_special(&s) {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            let (_, h) = self.real(nfsstat4::NFS4ERR_BAD_STATEID)?;
            let mut t = self.srv.t();
            let id = self.find_sid(&mut t, &s)?;
            let x = t.sids.get(&id).ok_or(nfsstat4::NFS4ERR_BAD_STATEID)?;
            let Kind::Deleg(d) = &x.kind else {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            };
            if x.fh.fid != h.fid {
                return Err(nfsstat4::NFS4ERR_BAD_STATEID);
            }
            let tag = d.tag;
            t.sids.remove(&id);
            t.to_recall.retain(|x| *x != id);
            Ok(tag)
        })();
        match r {
            Ok(tag) => {
                let _ = self.vfs.lease_report(tag, &[]);
                Res::Ok(nfs_resop4::OP_DELEGRETURN(DELEGRETURN4res {
                    status: nfsstat4::NFS4_OK,
                }))
            }
            Err(s) => Res::Err(s),
        }
    }

    pub(super) fn op_delegpurge(&mut self, a: &DELEGPURGE4args) -> Res {
        let _ = a;
        Res::Err(nfsstat4::NFS4ERR_NOTSUPP)
    }
}

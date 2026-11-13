// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A version 4 mount's state on the server: its opens (an open owner
//! per IFS lock owner; 4.0's sequence IDs and `OPEN_CONFIRM`), its lock
//! stateids and the ranges they hold, and, for 4.0's `CLAIM_NULL`, the
//! directory and name each handle was found by. After a new client ID
//! everything is reclaimed (`CLAIM_PREVIOUS`, `LOCK` with `reclaim`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use ifs::Errno;
use ifs::remote::{RHandle, RResult, RemoteError};

use super::attr::{self, Attrs};
use super::core::Core;
use crate::client::ranges::{self, Range};
use crate::types::nfs4::{
    DELEGRETURN4args, GETATTR4args, GETATTR4res, GETFH4res, LOCK4args, LOCK4res, OPEN_CONFIRM4args,
    OPEN_CONFIRM4res, OPEN4_RESULT_CONFIRM, OPEN4_SHARE_ACCESS_WANT_NO_DELEG, OPEN4args, OPEN4res,
    PUTFH4args, createhow4, exist_lock_owner4, locker4, nfs_argop4, nfs_fh4, nfs_lock_type4,
    nfs_resop4, nfsstat4, open_claim4, open_delegation_type4, open_delegation4,
    open_to_lock_owner4, openflag4, opentype4, state_owner4, stateid4, utf8string,
};

/// One open.
#[derive(Debug, Clone)]
pub(super) struct Open {
    pub fh: RHandle,
    pub owner: u64,
    pub access: u32,
    pub deny: u32,
    pub stateid: stateid4,
    /// Who opened it (reclaimed as them).
    pub cred: crate::rpc::Credential,
}

/// The locks one owner holds on one file.
#[derive(Debug, Clone)]
pub(super) struct Locks {
    pub stateid: stateid4,
    pub seqid: u32,
    pub held: Vec<Range>,
}

/// Everything the mount holds.
#[derive(Debug, Default)]
pub(super) struct State {
    /// 4.1's `CREATE_SESSION` sequence.
    pub create_seq: u32,
    pub next: u64,
    pub opens: HashMap<u64, Open>,
    /// 4.0: each open owner's next sequence ID, held while it is used.
    pub owners: HashMap<u64, Arc<Mutex<u32>>>,
    pub locks: HashMap<(RHandle, u64), Locks>,
    pub names: HashMap<RHandle, (RHandle, Vec<u8>)>,
}

/// The special stateid of no open (anonymous).
#[must_use]
pub(super) fn anonymous() -> stateid4 {
    stateid4 {
        seqid: 0,
        other: [0; 12],
    }
}

impl State {
    /// An open owner's sequence.
    pub fn owner_seq(&mut self, owner: u64) -> Arc<Mutex<u32>> {
        Arc::clone(self.owners.entry(owner).or_default())
    }

    /// A stateid for I/O on `fh`: an open of it with `access`, else the
    /// anonymous one.
    pub fn io_stateid(&self, fh: &RHandle, access: u32) -> stateid4 {
        self.opens
            .values()
            .find(|o| &o.fh == fh && o.access & access != 0)
            .map_or_else(anonymous, |o| o.stateid.clone())
    }
}

/// The open owner of an IFS lock owner.
#[must_use]
pub(super) fn open_owner(core: &Core, owner: u64) -> state_owner4 {
    let mut o = b"open ".to_vec();
    o.extend_from_slice(&owner.to_be_bytes());
    state_owner4 {
        clientid: core.client().clientid,
        owner: o,
    }
}

/// The lock owner of an IFS lock owner.
#[must_use]
pub(super) fn lock_owner(core: &Core, owner: u64) -> state_owner4 {
    let mut o = b"lock ".to_vec();
    o.extend_from_slice(&owner.to_be_bytes());
    state_owner4 {
        clientid: core.client().clientid,
        owner: o,
    }
}

/// Whether a status moves an owner's sequence ID (RFC 7530 9.1.7).
#[must_use]
pub(super) fn moves_seqid(s: nfsstat4) -> bool {
    !matches!(
        s,
        nfsstat4::NFS4ERR_STALE_CLIENTID
            | nfsstat4::NFS4ERR_STALE_STATEID
            | nfsstat4::NFS4ERR_BAD_STATEID
            | nfsstat4::NFS4ERR_BAD_SEQID
            | nfsstat4::NFS4ERR_BADXDR
            | nfsstat4::NFS4ERR_RESOURCE
            | nfsstat4::NFS4ERR_NOFILEHANDLE
            | nfsstat4::NFS4ERR_MOVED
    )
}

/// What an `OPEN` names.
#[derive(Debug, Clone)]
pub(super) enum Claim {
    /// A name in a directory.
    Name(RHandle, Vec<u8>),
    /// The file itself (4.1).
    Fh(RHandle),
    /// The file, after the server restarted.
    Previous(RHandle),
}

/// What an `OPEN` gave.
#[derive(Debug, Clone)]
pub(super) struct Opened {
    pub stateid: stateid4,
    pub fh: RHandle,
    pub attrs: Attrs,
}

fn putfh(h: &RHandle) -> nfs_argop4 {
    nfs_argop4::OP_PUTFH(PUTFH4args {
        object: nfs_fh4(h.0.clone()),
    })
}

/// `OPEN` (with `OPEN_CONFIRM` in 4.0 when asked), as `owner`.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn open_wire(
    core: &Core,
    cred: &crate::rpc::Credential,
    claim: &Claim,
    owner: u64,
    access: u32,
    deny: u32,
    create: Option<&createhow4>,
    recover: bool,
) -> RResult<Opened> {
    let seq = core.state().owner_seq(owner);
    let mut seqid = seq.lock().unwrap_or_else(PoisonError::into_inner);
    let want = if core.minor == 1 {
        access | OPEN4_SHARE_ACCESS_WANT_NO_DELEG
    } else {
        access
    };
    let s0 = *seqid;
    let build = |c: &Core| {
        let (fh, claim) = match claim {
            Claim::Name(d, n) => (d, open_claim4::CLAIM_NULL(utf8string(n.clone()))),
            Claim::Fh(f) => (f, open_claim4::CLAIM_FH),
            Claim::Previous(f) => (
                f,
                open_claim4::CLAIM_PREVIOUS(open_delegation_type4::OPEN_DELEGATE_NONE),
            ),
        };
        vec![
            putfh(fh),
            nfs_argop4::OP_OPEN(OPEN4args {
                seqid: s0,
                share_access: want,
                share_deny: deny,
                owner: open_owner(c, owner),
                openhow: match create {
                    Some(h) => openflag4::OPEN4_CREATE(h.clone()),
                    None => openflag4::default(opentype4::OPEN4_NOCREATE),
                },
                claim,
            }),
            nfs_argop4::OP_GETFH,
            nfs_argop4::OP_GETATTR(GETATTR4args {
                attr_request: attr::bitmap(attr::OBJECT),
            }),
        ]
    };
    let (s, list) = core.send(cred, &build, recover)?;
    if core.minor == 0 && moves_seqid(s) {
        *seqid = seqid.wrapping_add(1);
    }
    if s != nfsstat4::NFS4_OK {
        return Err(crate::client::status::nfs4(s));
    }
    let bad = || RemoteError::Errno(Errno::EIO);
    let (ok, fh, attrs) = match (list.get(1), list.get(2), list.get(3)) {
        (
            Some(nfs_resop4::OP_OPEN(OPEN4res::NFS4_OK(ok))),
            Some(nfs_resop4::OP_GETFH(GETFH4res::NFS4_OK(fh))),
            Some(nfs_resop4::OP_GETATTR(GETATTR4res::NFS4_OK(a))),
        ) => (
            ok.clone(),
            RHandle(fh.object.0.clone()),
            attr::decode(&a.obj_attributes).map_err(|_| bad())?,
        ),
        _ => return Err(bad()),
    };
    let mut stateid = ok.stateid.clone();
    if core.minor == 0 && ok.rflags & OPEN4_RESULT_CONFIRM != 0 {
        let s1 = *seqid;
        let st = stateid.clone();
        let f2 = fh.clone();
        let (s, list) = core.send(
            cred,
            &|_| {
                vec![
                    putfh(&f2),
                    nfs_argop4::OP_OPEN_CONFIRM(OPEN_CONFIRM4args {
                        open_stateid: st.clone(),
                        seqid: s1,
                    }),
                ]
            },
            recover,
        )?;
        if moves_seqid(s) {
            *seqid = seqid.wrapping_add(1);
        }
        match list.get(1) {
            Some(nfs_resop4::OP_OPEN_CONFIRM(OPEN_CONFIRM4res::NFS4_OK(c))) => {
                stateid = c.open_stateid.clone();
            }
            _ => return Err(crate::client::status::nfs4(s)),
        }
    }
    drop(seqid);
    // Never asked for; given anyway, it goes straight back.
    let deleg = match &ok.delegation {
        open_delegation4::OPEN_DELEGATE_READ(d) => Some(d.stateid.clone()),
        open_delegation4::OPEN_DELEGATE_WRITE(d) => Some(d.stateid.clone()),
        _ => None,
    };
    if let Some(d) = deleg {
        let f2 = fh.clone();
        let _ = core.send(
            cred,
            &|_| {
                vec![
                    putfh(&f2),
                    nfs_argop4::OP_DELEGRETURN(DELEGRETURN4args {
                        deleg_stateid: d.clone(),
                    }),
                ]
            },
            recover,
        );
    }
    Ok(Opened { stateid, fh, attrs })
}

/// `LOCK` of `r` for `owner` on `fh` (`reclaim` after a restart); the
/// open it starts from when the owner holds no lock there yet.
#[allow(clippy::too_many_arguments)]
pub(super) fn lock_wire(
    core: &Core,
    cred: &crate::rpc::Credential,
    fh: &RHandle,
    owner: u64,
    r: Range,
    open: Option<&Open>,
    reclaim: bool,
) -> RResult<Result<(), nfsstat4>> {
    let existing = core.state().locks.get(&(fh.clone(), owner)).cloned();
    let seq = core.state().owner_seq(owner);
    let mut oseq = seq.lock().unwrap_or_else(PoisonError::into_inner);
    let (o0, l0) = (*oseq, existing.as_ref().map_or(0, |l| l.seqid));
    let first = existing.is_none();
    if first && open.is_none() {
        return Err(RemoteError::Errno(Errno::EBADF));
    }
    let build = |c: &Core| {
        let locker = match (&existing, open) {
            (Some(l), _) => locker4::FALSE(exist_lock_owner4 {
                lock_stateid: l.stateid.clone(),
                lock_seqid: l0,
            }),
            (None, Some(o)) => locker4::TRUE(open_to_lock_owner4 {
                open_seqid: o0,
                open_stateid: o.stateid.clone(),
                lock_seqid: 0,
                lock_owner: lock_owner(c, owner),
            }),
            (None, None) => locker4::FALSE(exist_lock_owner4 {
                lock_stateid: anonymous(),
                lock_seqid: 0,
            }),
        };
        vec![
            putfh(fh),
            nfs_argop4::OP_LOCK(LOCK4args {
                locktype: if r.exclusive {
                    nfs_lock_type4::WRITE_LT
                } else {
                    nfs_lock_type4::READ_LT
                },
                reclaim,
                offset: r.off,
                length: if r.len == 0 { u64::MAX } else { r.len },
                locker,
            }),
        ]
    };
    let (s, list) = core.send(cred, &build, !reclaim)?;
    if core.minor == 0 && first && moves_seqid(s) {
        *oseq = oseq.wrapping_add(1);
    }
    drop(oseq);
    if let Some(nfs_resop4::OP_LOCK(LOCK4res::NFS4_OK(ok))) = list.get(1) {
        let mut st = core.state();
        let l = st.locks.entry((fh.clone(), owner)).or_insert(Locks {
            stateid: ok.lock_stateid.clone(),
            seqid: 0,
            held: Vec::new(),
        });
        l.stateid = ok.lock_stateid.clone();
        l.seqid = if first { 1 } else { l0.wrapping_add(1) };
        ranges::add(&mut l.held, r);
        return Ok(Ok(()));
    }
    if !first
        && moves_seqid(s)
        && let Some(l) = core.state().locks.get_mut(&(fh.clone(), owner))
    {
        l.seqid = l0.wrapping_add(1);
    }
    Ok(Err(s))
}

/// After a new client ID: every open and lock taken again in the
/// server's grace period. What could not be is dropped and logged.
pub(super) fn reclaim(core: &Core) {
    let opens: Vec<(u64, Open)> = {
        let mut st = core.state();
        // A new client: its owners start again.
        st.owners.clear();
        st.opens.iter().map(|(k, o)| (*k, o.clone())).collect()
    };
    for (id, o) in opens {
        match open_wire(
            core,
            &o.cred,
            &Claim::Previous(o.fh.clone()),
            o.owner,
            o.access,
            o.deny,
            None,
            false,
        ) {
            Ok(op) => {
                if let Some(x) = core.state().opens.get_mut(&id) {
                    x.stateid = op.stateid;
                }
            }
            Err(e) => {
                tracing::warn!(?e, "an open was not reclaimed");
                core.state().opens.remove(&id);
            }
        }
    }
    let locks: Vec<((RHandle, u64), Locks)> = {
        let mut st = core.state();
        st.locks.drain().collect()
    };
    for ((fh, owner), l) in locks {
        let open = core
            .state()
            .opens
            .values()
            .find(|o| o.fh == fh && o.owner == owner)
            .cloned();
        for r in l.held {
            let cred = open
                .as_ref()
                .map_or_else(|| core.owner.clone(), |o| o.cred.clone());
            match lock_wire(core, &cred, &fh, owner, r, open.as_ref(), true) {
                Ok(Ok(())) => {}
                other => tracing::warn!(?other, "a lock was not reclaimed"),
            }
        }
    }
}

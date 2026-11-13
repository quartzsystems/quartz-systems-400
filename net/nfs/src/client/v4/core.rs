// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A version 4 mount's connection to its server: the client ID (4.0
//! `SETCLIENTID`, 4.1 `EXCHANGE_ID` and one session with a small slot
//! table), every `COMPOUND` (`SEQUENCE` first in 4.1), `DELAY` and
//! `GRACE` waited out, and the client ID or session established again
//! when the server lost it (`BADSESSION`, `STALE_CLIENTID`, `EXPIRED`),
//! the opens and locks then reclaimed ([`super::state`]). No callback
//! path is offered (4.0: an unroutable callback address; 4.1: no back
//! channel), so the server offers no delegations.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ifs::Errno;
use ifs::remote::{RResult, RemoteError};

use super::state::State;
use crate::client::channel::Channel;
use crate::client::settings::Policy;
use crate::rpc::Credential;
use crate::types::nfs4::NFS4_PROGRAM::NFS_V4::{Args, Res};
use crate::types::nfs4::{
    COMPOUND4args, CREATE_SESSION4args, CREATE_SESSION4res, DESTROY_CLIENTID4args,
    DESTROY_SESSION4args, EXCHANGE_ID4args, EXCHANGE_ID4res, EXCHGID4_FLAG_USE_NON_PNFS,
    RECLAIM_COMPLETE4args, RENEW4args, SEQUENCE4args, SEQUENCE4res, SETCLIENTID_CONFIRM4args,
    SETCLIENTID4args, SETCLIENTID4res, callback_sec_parms4, cb_client4, channel_attrs4,
    client_owner4, netaddr4, nfs_argop4, nfs_client_id4, nfs_resop4, nfsstat4, state_protect4_a,
    utf8string,
};

/// The callback program number this client names (and never serves).
const CB_PROGRAM: u32 = 0x4000_0000;
/// Slots asked for.
const SLOTS: u32 = 16;
/// The largest request and reply this client sends and takes.
const MAX_MSG: u32 = (1 << 20) + 64 * 1024;

/// The client ID and, in 4.1, the session.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Client {
    pub clientid: u64,
    pub session: [u8; 16],
}

#[derive(Debug, Clone, Copy, Default)]
struct Slot {
    seq: u32,
    busy: bool,
}

/// What a compound answered: its status and the results of the
/// operations it got to (without `SEQUENCE`).
pub(super) type Answer = (nfsstat4, Vec<nfs_resop4>);

/// The connection.
pub(super) struct Core {
    pub chan: Channel,
    pub minor: u32,
    pub policy: Policy,
    /// The `AUTH_SYS` machine name.
    pub host: Vec<u8>,
    /// Who mounted.
    pub owner: Credential,
    /// This mount's client owner and verifier.
    pub id: Vec<u8>,
    pub verifier: [u8; 8],
    client: Mutex<Client>,
    slots: Mutex<Vec<Slot>>,
    slot_cv: Condvar,
    recovering: Mutex<()>,
    /// Moves at every new client ID or session.
    pub epoch: AtomicU64,
    pub state: Mutex<State>,
    pub stopped: AtomicBool,
    pub lease: Mutex<Duration>,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("chan", &self.chan)
            .field("minor", &self.minor)
            .finish_non_exhaustive()
    }
}

fn recoverable(s: nfsstat4) -> bool {
    matches!(
        s,
        nfsstat4::NFS4ERR_BADSESSION
            | nfsstat4::NFS4ERR_DEADSESSION
            | nfsstat4::NFS4ERR_STALE_CLIENTID
            | nfsstat4::NFS4ERR_EXPIRED
            | nfsstat4::NFS4ERR_STALE_STATEID
    )
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Core {
    /// A connection over `chan` (nothing sent yet).
    pub fn new(chan: Channel, minor: u32, policy: Policy, host: &[u8], owner: Credential) -> Self {
        let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let boot = (t.as_secs() << 20) ^ u64::from(t.subsec_nanos()) ^ (n << 48);
        let mut id = b"QS400 ".to_vec();
        id.extend_from_slice(host);
        id.extend_from_slice(format!(" {} {n} {}", std::process::id(), t.as_secs()).as_bytes());
        Self {
            chan,
            minor,
            policy,
            host: host.to_vec(),
            owner,
            id,
            verifier: boot.to_be_bytes(),
            client: Mutex::new(Client::default()),
            slots: Mutex::new(Vec::new()),
            slot_cv: Condvar::new(),
            recovering: Mutex::new(()),
            epoch: AtomicU64::new(0),
            state: Mutex::new(State::default()),
            stopped: AtomicBool::new(false),
            lease: Mutex::new(Duration::from_secs(90)),
        }
    }

    pub fn client(&self) -> Client {
        *self.client.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn take_slot(&self) -> (u32, u32, u32) {
        let mut s = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            let n = u32::try_from(s.len()).unwrap_or(1).max(1);
            if let Some((i, slot)) = s.iter_mut().enumerate().find(|(_, x)| !x.busy) {
                slot.busy = true;
                return (u32::try_from(i).unwrap_or(0), slot.seq, n - 1);
            }
            s = self.slot_cv.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn give_slot(&self, i: u32, advance: bool) {
        let mut s = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(x) = s.get_mut(i as usize) {
            x.busy = false;
            if advance {
                x.seq = x.seq.wrapping_add(1);
            }
        }
        drop(s);
        self.slot_cv.notify_one();
    }

    /// One compound as `cred`, built by `build` (again after a
    /// recovery, with the state then in effect). `DELAY` and `GRACE`
    /// wait and send again; a lost session or client ID is established
    /// again when `recover` (never from a recovery's own compounds).
    pub fn send(
        &self,
        cred: &Credential,
        build: &dyn Fn(&Self) -> Vec<nfs_argop4>,
        recover: bool,
    ) -> RResult<Answer> {
        self.send_with(cred, build, recover, self.minor == 1)
    }

    /// As [`Self::send`]; `seq`: 4.1's `SEQUENCE` first (not before
    /// `EXCHANGE_ID` and `CREATE_SESSION`, which go alone).
    fn send_with(
        &self,
        cred: &Credential,
        build: &dyn Fn(&Self) -> Vec<nfs_argop4>,
        recover: bool,
        seq: bool,
    ) -> RResult<Answer> {
        let mut waits = 0u32;
        let mut misordered = 0;
        loop {
            if self.stopped.load(Ordering::Relaxed) {
                return Err(RemoteError::Interrupted);
            }
            let epoch = self.epoch.load(Ordering::Relaxed);
            let mut ops = build(self);
            let slot = seq.then(|| {
                let (i, seq, high) = self.take_slot();
                let c = self.client();
                ops.insert(
                    0,
                    nfs_argop4::OP_SEQUENCE(SEQUENCE4args {
                        sa_sessionid: c.session,
                        sa_sequenceid: seq,
                        sa_slotid: i,
                        sa_highest_slotid: high,
                        sa_cachethis: false,
                    }),
                );
                i
            });
            let args = Args::NFSPROC4_COMPOUND(COMPOUND4args {
                tag: utf8string(Vec::new()),
                minorversion: self.minor,
                argarray: ops,
            });
            let r = self.chan.call::<Args, Res>(cred, &args);
            let res = match r {
                Ok(Res::NFSPROC4_COMPOUND(c)) => c,
                Ok(Res::NFSPROC4_NULL) => {
                    if let Some(i) = slot {
                        self.give_slot(i, false);
                    }
                    return Err(RemoteError::Errno(Errno::EIO));
                }
                Err(e) => {
                    if let Some(i) = slot {
                        self.give_slot(i, false);
                    }
                    return Err(e);
                }
            };
            let mut list = res.resarray;
            let mut status = res.status;
            if let Some(i) = slot {
                let seq = match list.first() {
                    Some(nfs_resop4::OP_SEQUENCE(SEQUENCE4res::NFS4_OK(_))) => None,
                    Some(nfs_resop4::OP_SEQUENCE(SEQUENCE4res::default(s))) => Some(*s),
                    _ if status == nfsstat4::NFS4_OK => Some(nfsstat4::NFS4ERR_SERVERFAULT),
                    _ => Some(status),
                };
                self.give_slot(i, seq.is_none());
                match seq {
                    None => {
                        list.remove(0);
                    }
                    Some(nfsstat4::NFS4ERR_SEQ_MISORDERED) if misordered < 2 => {
                        // Out of step with the server's slot: move on.
                        misordered += 1;
                        self.give_slot_bump(i);
                        continue;
                    }
                    Some(s) => {
                        status = s;
                        list.clear();
                    }
                }
            }
            match status {
                nfsstat4::NFS4_OK => return Ok((status, list)),
                nfsstat4::NFS4ERR_DELAY | nfsstat4::NFS4ERR_GRACE => {
                    waits += 1;
                    let wait = Duration::from_millis(100)
                        .saturating_mul(1 << waits.min(5))
                        .min(Duration::from_secs(2));
                    if self.policy.soft && wait.saturating_mul(waits) > self.policy.major() {
                        return Err(RemoteError::TimedOut);
                    }
                    std::thread::sleep(wait);
                }
                s if recover && recoverable(s) => self.recover(epoch, s)?,
                _ => return Ok((status, list)),
            }
        }
    }

    fn give_slot_bump(&self, i: u32) {
        let mut s = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(x) = s.get_mut(i as usize) {
            x.seq = x.seq.wrapping_add(1);
        }
    }

    /// A compound whose every operation must succeed: the results.
    pub fn run(
        &self,
        cred: &Credential,
        build: &dyn Fn(&Self) -> Vec<nfs_argop4>,
    ) -> RResult<Vec<nfs_resop4>> {
        let (s, list) = self.send(cred, build, true)?;
        if s == nfsstat4::NFS4_OK {
            Ok(list)
        } else {
            Err(crate::client::status::nfs4(s))
        }
    }

    /// The client ID (and session) for the first time or again.
    pub fn establish(&self) -> RResult<()> {
        if self.minor == 1 {
            self.exchange_id()?;
            self.create_session()
        } else {
            self.setclientid()
        }
    }

    fn exchange_id(&self) -> RResult<()> {
        let id = self.id.clone();
        let v = self.verifier;
        let (s, list) = self.send_with(
            &self.owner,
            &|_| {
                vec![nfs_argop4::OP_EXCHANGE_ID(EXCHANGE_ID4args {
                    eia_clientowner: client_owner4 {
                        co_verifier: v,
                        co_ownerid: id.clone(),
                    },
                    eia_flags: EXCHGID4_FLAG_USE_NON_PNFS,
                    eia_state_protect: state_protect4_a::SP4_NONE,
                    eia_client_impl_id: Vec::new(),
                })]
            },
            false,
            false,
        )?;
        match (s, list.first()) {
            (nfsstat4::NFS4_OK, Some(nfs_resop4::OP_EXCHANGE_ID(EXCHANGE_ID4res::NFS4_OK(ok)))) => {
                let mut c = self.client.lock().unwrap_or_else(PoisonError::into_inner);
                c.clientid = ok.eir_clientid;
                drop(c);
                self.state().create_seq = ok.eir_sequenceid;
                Ok(())
            }
            (nfsstat4::NFS4_OK, _) => Err(RemoteError::Errno(Errno::EIO)),
            (s, _) => Err(crate::client::status::nfs4(s)),
        }
    }

    fn create_session(&self) -> RResult<()> {
        let c = self.client();
        let seq = self.state().create_seq;
        let chan = |reqs: u32, max: u32| channel_attrs4 {
            ca_headerpadsize: 0,
            ca_maxrequestsize: max,
            ca_maxresponsesize: max,
            ca_maxresponsesize_cached: 64 * 1024,
            ca_maxoperations: 16,
            ca_maxrequests: reqs,
            ca_rdma_ird: Vec::new(),
        };
        let (s, list) = self.send_with(
            &self.owner,
            &|_| {
                vec![nfs_argop4::OP_CREATE_SESSION(CREATE_SESSION4args {
                    csa_clientid: c.clientid,
                    csa_sequence: seq,
                    // No back channel: no callbacks, no delegations.
                    csa_flags: 0,
                    csa_fore_chan_attrs: chan(SLOTS, MAX_MSG),
                    csa_back_chan_attrs: chan(1, 4096),
                    csa_cb_program: CB_PROGRAM,
                    csa_sec_parms: vec![callback_sec_parms4::AUTH_NONE],
                })]
            },
            false,
            false,
        )?;
        match (s, list.first()) {
            (
                nfsstat4::NFS4_OK,
                Some(nfs_resop4::OP_CREATE_SESSION(CREATE_SESSION4res::NFS4_OK(ok))),
            ) => {
                let n = ok.csr_fore_chan_attrs.ca_maxrequests.clamp(1, SLOTS);
                *self.slots.lock().unwrap_or_else(PoisonError::into_inner) = vec![
                    Slot {
                        seq: 1,
                        busy: false
                    };
                    n as usize
                ];
                self.slot_cv.notify_all();
                self.client
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .session = ok.csr_sessionid;
                self.state().create_seq = seq.wrapping_add(1);
                Ok(())
            }
            (nfsstat4::NFS4_OK, _) => Err(RemoteError::Errno(Errno::EIO)),
            (s, _) => Err(crate::client::status::nfs4(s)),
        }
    }

    fn setclientid(&self) -> RResult<()> {
        let id = self.id.clone();
        let v = self.verifier;
        let (s, list) = self.send(
            &self.owner,
            &|_| {
                vec![nfs_argop4::OP_SETCLIENTID(SETCLIENTID4args {
                    client: nfs_client_id4 {
                        verifier: v,
                        id: id.clone(),
                    },
                    // Unroutable: the server cannot call back, so it
                    // offers no delegations.
                    callback: cb_client4 {
                        cb_program: CB_PROGRAM,
                        cb_location: netaddr4 {
                            na_r_netid: b"tcp".to_vec(),
                            na_r_addr: b"0.0.0.0.0.0".to_vec(),
                        },
                    },
                    callback_ident: 0,
                })]
            },
            false,
        )?;
        let (clientid, confirm) = match (s, list.first()) {
            (nfsstat4::NFS4_OK, Some(nfs_resop4::OP_SETCLIENTID(SETCLIENTID4res::NFS4_OK(ok)))) => {
                (ok.clientid, ok.setclientid_confirm)
            }
            (nfsstat4::NFS4_OK, _) => return Err(RemoteError::Errno(Errno::EIO)),
            (s, _) => return Err(crate::client::status::nfs4(s)),
        };
        let (s, _) = self.send(
            &self.owner,
            &|_| {
                vec![nfs_argop4::OP_SETCLIENTID_CONFIRM(
                    SETCLIENTID_CONFIRM4args {
                        clientid,
                        setclientid_confirm: confirm,
                    },
                )]
            },
            false,
        )?;
        if s != nfsstat4::NFS4_OK {
            return Err(crate::client::status::nfs4(s));
        }
        self.client
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clientid = clientid;
        Ok(())
    }

    /// 4.1: done reclaiming (or nothing to reclaim).
    pub fn reclaim_complete(&self) -> RResult<()> {
        if self.minor != 1 {
            return Ok(());
        }
        let (s, _) = self.send(
            &self.owner,
            &|_| {
                vec![nfs_argop4::OP_RECLAIM_COMPLETE(RECLAIM_COMPLETE4args {
                    rca_one_fs: false,
                })]
            },
            false,
        )?;
        match s {
            nfsstat4::NFS4_OK | nfsstat4::NFS4ERR_COMPLETE_ALREADY => Ok(()),
            s => Err(crate::client::status::nfs4(s)),
        }
    }

    /// The server lost the session or the client ID (`why`), seen in
    /// `epoch`: establish them again, and after a new client ID reclaim
    /// the opens and locks. One recovery for all who saw it.
    fn recover(&self, epoch: u64, why: nfsstat4) -> RResult<()> {
        let _g = self
            .recovering
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if self.epoch.load(Ordering::Relaxed) != epoch {
            return Ok(());
        }
        tracing::info!(?why, "NFSv4 client state recovered");
        let session_only = self.minor == 1
            && matches!(
                why,
                nfsstat4::NFS4ERR_BADSESSION | nfsstat4::NFS4ERR_DEADSESSION
            );
        if session_only && self.create_session().is_ok() {
            self.epoch.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        self.establish()?;
        self.epoch.fetch_add(1, Ordering::Relaxed);
        super::state::reclaim(self);
        self.reclaim_complete()
    }

    /// Keep the lease: `SEQUENCE` alone (4.1) or `RENEW` (4.0).
    pub fn renew(&self) -> RResult<()> {
        let (s, _) = self.send(
            &self.owner,
            &|c: &Self| {
                if c.minor == 1 {
                    Vec::new()
                } else {
                    vec![nfs_argop4::OP_RENEW(RENEW4args {
                        clientid: c.client().clientid,
                    })]
                }
            },
            true,
        )?;
        if s == nfsstat4::NFS4_OK {
            Ok(())
        } else {
            Err(crate::client::status::nfs4(s))
        }
    }

    /// The mount ends: 4.1's session and client ID are destroyed.
    pub fn destroy(&self) {
        if self.minor == 1 {
            let c = self.client();
            let _ = self.send(
                &self.owner,
                &|_| {
                    vec![nfs_argop4::OP_DESTROY_SESSION(DESTROY_SESSION4args {
                        dsa_sessionid: c.session,
                    })]
                },
                false,
            );
            // DESTROY_CLIENTID goes without SEQUENCE: alone.
            let args = Args::NFSPROC4_COMPOUND(COMPOUND4args {
                tag: utf8string(Vec::new()),
                minorversion: 1,
                argarray: vec![nfs_argop4::OP_DESTROY_CLIENTID(DESTROY_CLIENTID4args {
                    dca_clientid: c.clientid,
                })],
            });
            let _ = self.chan.call::<Args, Res>(&self.owner, &args);
        }
    }
}

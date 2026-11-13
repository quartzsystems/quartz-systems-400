// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server's `NFSv4` state (RFC 7530 section 9, RFC 8881 section 8):
//! clients, sessions, open and lock owners, stateids, delegations, the
//! grace period. One [`Tables`] per job behind the server's mutex, never
//! held across a file system call. Several `QNFSNFSD` jobs each keep
//! their own: what conflicts across jobs (share reservations, locks,
//! leases) is enforced by the IFS.
//!
//! Identifiers: a client ID is the start's boot number (32 bits) and a
//! counter; a stateid's `other` is the boot number and a counter, so an
//! ID of an earlier start is told by its first four bytes
//! (`NFS4ERR_STALE_CLIENTID`, `NFS4ERR_STALE_STATEID`).

pub mod stable;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::rpc::back::BackChannel;
use crate::server::FileHandle;
use crate::types::nfs4::{channel_attrs4, nfsstat4, stateid4};

/// The first lock sub-owner of `NFSv4` lock owners in the IFS (`N4`);
/// NLM's start at `0x4E4C...`.
pub const SUB_BASE: u64 = 0x4E34_0000_0000_0000;

/// Who sent a request, as client records compare it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Principal {
    /// `AUTH_NONE`.
    None,
    /// `AUTH_SYS` with this UID.
    Sys(u32),
}

/// A 4.0 client's callback (`SETCLIENTID`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cb40 {
    /// The program number.
    pub prog: u32,
    /// `r_netid`.
    pub netid: String,
    /// `r_addr`.
    pub addr: String,
    /// `callback_ident`.
    pub ident: u32,
}

/// The state of a callback path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbPath {
    /// Not tried yet.
    Unknown,
    /// Answered.
    Up,
    /// Failed.
    Down,
}

/// A client ID.
#[derive(Debug)]
pub struct Client {
    /// The ID.
    pub id: u64,
    /// 0 or 1.
    pub minor: u32,
    /// `nfs_client_id4.id` or `co_ownerid`.
    pub owner: Vec<u8>,
    /// The client's verifier.
    pub verf: [u8; 8],
    /// Who made it.
    pub principal: Principal,
    /// Confirmed (`SETCLIENTID_CONFIRM`, `CREATE_SESSION`).
    pub confirmed: bool,
    /// 4.0: `setclientid_confirm`.
    pub confirm: [u8; 8],
    /// 4.0: the callback.
    pub cb: Option<Cb40>,
    /// The callback path.
    pub path: CbPath,
    /// Last renewed.
    pub renewed: Instant,
    /// 4.1: the last `CREATE_SESSION` sequence ID.
    pub cs_seq: u32,
    /// 4.1: that `CREATE_SESSION`'s encoded result.
    pub cs_reply: Option<Vec<u8>>,
    /// 4.1: its sessions.
    pub sessions: Vec<[u8; 16]>,
    /// In the stable records as holding state.
    pub recorded: bool,
    /// Done reclaiming (`RECLAIM_COMPLETE`, or a 4.0 client's first
    /// request that is not a reclaim).
    pub reclaim_done: bool,
    /// `SEQ4_STATUS_*` flags of state revoked.
    pub revoked: u32,
    /// 4.0: a callback change waiting for its confirmation.
    pub pending_cb: Option<([u8; 8], Cb40)>,
}

impl Client {
    /// Whether its lease has run out at `now`.
    #[must_use]
    pub fn lapsed(&self, now: Instant, lease: std::time::Duration) -> bool {
        now.saturating_duration_since(self.renewed) > lease
    }
}

/// A reply kept for a retransmission (4.0 seqid ops).
#[derive(Debug, Clone)]
pub struct Replay {
    /// The seqid it answered.
    pub seqid: u32,
    /// The encoded `nfs_resop4`.
    pub bytes: Vec<u8>,
    /// The current filehandle after it.
    pub fh: Option<FileHandle>,
}

/// An open owner.
#[derive(Debug, Default)]
pub struct OpenOwner {
    /// Confirmed (4.0 `OPEN_CONFIRM`; 4.1 always).
    pub confirmed: bool,
    /// The last seqid taken.
    pub seq: Option<u32>,
    /// Its reply.
    pub last: Option<Replay>,
    /// A request of it runs.
    pub busy: bool,
    /// Its open stateids.
    pub opens: Vec<u64>,
    /// Last used.
    pub used: Option<Instant>,
}

/// A lock owner.
#[derive(Debug)]
pub struct LockOwner {
    /// Its IFS sub-owner.
    pub sub: u64,
    /// The last seqid taken.
    pub seq: Option<u32>,
    /// Its reply.
    pub last: Option<Replay>,
    /// A request of it runs.
    pub busy: bool,
    /// Its lock stateids.
    pub sids: Vec<u64>,
}

/// An owner: the client and its bytes.
pub type OwnerKey = (u64, Vec<u8>);

/// An open.
#[derive(Debug, Clone)]
pub struct OpenSt {
    /// Its owner.
    pub owner: OwnerKey,
    /// `OPEN4_SHARE_ACCESS_*` now.
    pub access: u32,
    /// `OPEN4_SHARE_DENY_*` now.
    pub deny: u32,
    /// Every access asked by its opens (bit `1 << access`).
    pub used_access: u8,
    /// Every deny asked by its opens.
    pub used_deny: u8,
    /// The file system's open.
    pub fd: u64,
    /// Made by a reclaim.
    pub reclaimed: bool,
    /// Its lock stateids.
    pub locks: Vec<u64>,
}

/// A lock stateid: one lock owner's locks on one file.
#[derive(Debug, Clone)]
pub struct LockSt {
    /// Its owner.
    pub owner: OwnerKey,
    /// The open it locks through.
    pub open: u64,
    /// The ranges held, `[start, end)`, `end` `u64::MAX` to the end.
    pub ranges: Vec<(u64, u64)>,
}

/// A recall under way.
#[derive(Debug, Clone, Copy)]
pub struct Recall {
    /// When it began.
    pub since: Instant,
    /// `CB_RECALL` was sent (or tried).
    pub sent: bool,
}

/// A read delegation.
#[derive(Debug, Clone)]
pub struct DelegSt {
    /// Its IFS lease's tag.
    pub tag: u64,
    /// Its recall.
    pub recall: Option<Recall>,
}

/// What a stateid stands for.
#[derive(Debug, Clone)]
pub enum Kind {
    /// An open.
    Open(OpenSt),
    /// Locks.
    Lock(LockSt),
    /// A read delegation.
    Deleg(DelegSt),
}

/// A stateid.
#[derive(Debug, Clone)]
pub struct Sid {
    /// Its number (`other`'s last eight bytes).
    pub id: u64,
    /// Its current seqid.
    pub seq: u32,
    /// Its client.
    pub client: u64,
    /// Its file.
    pub fh: FileHandle,
    /// What it is.
    pub kind: Kind,
}

/// Why a stateid is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    /// Its client's lease ran out.
    Expired,
    /// A delegation not returned in time.
    DelegRevoked,
    /// Taken by the administrator (`RLSIFSLCK`).
    AdminRevoked,
}

/// A cached reply of a slot.
#[derive(Debug, Clone)]
pub enum Cached {
    /// The whole `COMPOUND4res`.
    Reply(Vec<u8>),
    /// Not kept (`sa_cachethis` false).
    Uncached,
}

/// A fore-channel slot.
#[derive(Debug, Clone)]
pub struct Slot {
    /// The last sequence ID.
    pub seq: u32,
    /// A request runs on it.
    pub busy: bool,
    /// The reply to the last one.
    pub reply: Option<Cached>,
    /// A digest of the last request.
    pub digest: u64,
}

/// A session.
#[derive(Debug)]
pub struct Session {
    /// The ID.
    pub id: [u8; 16],
    /// Its client.
    pub client: u64,
    /// Fore channel attributes.
    pub fore: channel_attrs4,
    /// Back channel attributes.
    pub back: channel_attrs4,
    /// The slots.
    pub slots: Vec<Slot>,
    /// The callback program.
    pub cb_prog: u32,
    /// A flavor the backchannel may use (`AUTH_NONE` or `AUTH_SYS`).
    pub cb_sec: Option<crate::rpc::Credential>,
    /// The backchannel's connection.
    pub back_conn: Option<Arc<BackChannel>>,
    /// The connections bound.
    pub conns: HashSet<SocketAddr>,
    /// The back slot's sequence ID.
    pub cb_seq: u32,
}

/// The grace period.
#[derive(Debug)]
pub struct Grace {
    /// Until when (none: over).
    pub until: Option<Instant>,
    /// The owners that may reclaim (and their minor versions).
    pub reclaimers: Vec<(Vec<u8>, u32)>,
}

/// Everything.
#[derive(Debug)]
pub struct Tables {
    /// Clients by ID.
    pub clients: HashMap<u64, Client>,
    /// Sessions by ID.
    pub sessions: HashMap<[u8; 16], Session>,
    /// Stateids by number.
    pub sids: HashMap<u64, Sid>,
    /// Open owners.
    pub open_owners: HashMap<OwnerKey, OpenOwner>,
    /// Lock owners.
    pub lock_owners: HashMap<OwnerKey, LockOwner>,
    /// Stateids gone, and why; with their client.
    pub gone: HashMap<u64, (Gone, u64)>,
    /// The grace period.
    pub grace: Grace,
    /// The stable records.
    pub stable: stable::StableImage,
    /// Delegations to recall, by stateid number.
    pub to_recall: Vec<u64>,
    /// 4.0 callback paths to probe, by client.
    pub to_probe: Vec<u64>,
    /// Open stateids closed, and their owners (4.0 retransmissions).
    pub closed: HashMap<u64, OwnerKey>,
    next: u64,
    next_sub: u64,
}

impl Tables {
    /// Empty tables of start `boot`.
    #[must_use]
    pub fn new(grace: Grace, stable: stable::StableImage, boot: u32) -> Self {
        Self {
            clients: HashMap::new(),
            sessions: HashMap::new(),
            sids: HashMap::new(),
            open_owners: HashMap::new(),
            lock_owners: HashMap::new(),
            gone: HashMap::new(),
            grace,
            stable,
            to_recall: Vec::new(),
            to_probe: Vec::new(),
            closed: HashMap::new(),
            next: 1,
            // The start in the sub-owners keeps two jobs' apart.
            next_sub: SUB_BASE | (u64::from(boot & 0xffff) << 32) | 1,
        }
    }

    /// A new number.
    pub fn next_id(&mut self) -> u64 {
        let n = self.next;
        self.next += 1;
        n
    }

    /// A new lock sub-owner.
    pub fn next_sub(&mut self) -> u64 {
        let n = self.next_sub;
        self.next_sub += 1;
        n
    }

    /// The lock owner holding sub-owner `sub`.
    #[must_use]
    pub fn owner_of_sub(&self, sub: u64) -> Option<&OwnerKey> {
        self.lock_owners
            .iter()
            .find(|(_, o)| o.sub == sub)
            .map(|(k, _)| k)
    }

    /// Whether client `id` holds any state.
    #[must_use]
    pub fn holds_state(&self, id: u64) -> bool {
        self.sids.values().any(|s| s.client == id)
    }

    /// Whether a grace period runs at `now`.
    #[must_use]
    pub fn in_grace(&self, now: Instant) -> bool {
        self.grace.until.is_some_and(|u| now < u)
    }

    /// Whether the owner `owner` may reclaim.
    #[must_use]
    pub fn may_reclaim(&self, owner: &[u8]) -> bool {
        self.grace.reclaimers.iter().any(|(o, _)| o == owner)
    }
}

/// The anonymous stateid.
#[must_use]
pub fn is_anon(s: &stateid4) -> bool {
    s.seqid == 0 && s.other == [0; 12]
}

/// The READ bypass stateid.
#[must_use]
pub fn is_bypass(s: &stateid4) -> bool {
    s.seqid == u32::MAX && s.other == [0xff; 12]
}

/// 4.1's current stateid.
#[must_use]
pub fn is_current(s: &stateid4) -> bool {
    s.seqid == 1 && s.other == [0; 12]
}

/// 4.1's invalid stateid.
#[must_use]
pub fn is_invalid(s: &stateid4) -> bool {
    s.seqid == u32::MAX && s.other == [0; 12]
}

/// Whether a stateid is one of the special ones.
#[must_use]
pub fn is_special(s: &stateid4) -> bool {
    s.other == [0; 12] || s.other == [0xff; 12]
}

/// The invalid stateid.
#[must_use]
pub fn invalid() -> stateid4 {
    stateid4 {
        seqid: u32::MAX,
        other: [0; 12],
    }
}

/// A stateid's `other` of start `boot`.
#[must_use]
pub fn other(boot: u32, id: u64) -> [u8; 12] {
    let mut o = [0u8; 12];
    o[..4].copy_from_slice(&boot.to_be_bytes());
    o[4..].copy_from_slice(&id.to_be_bytes());
    o
}

/// A stateid's start and number.
#[must_use]
pub fn split_other(o: &[u8; 12]) -> (u32, u64) {
    let b = u32::from_be_bytes([o[0], o[1], o[2], o[3]]);
    let mut n = [0u8; 8];
    n.copy_from_slice(&o[4..]);
    (b, u64::from_be_bytes(n))
}

/// A client ID of start `boot`.
#[must_use]
pub fn client_id(boot: u32, n: u64) -> u64 {
    (u64::from(boot) << 32) | (n & 0xffff_ffff)
}

/// The start of a client ID.
#[must_use]
pub fn client_boot(id: u64) -> u32 {
    u32::try_from(id >> 32).unwrap_or(0)
}

/// Whether a seqid-carrying operation's error leaves the seqid where it
/// is (RFC 7530 section 9.1.7).
#[must_use]
pub fn keeps_seqid(s: nfsstat4) -> bool {
    use nfsstat4 as S;
    matches!(
        s,
        S::NFS4ERR_STALE_CLIENTID
            | S::NFS4ERR_STALE_STATEID
            | S::NFS4ERR_BAD_STATEID
            | S::NFS4ERR_BAD_SEQID
            | S::NFS4ERR_BADXDR
            | S::NFS4ERR_RESOURCE
            | S::NFS4ERR_NOFILEHANDLE
            | S::NFS4ERR_MOVED
    )
}

/// Adds `[start, end)` to `ranges`.
pub fn add_range(ranges: &mut Vec<(u64, u64)>, start: u64, end: u64) {
    let mut s = start;
    let mut e = end;
    ranges.retain(|&(a, b)| {
        if b < s || a > e {
            true
        } else {
            s = s.min(a);
            e = e.max(b);
            false
        }
    });
    ranges.push((s, e));
    ranges.sort_unstable();
}

/// Takes `[start, end)` out of `ranges`.
pub fn sub_range(ranges: &mut Vec<(u64, u64)>, start: u64, end: u64) {
    let mut out = Vec::new();
    for &(a, b) in ranges.iter() {
        if b <= start || a >= end {
            out.push((a, b));
            continue;
        }
        if a < start {
            out.push((a, start));
        }
        if b > end {
            out.push((end, b));
        }
    }
    *ranges = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_split() {
        let mut r = Vec::new();
        add_range(&mut r, 0, 10);
        add_range(&mut r, 20, 30);
        add_range(&mut r, 5, 22);
        assert_eq!(r, vec![(0, 30)]);
        sub_range(&mut r, 10, 15);
        assert_eq!(r, vec![(0, 10), (15, 30)]);
        sub_range(&mut r, 0, u64::MAX);
        assert!(r.is_empty());
        add_range(&mut r, 100, u64::MAX);
        sub_range(&mut r, 200, u64::MAX);
        assert_eq!(r, vec![(100, 200)]);
    }

    #[test]
    fn identifiers() {
        let o = other(7, 99);
        assert_eq!(split_other(&o), (7, 99));
        assert_eq!(client_boot(client_id(9, 3)), 9);
        assert!(is_anon(&stateid4 {
            seqid: 0,
            other: [0; 12]
        }));
        assert!(is_bypass(&stateid4 {
            seqid: u32::MAX,
            other: [0xff; 12]
        }));
        assert!(is_current(&stateid4 {
            seqid: 1,
            other: [0; 12]
        }));
    }
}

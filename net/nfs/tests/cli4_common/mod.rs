// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A scripted NFSv4 responder for the client's tests until the server
//! (NFS-6) exists: an in-memory tree under a pseudo root (`/home/exp`),
//! client IDs for 4.0 and 4.1, one session per client with checked
//! slot sequence IDs, opens, byte-range locks, a grace period after a
//! restart in which only reclaims are taken, and injected `DELAY` and
//! `BADSESSION`. Enough for the client's own behaviour; the real
//! server's semantics are NFS-6's tests.

#![allow(dead_code, clippy::too_many_lines)]

use std::collections::{BTreeMap, HashMap};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use nfs::client::v4::attr::{self, Attrs};
use nfs::rpc::{Answer, CallContext, Dispatcher, Program, decode_args, encode_res, serve_tcp};
use nfs::types::nfs4::NFS4_PROGRAM::NFS_V4::{Args, Res};
use nfs::types::nfs4::*;
use nfs::xdr::Decoder;

/// One object.
#[derive(Debug, Clone)]
pub struct Node {
    pub dir: bool,
    pub data: Vec<u8>,
    pub kids: BTreeMap<Vec<u8>, u64>,
    pub mode: u32,
    pub change: u64,
}

/// Counts the tests look at.
#[derive(Debug, Default, Clone)]
pub struct Seen {
    pub exchange_id: u32,
    pub create_session: u32,
    pub setclientid: u32,
    pub reclaim_opens: u32,
    pub reclaim_locks: u32,
    pub reclaim_complete: u32,
    pub sequence_only: u32,
    pub renew: u32,
    pub delays: u32,
    pub seq_misordered: u32,
}

#[derive(Debug, Clone)]
struct OpenSt {
    fh: u64,
    client: u64,
    owner: Vec<u8>,
}

#[derive(Debug, Clone)]
struct LockSt {
    fh: u64,
    client: u64,
    owner: Vec<u8>,
    write: bool,
    off: u64,
    end: u64,
}

#[derive(Default)]
struct St {
    nodes: HashMap<u64, Node>,
    next: u64,
    /// Client IDs: confirmed or not.
    clients: HashMap<u64, bool>,
    next_client: u64,
    /// Session to (client, slot sequence IDs).
    sessions: HashMap<[u8; 16], (u64, Vec<u32>)>,
    opens: HashMap<[u8; 12], OpenSt>,
    locks: Vec<LockSt>,
    next_state: u64,
    grace_until: Option<Instant>,
    fail_seq: Vec<nfsstat4>,
    fail_op: Vec<nfsstat4>,
    seen: Seen,
    boot: u64,
}

/// The responder.
pub struct Fake4 {
    st: Mutex<St>,
    pub minors: Vec<u32>,
}

fn ok_attrs(n: &Node, id: u64, want: &[u32]) -> fattr4 {
    let has = |x: u32| want.contains(&x);
    let mut a = Attrs::default();
    if has(FATTR4_TYPE) {
        a.ftype = Some(if n.dir { 2 } else { 1 });
    }
    if has(FATTR4_CHANGE) {
        a.change = Some(n.change);
    }
    if has(FATTR4_SIZE) {
        a.size = Some(n.data.len() as u64);
    }
    if has(FATTR4_FILEID) {
        a.fileid = Some(id);
    }
    if has(FATTR4_MODE) {
        a.mode = Some(n.mode);
    }
    if has(FATTR4_NUMLINKS) {
        a.numlinks = Some(1);
    }
    if has(FATTR4_OWNER) {
        a.owner = Some(b"100".to_vec());
    }
    if has(FATTR4_OWNER_GROUP) {
        a.group = Some(b"100".to_vec());
    }
    if has(FATTR4_TIME_METADATA) {
        a.ctime = Some(i64::try_from(n.change).unwrap_or(0) * 1000);
    }
    if has(FATTR4_TIME_MODIFY) {
        a.mtime = Some(i64::try_from(n.change).unwrap_or(0) * 1000);
    }
    if has(FATTR4_LEASE_TIME) {
        a.lease_time = Some(2);
    }
    if has(FATTR4_MAXREAD) {
        a.maxread = Some(65536);
    }
    if has(FATTR4_MAXWRITE) {
        a.maxwrite = Some(65536);
    }
    if has(FATTR4_CASE_INSENSITIVE) {
        a.case_insensitive = Some(false);
    }
    if has(FATTR4_FILEHANDLE) {
        a.filehandle = Some(id.to_be_bytes().to_vec());
    }
    if has(FATTR4_RDATTR_ERROR) {
        a.rdattr_error = Some(0);
    }
    attr::encode(&a)
}

fn fh_of(f: &nfs_fh4) -> Option<u64> {
    <[u8; 8]>::try_from(f.0.as_slice())
        .ok()
        .map(u64::from_be_bytes)
}

impl Fake4 {
    /// A server of minor versions `minors`, exporting `/home/exp`.
    pub fn new(minors: &[u32]) -> Arc<Self> {
        let mut st = St {
            next: 10,
            next_client: 1,
            boot: 1,
            ..St::default()
        };
        let dir = |kids: &[(&[u8], u64)]| Node {
            dir: true,
            data: Vec::new(),
            kids: kids.iter().map(|(k, v)| (k.to_vec(), *v)).collect(),
            mode: 0o755,
            change: 1,
        };
        st.nodes.insert(1, dir(&[(b"home", 2)]));
        st.nodes.insert(2, dir(&[(b"exp", 3)]));
        st.nodes.insert(3, dir(&[]));
        Arc::new(Self {
            st: Mutex::new(st),
            minors: minors.to_vec(),
        })
    }

    fn st(&self) -> MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the server saw.
    pub fn seen(&self) -> Seen {
        self.st().seen.clone()
    }

    /// The next `SEQUENCE` fails with `s`.
    pub fn fail_sequence(&self, s: nfsstat4) {
        self.st().fail_seq.push(s);
    }

    /// The next compound's first operation after `SEQUENCE` fails with `s`.
    pub fn fail_op(&self, s: nfsstat4) {
        self.st().fail_op.push(s);
    }

    /// The server restarts: client IDs, sessions, opens and locks are
    /// gone; a grace period of `grace` begins. Files stay.
    pub fn restart(&self, grace: Duration) {
        let mut st = self.st();
        st.clients.clear();
        st.sessions.clear();
        st.locks.clear();
        // Opens are remembered only to be reclaimed (stable storage).
        st.boot += 1;
        st.grace_until = Some(Instant::now() + grace);
    }

    /// A file's data, by path under the export.
    pub fn file(&self, name: &str) -> Option<Vec<u8>> {
        let st = self.st();
        let id = *st.nodes.get(&3)?.kids.get(name.as_bytes())?;
        Some(st.nodes.get(&id)?.data.clone())
    }

    /// Locks held now.
    pub fn locks(&self) -> usize {
        self.st().locks.len()
    }

    /// Serve on loopback: the address, and the flag that stops it.
    pub fn serve(self: &Arc<Self>) -> (SocketAddr, Arc<AtomicBool>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let addr = l.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = Arc::clone(&stop);
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            let mut d = Dispatcher::new();
            d.add(me);
            let d = Arc::new(d);
            while !s2.load(Ordering::Relaxed) {
                match l.accept() {
                    Ok((c, _)) => {
                        c.set_nonblocking(false).unwrap();
                        let d = Arc::clone(&d);
                        std::thread::spawn(move || {
                            let _ = serve_tcp(&c, &d, 2 << 20);
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        (addr, stop)
    }

    fn in_grace(st: &St) -> bool {
        st.grace_until.is_some_and(|t| Instant::now() < t)
    }

    fn new_state(st: &mut St) -> stateid4 {
        st.next_state += 1;
        let mut other = [0u8; 12];
        other[..8].copy_from_slice(&st.next_state.to_be_bytes());
        other[8..].copy_from_slice(&u32::try_from(st.boot).unwrap_or(0).to_be_bytes());
        stateid4 { seqid: 1, other }
    }

    fn compound(&self, a: COMPOUND4args) -> COMPOUND4res {
        let mut st = self.st();
        let mut out = Vec::new();
        let mut status = nfsstat4::NFS4_OK;
        if !self.minors.contains(&a.minorversion) {
            return COMPOUND4res {
                status: nfsstat4::NFS4ERR_MINOR_VERS_MISMATCH,
                tag: a.tag,
                resarray: Vec::new(),
            };
        }
        let only_seq = a.argarray.len() == 1;
        let (mut cur, mut saved): (Option<u64>, Option<u64>) = (None, None);
        let mut client: Option<u64> = None;
        let mut first_op = true;
        for op in a.argarray {
            let is_seq = matches!(op, nfs_argop4::OP_SEQUENCE(_));
            if !is_seq && first_op {
                first_op = false;
                if let Some(s) = st.fail_op.pop() {
                    if s == nfsstat4::NFS4ERR_DELAY {
                        st.seen.delays += 1;
                    }
                    status = s;
                    out.push(nfs_resop4::OP_PUTROOTFH(PUTROOTFH4res { status: s }));
                    break;
                }
            }
            let r = self.op(&mut st, op, &mut cur, &mut saved, &mut client, only_seq);
            let s = status_of(&r);
            out.push(r);
            if s != nfsstat4::NFS4_OK {
                status = s;
                break;
            }
        }
        COMPOUND4res {
            status,
            tag: a.tag,
            resarray: out,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn op(
        &self,
        st: &mut St,
        op: nfs_argop4,
        cur: &mut Option<u64>,
        saved: &mut Option<u64>,
        client: &mut Option<u64>,
        only_seq: bool,
    ) -> nfs_resop4 {
        use nfs_resop4 as R;
        use nfsstat4 as S;
        match op {
            nfs_argop4::OP_EXCHANGE_ID(_) => {
                st.seen.exchange_id += 1;
                let id = st.next_client;
                st.next_client += 1;
                st.clients.insert(id, false);
                R::OP_EXCHANGE_ID(EXCHANGE_ID4res::NFS4_OK(EXCHANGE_ID4resok {
                    eir_clientid: id,
                    eir_sequenceid: 1,
                    eir_flags: EXCHGID4_FLAG_USE_NON_PNFS,
                    eir_state_protect: state_protect4_r::SP4_NONE,
                    eir_server_owner: server_owner4 {
                        so_minor_id: 0,
                        so_major_id: b"fake".to_vec(),
                    },
                    eir_server_scope: b"fake".to_vec(),
                    eir_server_impl_id: Vec::new(),
                }))
            }
            nfs_argop4::OP_CREATE_SESSION(c) => {
                st.seen.create_session += 1;
                if !st.clients.contains_key(&c.csa_clientid) {
                    return R::OP_CREATE_SESSION(CREATE_SESSION4res::default(
                        S::NFS4ERR_STALE_CLIENTID,
                    ));
                }
                st.clients.insert(c.csa_clientid, true);
                let mut sid = [0u8; 16];
                sid[..8].copy_from_slice(&c.csa_clientid.to_be_bytes());
                sid[8..].copy_from_slice(&u64::from(st.seen.create_session).to_be_bytes());
                let n = c.csa_fore_chan_attrs.ca_maxrequests.min(4);
                st.sessions
                    .insert(sid, (c.csa_clientid, vec![1; n as usize]));
                let mut fore = c.csa_fore_chan_attrs.clone();
                fore.ca_maxrequests = n;
                R::OP_CREATE_SESSION(CREATE_SESSION4res::NFS4_OK(CREATE_SESSION4resok {
                    csr_sessionid: sid,
                    csr_sequence: c.csa_sequence,
                    csr_flags: 0,
                    csr_fore_chan_attrs: fore,
                    csr_back_chan_attrs: c.csa_back_chan_attrs,
                }))
            }
            nfs_argop4::OP_SEQUENCE(s) => {
                if let Some(f) = st.fail_seq.pop() {
                    return R::OP_SEQUENCE(SEQUENCE4res::default(f));
                }
                let Some((cid, slots)) = st.sessions.get_mut(&s.sa_sessionid) else {
                    return R::OP_SEQUENCE(SEQUENCE4res::default(S::NFS4ERR_BADSESSION));
                };
                let Some(want) = slots.get_mut(s.sa_slotid as usize) else {
                    return R::OP_SEQUENCE(SEQUENCE4res::default(S::NFS4ERR_BADSLOT));
                };
                if s.sa_sequenceid != *want {
                    st.seen.seq_misordered += 1;
                    return R::OP_SEQUENCE(SEQUENCE4res::default(S::NFS4ERR_SEQ_MISORDERED));
                }
                *want += 1;
                let n = u32::try_from(slots.len()).unwrap_or(1);
                *client = Some(*cid);
                if only_seq {
                    st.seen.sequence_only += 1;
                }
                R::OP_SEQUENCE(SEQUENCE4res::NFS4_OK(SEQUENCE4resok {
                    sr_sessionid: s.sa_sessionid,
                    sr_sequenceid: s.sa_sequenceid,
                    sr_slotid: s.sa_slotid,
                    sr_highest_slotid: n - 1,
                    sr_target_highest_slotid: n - 1,
                    sr_status_flags: 0,
                }))
            }
            nfs_argop4::OP_SETCLIENTID(_) => {
                st.seen.setclientid += 1;
                let id = st.next_client;
                st.next_client += 1;
                st.clients.insert(id, false);
                R::OP_SETCLIENTID(SETCLIENTID4res::NFS4_OK(SETCLIENTID4resok {
                    clientid: id,
                    setclientid_confirm: id.to_be_bytes(),
                }))
            }
            nfs_argop4::OP_SETCLIENTID_CONFIRM(c) => {
                let s = if st.clients.contains_key(&c.clientid) {
                    st.clients.insert(c.clientid, true);
                    S::NFS4_OK
                } else {
                    S::NFS4ERR_STALE_CLIENTID
                };
                R::OP_SETCLIENTID_CONFIRM(SETCLIENTID_CONFIRM4res { status: s })
            }
            nfs_argop4::OP_RENEW(r) => {
                st.seen.renew += 1;
                let s = if st.clients.contains_key(&r.clientid) {
                    S::NFS4_OK
                } else {
                    S::NFS4ERR_STALE_CLIENTID
                };
                R::OP_RENEW(RENEW4res { status: s })
            }
            nfs_argop4::OP_RECLAIM_COMPLETE(_) => {
                st.seen.reclaim_complete += 1;
                R::OP_RECLAIM_COMPLETE(RECLAIM_COMPLETE4res {
                    rcr_status: S::NFS4_OK,
                })
            }
            nfs_argop4::OP_DESTROY_SESSION(d) => {
                st.sessions.remove(&d.dsa_sessionid);
                R::OP_DESTROY_SESSION(DESTROY_SESSION4res {
                    dsr_status: S::NFS4_OK,
                })
            }
            nfs_argop4::OP_DESTROY_CLIENTID(d) => {
                st.clients.remove(&d.dca_clientid);
                R::OP_DESTROY_CLIENTID(DESTROY_CLIENTID4res {
                    dcr_status: S::NFS4_OK,
                })
            }
            nfs_argop4::OP_PUTROOTFH => {
                *cur = Some(1);
                R::OP_PUTROOTFH(PUTROOTFH4res { status: S::NFS4_OK })
            }
            nfs_argop4::OP_PUTFH(p) => {
                match fh_of(&p.object).filter(|f| st.nodes.contains_key(f)) {
                    Some(f) => {
                        *cur = Some(f);
                        R::OP_PUTFH(PUTFH4res { status: S::NFS4_OK })
                    }
                    None => R::OP_PUTFH(PUTFH4res {
                        status: S::NFS4ERR_STALE,
                    }),
                }
            }
            nfs_argop4::OP_SAVEFH => {
                *saved = *cur;
                R::OP_SAVEFH(SAVEFH4res { status: S::NFS4_OK })
            }
            nfs_argop4::OP_LOOKUP(l) => {
                let k = cur
                    .and_then(|c| st.nodes.get(&c))
                    .and_then(|n| n.kids.get(&l.objname.0).copied());
                match k {
                    Some(k) => {
                        *cur = Some(k);
                        R::OP_LOOKUP(LOOKUP4res { status: S::NFS4_OK })
                    }
                    None => R::OP_LOOKUP(LOOKUP4res {
                        status: S::NFS4ERR_NOENT,
                    }),
                }
            }
            nfs_argop4::OP_GETFH => R::OP_GETFH(GETFH4res::NFS4_OK(GETFH4resok {
                object: nfs_fh4(cur.unwrap_or(0).to_be_bytes().to_vec()),
            })),
            nfs_argop4::OP_GETATTR(g) => {
                let id = cur.unwrap_or(0);
                match st.nodes.get(&id) {
                    Some(n) => R::OP_GETATTR(GETATTR4res::NFS4_OK(GETATTR4resok {
                        obj_attributes: ok_attrs(n, id, &attr::numbers(&g.attr_request)),
                    })),
                    None => R::OP_GETATTR(GETATTR4res::default(S::NFS4ERR_STALE)),
                }
            }
            nfs_argop4::OP_ACCESS(a) => R::OP_ACCESS(ACCESS4res::NFS4_OK(ACCESS4resok {
                supported: a.access,
                access: a.access,
            })),
            nfs_argop4::OP_OPEN(o) => {
                let reclaim = matches!(o.claim, open_claim4::CLAIM_PREVIOUS(_));
                if Self::in_grace(st) && !reclaim {
                    return R::OP_OPEN(OPEN4res::default(S::NFS4ERR_GRACE));
                }
                if reclaim {
                    st.seen.reclaim_opens += 1;
                }
                let dir = cur.unwrap_or(0);
                let target = match &o.claim {
                    open_claim4::CLAIM_NULL(n) => {
                        let existing = st.nodes.get(&dir).and_then(|d| d.kids.get(&n.0).copied());
                        match (&o.openhow, existing) {
                            (openflag4::OPEN4_CREATE(createhow4::GUARDED4(_)), Some(_)) => {
                                return R::OP_OPEN(OPEN4res::default(S::NFS4ERR_EXIST));
                            }
                            (openflag4::OPEN4_CREATE(_), None) => {
                                st.next += 1;
                                let id = st.next;
                                st.nodes.insert(
                                    id,
                                    Node {
                                        dir: false,
                                        data: Vec::new(),
                                        kids: BTreeMap::new(),
                                        mode: 0o644,
                                        change: 1,
                                    },
                                );
                                if let Some(d) = st.nodes.get_mut(&dir) {
                                    d.kids.insert(n.0.clone(), id);
                                    d.change += 1;
                                }
                                id
                            }
                            (_, Some(id)) => id,
                            (_, None) => return R::OP_OPEN(OPEN4res::default(S::NFS4ERR_NOENT)),
                        }
                    }
                    _ => dir,
                };
                *cur = Some(target);
                let sid = Self::new_state(st);
                st.opens.insert(
                    sid.other,
                    OpenSt {
                        fh: target,
                        client: o.owner.clientid,
                        owner: o.owner.owner.clone(),
                    },
                );
                // 4.0: a new owner confirms.
                let confirm = if client.is_none() {
                    OPEN4_RESULT_CONFIRM
                } else {
                    0
                };
                R::OP_OPEN(OPEN4res::NFS4_OK(OPEN4resok {
                    stateid: sid,
                    cinfo: change_info4 {
                        atomic: true,
                        before: 1,
                        after: 2,
                    },
                    rflags: confirm,
                    attrset: bitmap4(Vec::new()),
                    delegation: open_delegation4::OPEN_DELEGATE_NONE,
                }))
            }
            nfs_argop4::OP_OPEN_CONFIRM(c) => {
                R::OP_OPEN_CONFIRM(OPEN_CONFIRM4res::NFS4_OK(OPEN_CONFIRM4resok {
                    open_stateid: stateid4 {
                        seqid: c.open_stateid.seqid + 1,
                        other: c.open_stateid.other,
                    },
                }))
            }
            nfs_argop4::OP_CLOSE(c) => {
                st.opens.remove(&c.open_stateid.other);
                R::OP_CLOSE(CLOSE4res::NFS4_OK(c.open_stateid))
            }
            nfs_argop4::OP_READ(r) => {
                let n = st.nodes.get(&cur.unwrap_or(0)).cloned();
                match n {
                    Some(n) => {
                        let s = usize::try_from(r.offset)
                            .unwrap_or(usize::MAX)
                            .min(n.data.len());
                        let e = (s + r.count as usize).min(n.data.len());
                        R::OP_READ(READ4res::NFS4_OK(READ4resok {
                            eof: e == n.data.len(),
                            data: n.data[s..e].to_vec(),
                        }))
                    }
                    None => R::OP_READ(READ4res::default(S::NFS4ERR_STALE)),
                }
            }
            nfs_argop4::OP_WRITE(w) => {
                if Self::in_grace(st) && w.stateid.other == [0; 12] {
                    return R::OP_WRITE(WRITE4res::default(S::NFS4ERR_GRACE));
                }
                let boot = st.boot;
                let Some(n) = st.nodes.get_mut(&cur.unwrap_or(0)) else {
                    return R::OP_WRITE(WRITE4res::default(S::NFS4ERR_STALE));
                };
                let off = usize::try_from(w.offset).unwrap_or(0);
                if n.data.len() < off + w.data.len() {
                    n.data.resize(off + w.data.len(), 0);
                }
                n.data[off..off + w.data.len()].copy_from_slice(&w.data);
                n.change += 1;
                R::OP_WRITE(WRITE4res::NFS4_OK(WRITE4resok {
                    count: u32::try_from(w.data.len()).unwrap_or(0),
                    committed: stable_how4::FILE_SYNC4,
                    writeverf: boot.to_be_bytes(),
                }))
            }
            nfs_argop4::OP_COMMIT(_) => R::OP_COMMIT(COMMIT4res::NFS4_OK(COMMIT4resok {
                writeverf: st.boot.to_be_bytes(),
            })),
            nfs_argop4::OP_CREATE(c) => {
                st.next += 1;
                let id = st.next;
                let dir = cur.unwrap_or(0);
                if st
                    .nodes
                    .get(&dir)
                    .is_some_and(|d| d.kids.contains_key(&c.objname.0))
                {
                    return R::OP_CREATE(CREATE4res::default(S::NFS4ERR_EXIST));
                }
                st.nodes.insert(
                    id,
                    Node {
                        dir: matches!(c.objtype, createtype4::NF4DIR),
                        data: Vec::new(),
                        kids: BTreeMap::new(),
                        mode: 0o755,
                        change: 1,
                    },
                );
                if let Some(d) = st.nodes.get_mut(&dir) {
                    d.kids.insert(c.objname.0.clone(), id);
                    d.change += 1;
                }
                *cur = Some(id);
                R::OP_CREATE(CREATE4res::NFS4_OK(CREATE4resok {
                    cinfo: change_info4 {
                        atomic: true,
                        before: 1,
                        after: 2,
                    },
                    attrset: bitmap4(Vec::new()),
                }))
            }
            nfs_argop4::OP_REMOVE(r) => {
                let dir = cur.unwrap_or(0);
                match st
                    .nodes
                    .get_mut(&dir)
                    .and_then(|d| d.kids.remove(&r.target.0))
                {
                    Some(id) => {
                        st.nodes.remove(&id);
                        R::OP_REMOVE(REMOVE4res::NFS4_OK(REMOVE4resok {
                            cinfo: change_info4 {
                                atomic: true,
                                before: 1,
                                after: 2,
                            },
                        }))
                    }
                    None => R::OP_REMOVE(REMOVE4res::default(S::NFS4ERR_NOENT)),
                }
            }
            nfs_argop4::OP_RENAME(r) => {
                let (from, to) = (saved.unwrap_or(0), cur.unwrap_or(0));
                match st
                    .nodes
                    .get_mut(&from)
                    .and_then(|d| d.kids.remove(&r.oldname.0))
                {
                    Some(id) => {
                        if let Some(d) = st.nodes.get_mut(&to) {
                            d.kids.insert(r.newname.0.clone(), id);
                        }
                        let ci = change_info4 {
                            atomic: true,
                            before: 1,
                            after: 2,
                        };
                        R::OP_RENAME(RENAME4res::NFS4_OK(RENAME4resok {
                            source_cinfo: ci.clone(),
                            target_cinfo: ci,
                        }))
                    }
                    None => R::OP_RENAME(RENAME4res::default(S::NFS4ERR_NOENT)),
                }
            }
            nfs_argop4::OP_READDIR(r) => {
                let dir = st.nodes.get(&cur.unwrap_or(0)).cloned();
                let Some(d) = dir else {
                    return R::OP_READDIR(READDIR4res::default(S::NFS4ERR_STALE));
                };
                let want = attr::numbers(&r.attr_request);
                let all: Vec<(Vec<u8>, u64)> = d.kids.into_iter().collect();
                let start = usize::try_from(r.cookie.saturating_sub(2)).unwrap_or(0);
                let page: Vec<entry4> = all
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(50)
                    .map(|(i, (n, id))| entry4 {
                        cookie: i as u64 + 3,
                        name: utf8string(n.clone()),
                        attrs: ok_attrs(&st.nodes[id], *id, &want),
                    })
                    .collect();
                let eof = start + page.len() >= all.len();
                R::OP_READDIR(READDIR4res::NFS4_OK(READDIR4resok {
                    cookieverf: [0; 8],
                    reply: dirlist4 { entries: page, eof },
                }))
            }
            nfs_argop4::OP_LOCK(l) => {
                if l.reclaim {
                    st.seen.reclaim_locks += 1;
                } else if Self::in_grace(st) {
                    return R::OP_LOCK(LOCK4res::default(S::NFS4ERR_GRACE));
                }
                let (client, owner) = match &l.locker {
                    locker4::TRUE(o) => (o.lock_owner.clientid, o.lock_owner.owner.clone()),
                    locker4::FALSE(e) => {
                        let found = st
                            .locks
                            .iter()
                            .find(|x| x.fh == cur.unwrap_or(0))
                            .map(|x| (x.client, x.owner.clone()));
                        let _ = e;
                        found.unwrap_or((0, Vec::new()))
                    }
                };
                let fh = cur.unwrap_or(0);
                let end = l.offset.saturating_add(l.length);
                let write = matches!(l.locktype, nfs_lock_type4::WRITE_LT);
                if let Some(x) = st.locks.iter().find(|x| {
                    x.fh == fh
                        && (x.owner != owner || x.client != client)
                        && x.off < end
                        && l.offset < x.end
                        && (x.write || write)
                }) {
                    return R::OP_LOCK(LOCK4res::NFS4ERR_DENIED(LOCK4denied {
                        offset: x.off,
                        length: x.end - x.off,
                        locktype: if x.write {
                            nfs_lock_type4::WRITE_LT
                        } else {
                            nfs_lock_type4::READ_LT
                        },
                        owner: state_owner4 {
                            clientid: x.client,
                            owner: x.owner.clone(),
                        },
                    }));
                }
                st.locks.push(LockSt {
                    fh,
                    client,
                    owner,
                    write,
                    off: l.offset,
                    end,
                });
                let sid = Self::new_state(st);
                R::OP_LOCK(LOCK4res::NFS4_OK(LOCK4resok { lock_stateid: sid }))
            }
            nfs_argop4::OP_LOCKU(u) => {
                let fh = cur.unwrap_or(0);
                let end = u.offset.saturating_add(u.length);
                st.locks
                    .retain(|x| !(x.fh == fh && x.off >= u.offset && x.end <= end));
                R::OP_LOCKU(LOCKU4res::NFS4_OK(u.lock_stateid))
            }
            nfs_argop4::OP_LOCKT(t) => {
                let fh = cur.unwrap_or(0);
                let end = t.offset.saturating_add(t.length);
                match st.locks.iter().find(|x| {
                    x.fh == fh
                        && (x.owner != t.owner.owner || x.client != t.owner.clientid)
                        && x.off < end
                        && t.offset < x.end
                }) {
                    Some(x) => R::OP_LOCKT(LOCKT4res::NFS4ERR_DENIED(LOCK4denied {
                        offset: x.off,
                        length: x.end - x.off,
                        locktype: nfs_lock_type4::WRITE_LT,
                        owner: state_owner4 {
                            clientid: x.client,
                            owner: x.owner.clone(),
                        },
                    })),
                    None => R::OP_LOCKT(LOCKT4res::NFS4_OK),
                }
            }
            nfs_argop4::OP_SETATTR(s) => {
                let a = attr::decode(&s.obj_attributes).ok();
                if let (Some(n), Some(a)) = (st.nodes.get_mut(&cur.unwrap_or(0)), a) {
                    if let Some(sz) = a.size {
                        n.data.resize(usize::try_from(sz).unwrap_or(0), 0);
                    }
                    if let Some(m) = a.mode {
                        n.mode = m;
                    }
                    n.change += 1;
                }
                R::OP_SETATTR(SETATTR4res {
                    status: S::NFS4_OK,
                    attrsset: s.obj_attributes.attrmask,
                })
            }
            _ => R::OP_ILLEGAL(ILLEGAL4res {
                status: S::NFS4ERR_NOTSUPP,
            }),
        }
    }
}

/// An operation's status.
pub fn status_of(r: &nfs_resop4) -> nfsstat4 {
    use nfs_resop4 as R;
    use nfsstat4 as S;
    match r {
        R::OP_EXCHANGE_ID(EXCHANGE_ID4res::default(s))
        | R::OP_CREATE_SESSION(CREATE_SESSION4res::default(s))
        | R::OP_SEQUENCE(SEQUENCE4res::default(s))
        | R::OP_SETCLIENTID(SETCLIENTID4res::default(s))
        | R::OP_GETATTR(GETATTR4res::default(s))
        | R::OP_OPEN(OPEN4res::default(s))
        | R::OP_READ(READ4res::default(s))
        | R::OP_WRITE(WRITE4res::default(s))
        | R::OP_CREATE(CREATE4res::default(s))
        | R::OP_REMOVE(REMOVE4res::default(s))
        | R::OP_RENAME(RENAME4res::default(s))
        | R::OP_READDIR(READDIR4res::default(s))
        | R::OP_LOCK(LOCK4res::default(s)) => *s,
        R::OP_SETCLIENTID_CONFIRM(x) => x.status,
        R::OP_RENEW(x) => x.status,
        R::OP_PUTFH(x) => x.status,
        R::OP_PUTROOTFH(x) => x.status,
        R::OP_LOOKUP(x) => x.status,
        R::OP_SETATTR(x) => x.status,
        R::OP_ILLEGAL(x) => x.status,
        R::OP_LOCK(LOCK4res::NFS4ERR_DENIED(_)) | R::OP_LOCKT(LOCKT4res::NFS4ERR_DENIED(_)) => {
            S::NFS4ERR_DENIED
        }
        _ => S::NFS4_OK,
    }
}

impl Program for Fake4 {
    fn program(&self) -> u32 {
        100_003
    }
    fn versions(&self) -> &[u32] {
        &[4]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        match decode_args::<Args>(ctx, d)? {
            Args::NFSPROC4_NULL => encode_res(&Res::NFSPROC4_NULL),
            Args::NFSPROC4_COMPOUND(a) => encode_res(&Res::NFSPROC4_COMPOUND(self.compound(a))),
        }
    }
}

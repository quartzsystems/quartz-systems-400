// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Server`]: what every connection thread of a `QNFSNFSD` job shares
//! (behind an `Arc`): the export table, the state tables, the start, the
//! clock, the identity registry, the stable records and the callback
//! connector. [`Server::background`] is the job's one extra thread: the
//! file system's lease breaks become `CB_RECALL`s, leases run out,
//! recalls not answered are revoked, the grace period ends.
//!
//! **Courtesy.** A client whose lease ran out keeps its state while
//! nothing needs it: a request it sends renews the lease and finds
//! everything as it was. Its state goes when another client's open,
//! lock or delegation conflicts with it (found by the request that
//! conflicts, which then goes on), or at the latest
//! [`Config::courtesy`] after the lease ran out. A local job in the
//! way of such state waits for that bound (the IFS cannot ask this job).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use super::cb::{self, CbConnector};
use super::state::stable::{StableClient, StableImage, StableStore};
use super::state::{CbPath, Gone, Grace, Kind, Tables, other};
use crate::registry::Registry;
use crate::rpc::Credential;
use crate::server::export::ExportTable;
use crate::server::instance::{Clock, Instance};
use crate::server::news::NewsSource;
use crate::server::vfs::Vfs;
use crate::types::nfs4::{SEQ4_STATUS_RECALLABLE_STATE_REVOKED, stateid4};

/// The server's settings.
#[derive(Debug, Clone)]
pub struct Config {
    /// The lease (`lease_time`).
    pub lease: Duration,
    /// The grace period after a start.
    pub grace: Duration,
    /// How long a lapsed client's state is kept while nothing needs it.
    pub courtesy: Duration,
    /// How long a recalled delegation may stay out.
    pub recall_wait: Duration,
    /// How long a callback is waited for.
    pub cb_timeout: Duration,
    /// `so_major_id` and `eir_server_scope`: the system's name.
    pub system: Vec<u8>,
    /// The implementation's name (`nii_name`).
    pub impl_name: String,
    /// Fore-channel slots offered at most.
    pub slots: u32,
    /// Operations in one compound at most.
    pub max_ops: u32,
    /// The largest `READ` and `WRITE`.
    pub max_io: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lease: Duration::from_secs(90),
            grace: Duration::from_secs(90),
            courtesy: Duration::from_secs(270),
            recall_wait: Duration::from_secs(90),
            cb_timeout: Duration::from_secs(5),
            system: b"QS400".to_vec(),
            impl_name: "QS/400 NFS".into(),
            slots: 64,
            max_ops: 64,
            max_io: 1 << 20,
        }
    }
}

/// What the file system must be told after state went.
#[derive(Debug, Default)]
pub(crate) struct Cleanup {
    pub closes: Vec<u64>,
    pub subs: Vec<u64>,
    pub leases: Vec<u64>,
}

impl Cleanup {
    pub(crate) fn run(self, vfs: &dyn Vfs) {
        for tag in self.leases {
            let _ = vfs.lease_report(tag, &[]);
        }
        for s in self.subs {
            let _ = vfs.release_sub(s);
        }
        for o in self.closes {
            let _ = vfs.close(o);
        }
    }
}

/// The `NFSv4` server one job runs.
pub struct Server {
    pub(crate) cfg: Config,
    pub(crate) exports: Arc<dyn ExportTable>,
    pub(crate) instance: Arc<Instance>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) registry: Arc<Registry>,
    stable: Arc<dyn StableStore>,
    pub(crate) cb: Arc<dyn CbConnector>,
    pub(crate) boot: u32,
    pub(crate) started: Instant,
    t: Mutex<Tables>,
    me: Weak<Server>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("boot", &self.boot)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// A start of the server. The stable records of the start before
    /// say which clients may reclaim: while any may, a grace period of
    /// [`Config::grace`] runs.
    pub fn new(
        cfg: Config,
        exports: Arc<dyn ExportTable>,
        instance: Arc<Instance>,
        clock: Arc<dyn Clock>,
        registry: Arc<Registry>,
        stable: Arc<dyn StableStore>,
        cb: Arc<dyn CbConnector>,
    ) -> Arc<Self> {
        let v = instance.verf();
        let mut boot = u32::from_be_bytes([v[0], v[1], v[2], v[3]])
            ^ u32::from_be_bytes([v[4], v[5], v[6], v[7]]).rotate_left(7);
        if boot == 0 {
            boot = 1;
        }
        let started = clock.now();
        let before = stable.load().unwrap_or_else(|e| {
            tracing::warn!("NFSv4 stable records not read: {e}");
            StableImage::default()
        });
        let reclaimers: Vec<(Vec<u8>, u32)> = before
            .clients
            .iter()
            .filter(|c| c.holds || (!before.grace_done && c.may_reclaim))
            .map(|c| (c.owner.clone(), u32::from(c.minor)))
            .collect();
        let until = (!reclaimers.is_empty()).then(|| started + cfg.grace);
        let img = StableImage {
            grace_done: until.is_none(),
            clients: reclaimers
                .iter()
                .map(|(o, m)| StableClient {
                    owner: o.clone(),
                    minor: u8::try_from(*m).unwrap_or(0),
                    holds: false,
                    may_reclaim: true,
                })
                .collect(),
        };
        if let Err(e) = stable.store(&img) {
            tracing::warn!("NFSv4 stable records not written: {e}");
        }
        let t = Tables::new(Grace { until, reclaimers }, img, boot);
        Arc::new_cyclic(|me| Self {
            cfg,
            exports,
            instance,
            clock,
            registry,
            stable,
            cb,
            boot,
            started,
            t: Mutex::new(t),
            me: me.clone(),
        })
    }

    /// The settings.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The registry.
    pub fn registry(&self) -> &Arc<Registry> {
        &self.registry
    }

    pub(crate) fn t(&self) -> MutexGuard<'_, Tables> {
        self.t.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn now(&self) -> Instant {
        self.clock.now()
    }

    /// Whether the grace period runs.
    pub fn in_grace(&self) -> bool {
        self.t().in_grace(self.now())
    }

    /// Clients known (confirmed or not).
    pub fn clients(&self) -> usize {
        self.t().clients.len()
    }

    /// Delegations held.
    pub fn delegations(&self) -> usize {
        self.t()
            .sids
            .values()
            .filter(|s| matches!(s.kind, Kind::Deleg(_)))
            .count()
    }

    /// The stateid of number `id` as the wire has it.
    pub(crate) fn wire_sid(&self, id: u64, seq: u32) -> stateid4 {
        stateid4 {
            seqid: seq,
            other: other(self.boot, id),
        }
    }

    pub(crate) fn store_stable(&self, t: &Tables) {
        if let Err(e) = self.stable.store(&t.stable) {
            tracing::warn!("NFSv4 stable records not written: {e}");
        }
    }

    /// Notes in the stable records that client `id` holds state.
    pub(crate) fn record_state(&self, t: &mut Tables, id: u64) {
        let Some(c) = t.clients.get_mut(&id) else {
            return;
        };
        if c.recorded {
            return;
        }
        c.recorded = true;
        let (owner, minor) = (c.owner.clone(), c.minor);
        match t.stable.clients.iter_mut().find(|s| s.owner == owner) {
            Some(s) => s.holds = true,
            None => t.stable.clients.push(StableClient {
                owner,
                minor: u8::try_from(minor).unwrap_or(0),
                holds: true,
                may_reclaim: false,
            }),
        }
        self.store_stable(t);
    }

    /// Ends the grace period when its time is up or every client that
    /// may reclaim has said it is done.
    pub(crate) fn check_grace(&self, t: &mut Tables, now: Instant) {
        let Some(until) = t.grace.until else { return };
        let done = t.grace.reclaimers.iter().all(|(o, _)| {
            t.clients
                .values()
                .any(|c| c.confirmed && &c.owner == o && c.reclaim_done)
        });
        if now >= until || done {
            t.grace.until = None;
            t.stable.grace_done = true;
            for c in &mut t.stable.clients {
                c.may_reclaim = false;
            }
            t.stable.clients.retain(|c| c.holds);
            self.store_stable(t);
        }
    }

    /// Takes client `id` and all its state away.
    pub(crate) fn drop_client(&self, t: &mut Tables, id: u64, why: Gone) -> Cleanup {
        let mut c = Cleanup::default();
        let sids: Vec<u64> = t
            .sids
            .values()
            .filter(|s| s.client == id)
            .map(|s| s.id)
            .collect();
        for s in sids {
            if let Some(sid) = t.sids.remove(&s) {
                match sid.kind {
                    Kind::Open(o) => c.closes.push(o.fd),
                    Kind::Deleg(d) => c.leases.push(d.tag),
                    Kind::Lock(_) => {}
                }
                t.gone.insert(s, (why, id));
            }
        }
        let subs: Vec<u64> = t
            .lock_owners
            .iter()
            .filter(|(k, _)| k.0 == id)
            .map(|(_, o)| o.sub)
            .collect();
        c.subs = subs;
        t.lock_owners.retain(|k, _| k.0 != id);
        t.open_owners.retain(|k, _| k.0 != id);
        t.sessions.retain(|_, s| s.client != id);
        t.to_recall.retain(|s| t.sids.contains_key(s));
        if let Some(cl) = t.clients.remove(&id)
            && cl.recorded
        {
            // Another record of the same owner may carry it on.
            if !t
                .clients
                .values()
                .any(|o| o.owner == cl.owner && o.recorded)
            {
                if let Some(s) = t.stable.clients.iter_mut().find(|s| s.owner == cl.owner) {
                    s.holds = false;
                }
                t.stable.clients.retain(|s| s.holds || s.may_reclaim);
                self.store_stable(t);
            }
        }
        if t.gone.len() > 65_536 {
            t.gone.clear();
        }
        c
    }

    /// Drops lapsed clients that hold opens or locks on the file of
    /// `fid` (another client needs it): whether any went.
    pub(crate) fn purge_lapsed_on(
        &self,
        vfs: &dyn Vfs,
        fid: ifs::ops::audit::FileId,
        sub: Option<u64>,
    ) -> bool {
        let now = self.now();
        let mut t = self.t();
        let lease = self.cfg.lease;
        let mut victims: Vec<u64> = t
            .sids
            .values()
            .filter(|s| s.fh.fid == fid)
            .map(|s| s.client)
            .collect();
        if let Some(sub) = sub
            && let Some(k) = t.owner_of_sub(sub)
        {
            victims.push(k.0);
        }
        victims.sort_unstable();
        victims.dedup();
        victims.retain(|id| t.clients.get(id).is_some_and(|c| c.lapsed(now, lease)));
        if victims.is_empty() {
            return false;
        }
        let mut all = Cleanup::default();
        for v in victims {
            let c = self.drop_client(&mut t, v, Gone::Expired);
            all.closes.extend(c.closes);
            all.subs.extend(c.subs);
            all.leases.extend(c.leases);
        }
        drop(t);
        all.run(vfs);
        true
    }

    /// Takes away the state of the clients whose owner `which` names
    /// (`RLSIFSLCK`): their stateids answer `NFS4ERR_ADMIN_REVOKED`, a
    /// 4.1 client is told by `SEQ4_STATUS_ADMIN_STATE_REVOKED`; the
    /// clients themselves stay. How many lost state.
    pub fn revoke(&self, vfs: &dyn Vfs, which: impl Fn(&[u8]) -> bool) -> usize {
        let mut t = self.t();
        let ids: Vec<u64> = t
            .clients
            .values()
            .filter(|c| which(&c.owner))
            .map(|c| c.id)
            .filter(|id| t.holds_state(*id))
            .collect();
        let mut all = Cleanup::default();
        for id in &ids {
            let sids: Vec<u64> = t
                .sids
                .values()
                .filter(|s| s.client == *id)
                .map(|s| s.id)
                .collect();
            for s in sids {
                if let Some(sid) = t.sids.remove(&s) {
                    match sid.kind {
                        Kind::Open(o) => all.closes.push(o.fd),
                        Kind::Deleg(d) => all.leases.push(d.tag),
                        Kind::Lock(_) => {}
                    }
                    t.gone.insert(s, (Gone::AdminRevoked, *id));
                }
            }
            for (_, lo) in t.lock_owners.iter().filter(|(k, _)| k.0 == *id) {
                all.subs.push(lo.sub);
            }
            t.lock_owners.retain(|k, _| k.0 != *id);
            for (_, oo) in t.open_owners.iter_mut().filter(|(k, _)| k.0 == *id) {
                oo.opens.clear();
            }
            if let Some(c) = t.clients.get_mut(id) {
                c.revoked |= crate::types::nfs4::SEQ4_STATUS_ADMIN_STATE_REVOKED;
            }
        }
        drop(t);
        all.run(vfs);
        ids.len()
    }

    /// Recalls the delegation whose IFS lease is `tag` (a break order
    /// that leaves it no read caching).
    pub fn lease_break(&self, tag: u64, keep: u8) {
        if keep & ifs::lease::state::R != 0 {
            return;
        }
        let now = self.now();
        let mut t = self.t();
        let id = t.sids.values().find_map(|s| match &s.kind {
            Kind::Deleg(d) if d.tag == tag => Some(s.id),
            _ => None,
        });
        if let Some(id) = id {
            start_recall(&mut t, id, now);
        }
    }

    /// One round of the background work: grace, leases, recalls, probes.
    pub fn tick(&self, vfs: &dyn Vfs) {
        let now = self.now();
        let mut t = self.t();
        self.check_grace(&mut t, now);
        let lease = self.cfg.lease;
        let gone: Vec<u64> = t
            .clients
            .values()
            .filter(|c| {
                let idle = now.saturating_duration_since(c.renewed);
                if c.confirmed {
                    idle > lease + self.cfg.courtesy
                } else {
                    idle > lease
                }
            })
            .map(|c| c.id)
            .collect();
        let mut all = Cleanup::default();
        for id in gone {
            let c = self.drop_client(&mut t, id, Gone::Expired);
            all.closes.extend(c.closes);
            all.subs.extend(c.subs);
            all.leases.extend(c.leases);
        }
        // Recalls not answered in time.
        let late: Vec<u64> = t
            .sids
            .values()
            .filter_map(|s| match &s.kind {
                Kind::Deleg(d)
                    if d.recall.is_some_and(|r| {
                        now.saturating_duration_since(r.since) > self.cfg.recall_wait
                    }) =>
                {
                    Some(s.id)
                }
                _ => None,
            })
            .collect();
        for id in late {
            if let Some(s) = t.sids.remove(&id) {
                if let Kind::Deleg(d) = s.kind {
                    all.leases.push(d.tag);
                }
                t.gone.insert(id, (Gone::DelegRevoked, s.client));
                if let Some(c) = t.clients.get_mut(&s.client) {
                    c.revoked |= SEQ4_STATUS_RECALLABLE_STATE_REVOKED;
                }
            }
        }
        // 4.0 open owners with nothing open and idle for a lease.
        t.open_owners.retain(|_, o| {
            !o.opens.is_empty()
                || o.used
                    .is_none_or(|u| now.saturating_duration_since(u) <= lease)
        });
        let recalls = std::mem::take(&mut t.to_recall);
        let probes = std::mem::take(&mut t.to_probe);
        drop(t);
        all.run(vfs);
        if let Some(me) = self.me.upgrade() {
            for id in recalls {
                let s = Arc::clone(&me);
                std::thread::spawn(move || s.send_recall(id));
            }
            for id in probes {
                let s = Arc::clone(&me);
                std::thread::spawn(move || s.probe(id));
            }
        }
    }

    /// The job's background thread until `stop`: break orders from
    /// `news` (the job's lease poll) and [`Server::tick`] about every
    /// 200 milliseconds, on `vfs` (the thread's own).
    pub fn background(&self, vfs: &dyn Vfs, news: &mut dyn NewsSource, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            match news.poll(200) {
                Ok(n) => {
                    for (tag, keep) in n.breaks {
                        self.lease_break(tag, keep);
                    }
                }
                Err(e) => {
                    tracing::warn!("NFSv4 lease poll failed: {e}");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
            self.tick(vfs);
        }
    }

    /// Probes client `id`'s 4.0 callback path with `CB_NULL`.
    fn probe(&self, id: u64) {
        let cb = self.t().clients.get(&id).and_then(|c| c.cb.clone());
        let Some(cb) = cb else { return };
        let ok = self
            .cb
            .connect(&cb.netid, &cb.addr)
            .is_some_and(|mut ch| ch.call(cb.prog, 0, &[]).is_ok());
        if let Some(c) = self.t().clients.get_mut(&id)
            && c.cb.as_ref() == Some(&cb)
        {
            c.path = if ok { CbPath::Up } else { CbPath::Down };
        }
    }

    /// Sends `CB_RECALL` for delegation `id`.
    fn send_recall(&self, id: u64) {
        enum Via {
            V40(super::state::Cb40),
            V41(
                Arc<crate::rpc::back::BackChannel>,
                [u8; 16],
                u32,
                u32,
                Credential,
            ),
        }
        let (via, sid, fh, client) = {
            let mut t = self.t();
            let Some(s) = t.sids.get(&id) else { return };
            let (seq, fh, client) = (s.seq, s.fh.to_bytes(), s.client);
            let Some(c) = t.clients.get(&client) else {
                return;
            };
            let via = if c.minor == 0 {
                c.cb.clone().map(Via::V40)
            } else {
                let sessions = c.sessions.clone();
                sessions.iter().find_map(|sid| {
                    let s = t.sessions.get_mut(sid)?;
                    let b = s.back_conn.as_ref().filter(|b| b.is_open())?.clone();
                    let cred = s.cb_sec.clone()?;
                    s.cb_seq = s.cb_seq.wrapping_add(1);
                    Some(Via::V41(b, s.id, s.cb_seq, s.cb_prog, cred))
                })
            };
            (via, self.wire_sid(id, seq), fh, client)
        };
        let ok = match via {
            None => false,
            Some(Via::V40(c)) => cb::recall_args(c.ident, sid, &fh, None)
                .ok()
                .and_then(|a| {
                    let mut ch = self.cb.connect(&c.netid, &c.addr)?;
                    ch.call(c.prog, 1, &a).ok()
                })
                .is_some_and(|r| cb::reply_status(&r).is_ok()),
            Some(Via::V41(b, session, seq, prog, cred)) => {
                cb::recall_args(0, sid, &fh, Some((session, seq)))
                    .ok()
                    .and_then(|a| b.call(prog, 1, 1, &cred, &a, self.cfg.cb_timeout).ok())
                    .is_some_and(|r| cb::reply_status(&r).is_ok())
            }
        };
        let mut t = self.t();
        if let Some(c) = t.clients.get_mut(&client) {
            c.path = if ok { CbPath::Up } else { CbPath::Down };
        }
        if let Some(s) = t.sids.get_mut(&id)
            && let Kind::Deleg(d) = &mut s.kind
            && let Some(r) = &mut d.recall
        {
            r.sent = true;
        }
    }

    /// The connection `peer`'s sessions lose it (it closed).
    pub(crate) fn conn_gone(&self, peer: SocketAddr) {
        let mut t = self.t();
        for s in t.sessions.values_mut() {
            s.conns.remove(&peer);
            if s.back_conn.as_ref().is_some_and(|b| b.peer() == peer) {
                s.back_conn = None;
            }
        }
    }
}

/// Marks delegation `id` recalled (once) and queues its `CB_RECALL`.
pub(crate) fn start_recall(t: &mut Tables, id: u64, now: Instant) {
    let Some(s) = t.sids.get_mut(&id) else { return };
    if let Kind::Deleg(d) = &mut s.kind
        && d.recall.is_none()
    {
        d.recall = Some(super::state::Recall {
            since: now,
            sent: false,
        });
        t.to_recall.push(id);
    }
}

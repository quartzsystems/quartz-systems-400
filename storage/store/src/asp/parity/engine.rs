// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One parity set at run time: its members' devices, reads that compute
//! a page from its stripe when its unit is not in step, writes held to
//! the next flush and made in epochs behind partial parity, the repair
//! at open, and the synchronization and rebuild steps.
//! See `docs/STORE_FORMAT.md` §11.

use std::collections::{BTreeMap, HashMap};
use std::io;

use blockdev::{BlockDev, BlockError};
use tracing::{info, warn};

use crate::asp::layout::RESERVED_HEAD;
use crate::asp::parity::geometry::{Geometry, PPL_ENTRIES, PageRole};
use crate::asp::parity::gf;
use crate::asp::parity::ppl::{self, Entry, Ident};
use crate::page::{PAGE_SIZE, trailer};

/// Writes held before an epoch runs on its own.
const PENDING_MAX: usize = 4096;

type Res<T> = std::result::Result<T, BlockError>;
/// Pages of one stripe to write: member, page, bytes.
type Writes = Vec<(usize, u64, Vec<u8>)>;

/// Whether a member's pages can be read from its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// In step.
    Ok,
    /// Being rebuilt: in step for blocks below the mark.
    Rebuilding(u64),
    /// Not in step anywhere: missing or failing.
    Failed,
}

/// One member.
pub struct Member {
    /// The unit's UUID.
    pub unit_uuid: [u8; 16],
    /// The device, when attached.
    pub dev: Option<Box<dyn BlockDev>>,
    /// Its health.
    pub health: Health,
    /// The unit's size in pages.
    pub pages: u64,
}

/// Something the engine did on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    /// A member failed and the set goes on without it.
    Failed {
        /// Member number.
        member: usize,
        /// What failed.
        what: String,
    },
    /// A page computed from its stripe was written back to its unit.
    Healed {
        /// Member number.
        member: usize,
        /// The page.
        lba: u64,
    },
}

/// A test hook: stop an epoch part way, as a crash would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// After the partial parity and the data pages are durable, before
    /// any parity page is written: the epoch fails.
    StopAfterData,
}

/// A set at run time.
pub struct Engine {
    asp_uuid: [u8; 16],
    /// Set number.
    pub set: u8,
    /// The layout.
    pub geo: Geometry,
    /// Members by number.
    pub members: Vec<Member>,
    /// While synchronizing: blocks below this are protected.
    pub sync: Option<u64>,
    pending: BTreeMap<(usize, u64), Vec<u8>>,
    dirty: bool,
    seq: u64,
    state_seq: u64,
    next_slot: Vec<usize>,
    events: Vec<EngineEvent>,
    /// The test hook, if armed.
    pub fault: Option<Fault>,
    /// Per member, the runs `(lba, len)` of IFS volume pages: raw,
    /// with no trailer to check.
    raw: Vec<Vec<(u64, u64)>>,
}

fn err(what: impl Into<String>) -> BlockError {
    BlockError::Io {
        op: "parity",
        lba: 0,
        source: io::Error::other(what.into()),
    }
}

fn missing(lba: u64) -> BlockError {
    BlockError::Io {
        op: "read",
        lba,
        source: io::Error::new(io::ErrorKind::NotFound, "the unit is missing"),
    }
}

/// Whether a page is not zero and fails its trailer checksum.
fn suspicious(page: &[u8]) -> bool {
    page.iter().any(|&b| b != 0) && trailer::stored_crc(page) != trailer::checksum(page)
}

/// A stripe's pages as the units hold them; `None` where not known.
struct Stripe {
    data_members: Vec<usize>,
    data: Vec<Option<Vec<u8>>>,
    p: Option<Vec<u8>>,
    q: Option<Vec<u8>>,
}

impl Stripe {
    fn coefs(&self) -> Vec<u8> {
        self.data_members.iter().map(|&m| gf::coef(m)).collect()
    }

    /// Compute the unknown data pages.
    fn fill(&mut self) -> bool {
        let coefs = self.coefs();
        gf::solve(&mut self.data, &coefs, self.p.as_deref(), self.q.as_deref())
    }
}

/// An epoch's plan for one stripe.
struct Plan {
    stripe: u64,
    writes: Vec<(usize, u64, Vec<u8>)>,
    entry: Entry,
    p: Vec<u8>,
    q: Option<Vec<u8>>,
}

impl Engine {
    /// A set over its members. Nothing is read until [`recover`](Self::recover).
    #[must_use]
    pub fn new(asp_uuid: [u8; 16], set: u8, geo: Geometry, members: Vec<Member>, sync: Option<u64>) -> Self {
        let n = members.len();
        Self {
            asp_uuid,
            set,
            geo,
            members,
            sync,
            pending: BTreeMap::new(),
            dirty: false,
            seq: 1,
            state_seq: 0,
            next_slot: vec![0; n],
            events: Vec::new(),
            fault: None,
            raw: vec![Vec::new(); n],
        }
    }

    /// Set a member's IFS runs, ascending.
    pub fn set_raw(&mut self, member: usize, runs: Vec<(u64, u64)>) {
        if let Some(r) = self.raw.get_mut(member) {
            *r = runs;
        }
    }

    /// Whether a member's page is an IFS volume page.
    fn is_raw(&self, member: usize, lba: u64) -> bool {
        let runs = &self.raw[member];
        let i = runs.partition_point(|&(s, _)| s <= lba);
        i > 0 && lba < runs[i - 1].0 + runs[i - 1].1
    }

    /// Events since the last call.
    pub fn drain_events(&mut self) -> Vec<EngineEvent> {
        std::mem::take(&mut self.events)
    }

    /// Take the devices back, the held writes made first.
    pub fn take_devices(&mut self) -> Res<Vec<Option<Box<dyn BlockDev>>>> {
        self.flush()?;
        Ok(self.members.iter_mut().map(|m| m.dev.take()).collect())
    }

    fn ident(&self, member: usize) -> Ident {
        Ident {
            asp_uuid: self.asp_uuid,
            set: self.set,
            level: self.geo.level,
            members: u8::try_from(self.geo.n).expect("at most 18"),
            member: u8::try_from(member).expect("at most 18"),
            blocks: self.geo.blocks,
        }
    }

    /// Members not in step anywhere or not yet rebuilt.
    fn unknown_count(&self) -> usize {
        self.members.iter().filter(|m| m.health != Health::Ok).count()
    }

    /// Whether the set can go on without one more member.
    fn can_lose(&self) -> bool {
        self.sync.is_none() && self.unknown_count() < self.geo.k()
    }

    /// Whether a member's page of `stripe` can be read from its unit.
    fn in_step(&self, member: usize, stripe: u64) -> bool {
        let m = &self.members[member];
        m.dev.is_some()
            && match m.health {
                Health::Ok => true,
                Health::Rebuilding(done) => self.geo.block_of(stripe) < done,
                Health::Failed => false,
            }
    }

    /// Whether writes reach the member's unit.
    fn writable(&self, member: usize) -> bool {
        let m = &self.members[member];
        m.dev.is_some() && m.health != Health::Failed
    }

    /// Whether a stripe has parity to keep.
    fn protected(&self, stripe: u64) -> bool {
        self.sync.is_none_or(|d| self.geo.block_of(stripe) < d)
    }

    fn fail(&mut self, member: usize, what: &str) {
        warn!("parity set {}: member {member} failed: {what}", self.set);
        self.members[member].health = Health::Failed;
        self.events.push(EngineEvent::Failed {
            member,
            what: what.to_string(),
        });
        self.write_state();
    }

    /// Fail a member whose I/O failed, or give the error back when the
    /// set cannot lose it.
    fn on_error(&mut self, member: usize, e: BlockError) -> Res<()> {
        if self.members[member].health != Health::Failed && self.can_lose() {
            self.fail(member, &e.to_string());
            Ok(())
        } else {
            Err(e)
        }
    }

    fn dev(&self, member: usize) -> &dyn BlockDev {
        self.members[member].dev.as_deref().expect("attached")
    }

    /// One page from a member's unit.
    fn read_dev(&self, member: usize, lba: u64) -> Res<Vec<u8>> {
        let Some(dev) = self.members[member].dev.as_deref() else {
            return Err(missing(lba));
        };
        let mut page = vec![0u8; PAGE_SIZE];
        dev.read(lba, &mut page)?;
        Ok(page)
    }

    /// A page of a member in step for `stripe`, or `None` (failing
    /// members are failed on the way when the set can lose them).
    fn read_in_step(&mut self, member: usize, stripe: u64, lba: u64) -> Res<Option<Vec<u8>>> {
        if !self.in_step(member, stripe) {
            return Ok(None);
        }
        match self.read_dev(member, lba) {
            Ok(p) => Ok(Some(p)),
            Err(e) => self.on_error(member, e).map(|()| None),
        }
    }

    /// Read a stripe from the units, `skip` treated as unknown.
    fn read_stripe(&mut self, stripe: u64, skip: Option<usize>) -> Res<Stripe> {
        let data_members = self.geo.data_members(stripe);
        let mut data = Vec::with_capacity(data_members.len());
        for &m in &data_members {
            if Some(m) == skip {
                data.push(None);
            } else {
                let lba = self.geo.row_of(stripe, m);
                data.push(self.read_in_step(m, stripe, lba)?);
            }
        }
        let (pm, qm) = self.geo.parity_members(stripe);
        let p = if Some(pm) == skip {
            None
        } else {
            self.read_in_step(pm, stripe, self.geo.parity_lba(stripe, false))?
        };
        let q = match qm {
            Some(qm) if Some(qm) != skip => {
                self.read_in_step(qm, stripe, self.geo.parity_lba(stripe, true))?
            }
            _ => None,
        };
        Ok(Stripe {
            data_members,
            data,
            p,
            q,
        })
    }

    /// A row of `member` computed from the rest of its stripe.
    fn reconstruct(&mut self, member: usize, stripe: u64) -> Res<Vec<u8>> {
        if !self.protected(stripe) {
            return Err(err(format!(
                "parity set {}: stripe {stripe} has no parity yet and member {member} is not in step",
                self.set
            )));
        }
        let mut st = self.read_stripe(stripe, Some(member))?;
        if !st.fill() {
            return Err(err(format!(
                "parity set {}: too many units unknown in stripe {stripe}",
                self.set
            )));
        }
        let i = st
            .data_members
            .iter()
            .position(|&m| m == member)
            .expect("a data member");
        Ok(st.data[i].take().expect("filled"))
    }

    /// Read one page of a member, as the store sees it.
    pub fn read_page(&mut self, member: usize, lba: u64) -> Res<Vec<u8>> {
        if let Some(p) = self.pending.get(&(member, lba)) {
            return Ok(p.clone());
        }
        let Some(stripe) = self.geo.stripe_of(member, lba) else {
            // The unit's own pages: a lost unit's read as zeros.
            if !self.writable(member) {
                return Ok(vec![0u8; PAGE_SIZE]);
            }
            return match self.read_dev(member, lba) {
                Ok(p) => Ok(p),
                Err(e) => self.on_error(member, e).map(|()| vec![0u8; PAGE_SIZE]),
            };
        };
        if self.in_step(member, stripe) {
            match self.read_dev(member, lba) {
                Ok(page) => {
                    if !self.protected(stripe) || self.is_raw(member, lba) || !suspicious(&page) {
                        return Ok(page);
                    }
                    // A bad page on a good unit: try the stripe.
                    match self.reconstruct(member, stripe) {
                        Ok(fixed) if !suspicious(&fixed) => {
                            if self.dev(member).write(lba, &fixed).is_ok() {
                                self.dirty = true;
                                warn!("parity set {}: member {member} page {lba} rewritten from parity", self.set);
                                self.events.push(EngineEvent::Healed { member, lba });
                            }
                            return Ok(fixed);
                        }
                        _ => return Ok(page),
                    }
                }
                Err(e) => self.on_error(member, e)?,
            }
        }
        self.reconstruct(member, stripe)
    }

    /// Write one page of a member: rows are held for the next flush,
    /// the unit's own pages go straight to it.
    pub fn write_page(&mut self, member: usize, lba: u64, page: &[u8]) -> Res<()> {
        if self.geo.stripe_of(member, lba).is_some() {
            self.pending.insert((member, lba), page.to_vec());
            if self.pending.len() >= PENDING_MAX {
                self.flush()?;
            }
            return Ok(());
        }
        if !self.writable(member) {
            return Ok(()); // a missing unit's header or superblock copy
        }
        match self.dev(member).write(lba, page) {
            Ok(()) => {
                self.dirty = true;
                Ok(())
            }
            Err(e) => self.on_error(member, e),
        }
    }

    /// Flush every writable member.
    fn flush_members(&mut self, which: &[usize]) -> Res<()> {
        for &m in which {
            if !self.writable(m) {
                continue;
            }
            if let Err(e) = self.dev(m).flush() {
                self.on_error(m, e)?;
            }
        }
        Ok(())
    }

    fn all_members(&self) -> Vec<usize> {
        (0..self.members.len()).collect()
    }

    /// Make every held write durable, in epochs.
    pub fn flush(&mut self) -> Res<()> {
        if self.pending.is_empty() {
            if self.dirty {
                self.flush_members(&self.all_members())?;
                self.dirty = false;
            }
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending);
        let mut by_stripe: BTreeMap<u64, Vec<(usize, u64, Vec<u8>)>> = BTreeMap::new();
        for ((m, lba), page) in pending {
            let s = self.geo.stripe_of(m, lba).expect("a row");
            by_stripe.entry(s).or_default().push((m, lba, page));
        }
        let mut protected = Vec::new();
        for (s, writes) in by_stripe {
            if self.protected(s) {
                protected.push((s, writes));
                continue;
            }
            // Not synchronized yet: the data page alone.
            for (m, lba, page) in writes {
                if !self.writable(m) {
                    return Err(err(format!(
                        "parity set {}: member {m} is not available and stripe {s} has no parity yet",
                        self.set
                    )));
                }
                self.dev(m).write(lba, &page)?;
            }
        }
        let mut rest = protected.as_slice();
        while !rest.is_empty() {
            let (now, later) = rest.split_at(rest.len().min(PPL_ENTRIES));
            self.epoch(now)?;
            rest = later;
        }
        self.flush_members(&self.all_members())?;
        self.dirty = false;
        Ok(())
    }

    /// Plan one stripe of an epoch.
    fn plan(&mut self, stripe: u64, writes: &[(usize, u64, Vec<u8>)]) -> Res<Plan> {
        let mut st = self.read_stripe(stripe, None)?;
        if !st.fill() {
            return Err(err(format!(
                "parity set {}: too many units unknown in stripe {stripe}",
                self.set
            )));
        }
        let coefs = st.coefs();
        let mut written = 0u32;
        let mut new: Vec<Vec<u8>> = st.data.iter().map(|d| d.clone().expect("filled")).collect();
        for (m, _, page) in writes {
            let i = st.data_members.iter().position(|x| x == m).expect("a data member");
            written |= 1 << m;
            new[i].clone_from(page);
        }
        let k6 = self.geo.k() == 2;
        let (unw, unw_coefs): (Vec<&[u8]>, Vec<u8>) = st
            .data_members
            .iter()
            .zip(&st.data)
            .zip(&coefs)
            .filter(|((m, _), _)| written & (1 << **m) == 0)
            .map(|((_, d), c)| (d.as_deref().expect("filled"), *c))
            .unzip();
        let (pp, pq) = gf::compute(&unw, &unw_coefs, PAGE_SIZE, k6);
        let refs: Vec<&[u8]> = new.iter().map(Vec::as_slice).collect();
        let (p, q) = gf::compute(&refs, &coefs, PAGE_SIZE, k6);
        Ok(Plan {
            stripe,
            writes: writes.to_vec(),
            entry: Entry {
                stripe,
                written,
                p: pp,
                q: pq,
            },
            p,
            q,
        })
    }

    /// Steps 1 to 3 of an epoch for up to [`PPL_ENTRIES`] stripes.
    fn epoch(&mut self, stripes: &[(u64, Writes)]) -> Res<()> {
        let mut plans = Vec::with_capacity(stripes.len());
        for (s, writes) in stripes {
            plans.push(self.plan(*s, writes)?);
        }
        // Step 2: the partial parity on each parity member.
        let mut slots: BTreeMap<usize, Vec<Entry>> = BTreeMap::new();
        for plan in &plans {
            let (pm, qm) = self.geo.parity_members(plan.stripe);
            for m in std::iter::once(pm).chain(qm) {
                if self.writable(m) {
                    slots.entry(m).or_default().push(plan.entry.clone());
                }
            }
        }
        let seq = self.seq;
        self.seq += 1;
        let holders: Vec<usize> = slots.keys().copied().collect();
        for (m, entries) in slots {
            let bytes = ppl::encode(&self.ident(m), seq, &entries);
            let at = self.geo.slot(self.next_slot[m]);
            match self.dev(m).write(at, &bytes) {
                Ok(()) => self.next_slot[m] ^= 1,
                Err(e) => self.on_error(m, e)?,
            }
        }
        self.flush_members(&holders)?;
        // Step 3: in place.
        for plan in &plans {
            for (m, lba, page) in &plan.writes {
                if self.writable(*m)
                    && let Err(e) = self.dev(*m).write(*lba, page)
                {
                    self.on_error(*m, e)?;
                }
            }
        }
        if self.fault == Some(Fault::StopAfterData) {
            self.flush_members(&self.all_members())?;
            return Err(err("injected stop after the data pages"));
        }
        for plan in &plans {
            self.write_parity(plan.stripe, &plan.p, plan.q.as_deref())?;
        }
        Ok(())
    }

    fn write_parity(&mut self, stripe: u64, p: &[u8], q: Option<&[u8]>) -> Res<()> {
        let (pm, qm) = self.geo.parity_members(stripe);
        let mut out = vec![(pm, self.geo.parity_lba(stripe, false), p)];
        if let (Some(qm), Some(q)) = (qm, q) {
            out.push((qm, self.geo.parity_lba(stripe, true), q));
        }
        for (m, lba, page) in out {
            if self.writable(m)
                && let Err(e) = self.dev(m).write(lba, page)
            {
                self.on_error(m, e)?;
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// The newest valid slot of a member.
    fn newest_slot(&self, member: usize) -> Option<(usize, u64, Vec<Entry>)> {
        let id = self.ident(member);
        let mut best: Option<(usize, u64, Vec<Entry>)> = None;
        for i in 0..2 {
            let at = self.geo.slot(i);
            let Ok(head) = self.read_dev(member, at) else {
                continue;
            };
            let Some((_, count)) = ppl::header(&id, &head) else {
                continue;
            };
            let mut bytes = vec![0u8; ppl::pages_for(count) * PAGE_SIZE];
            if self.dev(member).read(at, &mut bytes).is_err() {
                continue;
            }
            if let Some((seq, entries)) = ppl::decode(&id, &bytes)
                && best.as_ref().is_none_or(|b| seq > b.1)
            {
                best = Some((i, seq, entries));
            }
        }
        best
    }

    /// Repair every stripe the newest partial-parity slots name. Runs at
    /// open, before anything else reads the set. Returns the stripes.
    pub fn recover(&mut self) -> Res<usize> {
        let mut found: HashMap<u64, (u64, Entry)> = HashMap::new();
        let mut top = 0;
        for m in 0..self.members.len() {
            if !self.writable(m) {
                continue;
            }
            let Some((slot, seq, entries)) = self.newest_slot(m) else {
                continue;
            };
            self.next_slot[m] = slot ^ 1;
            top = top.max(seq);
            for e in entries {
                if let Health::Rebuilding(done) = self.members[m].health
                    && self.geo.block_of(e.stripe) >= done
                {
                    continue;
                }
                if e.stripe >= self.geo.stripes() {
                    continue;
                }
                if found.get(&e.stripe).is_none_or(|(s, _)| seq > *s) {
                    found.insert(e.stripe, (seq, e));
                }
            }
        }
        self.seq = top + 1;
        let mut stripes: Vec<u64> = found.keys().copied().collect();
        stripes.sort_unstable();
        for s in &stripes {
            let (_, e) = found.remove(s).expect("listed");
            self.repair(&e)?;
        }
        self.flush_members(&self.all_members())?;
        if !stripes.is_empty() {
            info!(
                "parity set {}: {} stripe(s) of the last epoch checked",
                self.set,
                stripes.len()
            );
        }
        Ok(stripes.len())
    }

    /// Make one stripe of an interrupted epoch consistent.
    fn repair(&mut self, e: &Entry) -> Res<()> {
        let s = e.stripe;
        let mut st = self.read_stripe(s, None)?;
        let coefs = st.coefs();
        // The pages the epoch did not write, from the partial parity.
        let idx: Vec<usize> = (0..st.data.len())
            .filter(|&i| e.written & (1 << st.data_members[i]) == 0)
            .collect();
        let mut sub: Vec<Option<Vec<u8>>> = idx.iter().map(|&i| st.data[i].clone()).collect();
        let sub_coefs: Vec<u8> = idx.iter().map(|&i| coefs[i]).collect();
        if !gf::solve(&mut sub, &sub_coefs, Some(&e.p), e.q.as_deref()) {
            return Err(err(format!(
                "parity set {}: stripe {s} cannot be repaired: too many units unknown",
                self.set
            )));
        }
        for (&i, v) in idx.iter().zip(sub) {
            st.data[i] = v;
        }
        // The pages it wrote: what the stripe's parity says, else zeros.
        if st.data.iter().any(Option::is_none) && !st.fill() {
            for d in &mut st.data {
                d.get_or_insert_with(|| vec![0u8; PAGE_SIZE]);
            }
        }
        let refs: Vec<&[u8]> = st.data.iter().map(|d| d.as_deref().expect("filled")).collect();
        let (p, q) = gf::compute(&refs, &coefs, PAGE_SIZE, self.geo.k() == 2);
        self.write_parity(s, &p, q.as_deref())
    }

    /// The members failed now, as a mask.
    fn failed_mask(&self) -> u32 {
        self.members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.health == Health::Failed)
            .fold(0, |a, (i, _)| a | 1 << i)
    }

    /// Record on every member that answers which members have failed,
    /// before anything is written without them. Best effort: a member
    /// that cannot take it fails its next write.
    fn write_state(&mut self) {
        self.state_seq += 1;
        let page = ppl::encode_state(&self.ident(0), self.state_seq, self.failed_mask());
        let at = self.geo.state_page();
        for m in 0..self.members.len() {
            if self.writable(m) {
                let d = self.dev(m);
                let _ = d.write(at, &page).and_then(|()| d.flush());
            }
        }
    }

    /// Zero every member's state page (a new set).
    fn zero_state(&mut self) -> Res<()> {
        let zero = vec![0u8; PAGE_SIZE];
        let at = self.geo.state_page();
        for m in 0..self.members.len() {
            if self.writable(m)
                && let Err(e) = self.dev(m).write(at, &zero)
            {
                self.on_error(m, e)?;
            }
        }
        self.state_seq = 0;
        self.flush_members(&self.all_members())
    }

    /// The members the newest state page says failed. Read at open,
    /// before [`recover`](Self::recover).
    pub fn recorded_failures(&mut self) -> u32 {
        let at = self.geo.state_page();
        let id = self.ident(0);
        let mut best: Option<(u64, u32)> = None;
        for m in 0..self.members.len() {
            if !self.writable(m) {
                continue;
            }
            if let Ok(page) = self.read_dev(m, at)
                && let Some((seq, failed)) = ppl::decode_state(&id, &page)
                && best.is_none_or(|b| seq > b.0)
            {
                best = Some((seq, failed));
            }
        }
        self.state_seq = best.map_or(0, |b| b.0);
        best.map_or(0, |b| b.1)
    }

    /// Zero the slot headers of some members.
    fn zero_slots(&mut self, which: &[usize]) -> Res<()> {
        let zero = vec![0u8; PAGE_SIZE];
        for &m in which {
            if !self.writable(m) {
                continue;
            }
            for i in 0..2 {
                let at = self.geo.slot(i);
                if let Err(e) = self.dev(m).write(at, &zero) {
                    self.on_error(m, e)?;
                }
            }
            self.next_slot[m] = 0;
        }
        self.flush_members(which)
    }

    /// Compute the parity of up to `budget` blocks from the mark. Returns
    /// the new mark; the set is protected when it reaches the end.
    pub fn sync_step(&mut self, budget: u64) -> Res<u64> {
        self.flush()?;
        let Some(done) = self.sync else {
            return Ok(self.geo.blocks);
        };
        if done == 0 {
            self.zero_slots(&self.all_members())?;
            self.zero_state()?;
        }
        let end = (done + budget.max(1)).min(self.geo.blocks);
        if self.members.iter().all(|m| m.dev.is_some() && m.health == Health::Ok) {
            self.sync_bulk(done, end)?;
        } else {
            self.sync_stripes(done, end)?;
        }
        self.flush_members(&self.all_members())?;
        self.dirty = false;
        self.sync = (end < self.geo.blocks).then_some(end);
        Ok(end)
    }

    /// Where blocks `from..` start: the first row and the first parity page.
    fn bases(&self, from: u64) -> (u64, u64) {
        (
            RESERVED_HEAD + from * self.geo.w() as u64,
            self.geo.parity_lba(from * self.geo.n as u64, false),
        )
    }

    /// One run of pages from a member.
    fn read_run(&self, member: usize, lba: u64, pages: usize) -> Res<Vec<u8>> {
        let mut buf = vec![0u8; pages * PAGE_SIZE];
        self.dev(member).read(lba, &mut buf)?;
        Ok(buf)
    }

    /// Synchronize blocks `from..to` with one read of each member's rows
    /// and one write of its parity pages: they are contiguous.
    #[allow(clippy::many_single_char_names)] // stripe arithmetic
    fn sync_bulk(&mut self, from: u64, to: u64) -> Res<()> {
        let (count, width, kp) = (self.geo.n, self.geo.w(), self.geo.k());
        let blocks = usize::try_from(to - from).expect("a step's blocks");
        let (row0, par0) = self.bases(from);
        let mut runs = Vec::with_capacity(count);
        for m in 0..count {
            runs.push(self.read_run(m, row0, blocks * width)?);
        }
        let mut parity = vec![vec![0u8; blocks * kp * PAGE_SIZE]; count];
        for b in 0..blocks {
            for j in 0..count {
                let s = (from + b as u64) * count as u64 + j as u64;
                let dm = self.geo.data_members(s);
                let pages: Vec<&[u8]> = dm
                    .iter()
                    .map(|&m| {
                        let at = usize::try_from(self.geo.row_of(s, m) - row0).expect("in the run") * PAGE_SIZE;
                        &runs[m][at..at + PAGE_SIZE]
                    })
                    .collect();
                let coefs: Vec<u8> = dm.iter().map(|&m| gf::coef(m)).collect();
                let (p, q) = gf::compute(&pages, &coefs, PAGE_SIZE, kp == 2);
                let (pm, qm) = self.geo.parity_members(s);
                let at = b * kp * PAGE_SIZE;
                parity[pm][at..at + PAGE_SIZE].copy_from_slice(&p);
                if let (Some(qm), Some(q)) = (qm, q) {
                    parity[qm][at + PAGE_SIZE..at + 2 * PAGE_SIZE].copy_from_slice(&q);
                }
            }
        }
        for (m, buf) in parity.iter().enumerate() {
            self.dev(m).write(par0, buf)?;
        }
        Ok(())
    }

    /// Synchronize blocks `from..to` a stripe at a time.
    fn sync_stripes(&mut self, done: u64, end: u64) -> Res<()> {
        for s in done * self.geo.n as u64..end * self.geo.n as u64 {
            let st = self.read_stripe(s, None)?;
            if st.data.iter().any(Option::is_none) {
                return Err(err(format!(
                    "parity set {}: a member is not available during synchronization",
                    self.set
                )));
            }
            let refs: Vec<&[u8]> = st.data.iter().map(|d| d.as_deref().expect("read")).collect();
            let (p, q) = gf::compute(&refs, &st.coefs(), PAGE_SIZE, self.geo.k() == 2);
            self.write_parity(s, &p, q.as_deref())?;
        }
        Ok(())
    }

    /// Rebuild up to `budget` blocks of a member from the mark. Returns
    /// the new mark; the member is in step when it reaches the end.
    pub fn rebuild_step(&mut self, member: usize, budget: u64) -> Res<u64> {
        self.flush()?;
        let Health::Rebuilding(done) = self.members[member].health else {
            return Ok(self.geo.blocks);
        };
        if self.members[member].dev.is_none() {
            return Err(missing(0));
        }
        if done == 0 {
            self.zero_slots(&[member])?;
        }
        let end = (done + budget.max(1)).min(self.geo.blocks);
        let others_ok = self
            .members
            .iter()
            .enumerate()
            .all(|(i, m)| i == member || (m.dev.is_some() && m.health == Health::Ok));
        // A read that fails in bulk is retried a stripe at a time, where a
        // failing member is failed on the way when the set can lose it.
        if !(others_ok && self.rebuild_bulk(member, done, end).is_ok()) {
            self.rebuild_stripes(member, done, end)?;
        }
        self.dev(member).flush()?;
        if end < self.geo.blocks {
            self.members[member].health = Health::Rebuilding(end);
        } else {
            self.members[member].health = Health::Ok;
            self.write_state();
        }
        Ok(end)
    }

    /// Rebuild blocks `from..to` of a member with one read of each other
    /// member's rows and parity pages and one write of each of its own.
    #[allow(clippy::many_single_char_names)] // stripe arithmetic
    fn rebuild_bulk(&mut self, member: usize, from: u64, to: u64) -> Res<()> {
        let (count, width, kp) = (self.geo.n, self.geo.w(), self.geo.k());
        let blocks = usize::try_from(to - from).expect("a step's blocks");
        let (row0, par0) = self.bases(from);
        let mut runs = Vec::with_capacity(count);
        let mut parity = Vec::with_capacity(count);
        for m in 0..count {
            if m == member {
                runs.push(Vec::new());
                parity.push(Vec::new());
            } else {
                runs.push(self.read_run(m, row0, blocks * width)?);
                parity.push(self.read_run(m, par0, blocks * kp)?);
            }
        }
        let mut my_rows = vec![0u8; blocks * width * PAGE_SIZE];
        let mut my_par = vec![0u8; blocks * kp * PAGE_SIZE];
        let page = |buf: &[u8], at: usize| buf[at..at + PAGE_SIZE].to_vec();
        for b in 0..blocks {
            for j in 0..count {
                let s = (from + b as u64) * count as u64 + j as u64;
                let dm = self.geo.data_members(s);
                let row_at = |m: usize| usize::try_from(self.geo.row_of(s, m) - row0).expect("in the run") * PAGE_SIZE;
                let (pm, qm) = self.geo.parity_members(s);
                let at = b * kp * PAGE_SIZE;
                let mut st = Stripe {
                    data: dm.iter().map(|&m| (m != member).then(|| page(&runs[m], row_at(m)))).collect(),
                    p: (pm != member).then(|| page(&parity[pm], at)),
                    q: qm.filter(|&q| q != member).map(|q| page(&parity[q], at + PAGE_SIZE)),
                    data_members: dm,
                };
                if let Some(i) = st.data_members.iter().position(|&m| m == member) {
                    if !st.fill() {
                        return Err(err(format!("parity set {}: stripe {s} cannot be rebuilt", self.set)));
                    }
                    let to = row_at(member);
                    my_rows[to..to + PAGE_SIZE].copy_from_slice(st.data[i].as_deref().expect("filled"));
                } else {
                    let refs: Vec<&[u8]> = st.data.iter().map(|d| d.as_deref().expect("known")).collect();
                    let (p, q) = gf::compute(&refs, &st.coefs(), PAGE_SIZE, kp == 2);
                    if pm == member {
                        my_par[at..at + PAGE_SIZE].copy_from_slice(&p);
                    } else {
                        my_par[at + PAGE_SIZE..at + 2 * PAGE_SIZE].copy_from_slice(&q.expect("RAID 6"));
                    }
                }
            }
        }
        self.dev(member).write(row0, &my_rows)?;
        self.dev(member).write(par0, &my_par)
    }

    /// Rebuild blocks `done..end` of a member a stripe at a time.
    fn rebuild_stripes(&mut self, member: usize, done: u64, end: u64) -> Res<()> {
        let k6 = self.geo.k() == 2;
        for s in done * self.geo.n as u64..end * self.geo.n as u64 {
            let mut st = self.read_stripe(s, Some(member))?;
            if !st.fill() {
                return Err(err(format!(
                    "parity set {}: too many units unknown to rebuild stripe {s}",
                    self.set
                )));
            }
            let coefs = st.coefs();
            if let Some(i) = st.data_members.iter().position(|&m| m == member) {
                let lba = self.geo.row_of(s, member);
                let page = st.data[i].take().expect("filled");
                self.dev(member).write(lba, &page)?;
            } else {
                let refs: Vec<&[u8]> = st.data.iter().map(|d| d.as_deref().expect("filled")).collect();
                let (p, q) = gf::compute(&refs, &coefs, PAGE_SIZE, k6);
                let (pm, qm) = self.geo.parity_members(s);
                if pm == member {
                    self.dev(member).write(self.geo.parity_lba(s, false), &p)?;
                }
                if qm == Some(member) {
                    let q = q.expect("RAID 6");
                    self.dev(member).write(self.geo.parity_lba(s, true), &q)?;
                }
            }
        }
        Ok(())
    }

    /// Whether every stripe's parity matches its data, for tests and
    /// CHKSTG: the stripes that do not.
    pub fn verify(&mut self) -> Res<Vec<u64>> {
        self.flush()?;
        let mut bad = Vec::new();
        for s in 0..self.geo.stripes() {
            if !self.protected(s) {
                continue;
            }
            let st = self.read_stripe(s, None)?;
            if st.data.iter().any(Option::is_none) || st.p.is_none() {
                continue;
            }
            let refs: Vec<&[u8]> = st.data.iter().map(|d| d.as_deref().expect("read")).collect();
            let (p, q) = gf::compute(&refs, &st.coefs(), PAGE_SIZE, self.geo.k() == 2);
            if st.p.as_deref() != Some(&p[..]) || (q.is_some() && st.q.is_some() && st.q != q) {
                bad.push(s);
            }
        }
        Ok(bad)
    }

    /// The role of a member page, for displays.
    #[must_use]
    pub fn role(&self, member: usize, lba: u64) -> Option<PageRole> {
        self.geo.page_of(member, lba)
    }
}

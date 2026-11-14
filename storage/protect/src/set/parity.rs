// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parity set as one block device: the members' rows end to end.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use blockdev::{BLOCK_SIZE, BlockDev, BlockError, check_range};
use tracing::{info, warn};

use crate::PAGE_SIZE;
use crate::intent::IntentMap;
use crate::parity::{
    Engine, EngineEvent, Format, Geometry, Health, Level, MAX_MEMBERS, Member, ppl,
};
use crate::set::away::{self, Kept, ParityIntent, Watch};
use crate::set::{SetError, SetEvent, no_member};
use crate::spare::{self, Spare};

/// One member, as the caller recorded it.
pub struct ParityMember {
    /// Its identity.
    pub uuid: [u8; 16],
    /// Its health as last recorded: in step, being rebuilt to a mark,
    /// or failed.
    pub health: Health,
    /// Its device, when it was found.
    pub dev: Option<Box<dyn BlockDev>>,
}

/// A member, for displays and for the caller's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParityMemberInfo {
    /// Its identity.
    pub uuid: [u8; 16],
    /// Its health.
    pub health: Health,
    /// Whether a device is attached.
    pub attached: bool,
}

/// The background work a set has to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParityWork {
    /// Compute the set's parity: blocks done.
    Sync(u64),
    /// Rebuild a member: the member and its blocks done.
    Rebuild(usize, u64),
}

/// How far a piece of work has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParityProgress {
    /// The work, with its new mark.
    pub work: ParityWork,
    /// Blocks in the set.
    pub blocks: u64,
    /// The work is done.
    pub complete: bool,
}

/// A parity set. Clones are handles on the same set.
#[derive(Clone)]
pub struct ParitySet {
    engine: Arc<Mutex<Engine>>,
    geo: Geometry,
    /// Rows of each member.
    rows: u64,
    events: Arc<Mutex<Vec<SetEvent>>>,
    /// The maps of members out of service, when the set keeps them.
    away: Option<Arc<Mutex<Watch>>>,
}

/// The regions of the stripes the partial-parity slots of `devs` name:
/// what may have been in flight when the set stopped.
fn in_flight(
    fmt: &Format,
    id: [u8; 16],
    geo: &Geometry,
    devs: &[Option<Box<dyn BlockDev>>],
    rb: u64,
) -> IntentMap {
    let mut map = Watch::scratch(id, geo.blocks, rb);
    for (m, dev) in devs.iter().enumerate() {
        let Some(dev) = dev.as_deref() else {
            continue;
        };
        let ident = ppl::Ident {
            magic: fmt.ppl_magic,
            state_magic: fmt.state_magic,
            asp_uuid: id,
            set: 1,
            level: geo.level,
            members: u8::try_from(geo.n).expect("at most 18"),
            member: u8::try_from(m).expect("at most 18"),
            blocks: geo.blocks,
        };
        for i in 0..2 {
            let at = geo.slot(i);
            let mut head = vec![0u8; PAGE_SIZE];
            if dev.read(at, &mut head).is_err() {
                continue;
            }
            let Some((_, count)) = ppl::header(&ident, &head) else {
                continue;
            };
            let mut bytes = vec![0u8; ppl::pages_for(count) * PAGE_SIZE];
            if dev.read(at, &mut bytes).is_err() {
                continue;
            }
            if let Some((_, entries)) = ppl::decode(&ident, &bytes) {
                for e in entries {
                    if e.stripe < geo.stripes() {
                        map.set(geo.block_of(e.stripe) / rb);
                    }
                }
            }
        }
    }
    map
}

impl ParitySet {
    /// The layout of a set of `n` members whose smallest has `smallest`
    /// blocks, the first `head` of each member and its last block left
    /// alone. The caller records it: a set is opened with the layout it
    /// was made with, whatever the size of a member replaced since.
    pub fn plan(level: Level, n: usize, smallest: u64, head: u64) -> Result<Geometry, SetError> {
        if n > MAX_MEMBERS {
            return Err(SetError::Unusable(format!(
                "{n} members; a set has at most {MAX_MEMBERS}"
            )));
        }
        Geometry::new(level, n, smallest, head)
            .ok_or_else(|| SetError::Unusable("members too small or too few".into()))
    }

    /// A new set over blank or reused devices: every member in step and
    /// the parity to be computed. Run [`step`](Self::step) until the
    /// set is protected.
    pub fn create(
        fmt: Format,
        id: [u8; 16],
        level: Level,
        head: u64,
        devs: Vec<([u8; 16], Box<dyn BlockDev>)>,
    ) -> Result<Self, SetError> {
        Self::create_with(fmt, id, level, head, devs, None)
    }

    /// [`create`](Self::create), keeping a map for each member out of
    /// service as [`open_with`](Self::open_with) does.
    pub fn create_with(
        fmt: Format,
        id: [u8; 16],
        level: Level,
        head: u64,
        devs: Vec<([u8; 16], Box<dyn BlockDev>)>,
        intent: Option<ParityIntent>,
    ) -> Result<Self, SetError> {
        let smallest = devs.iter().map(|(_, d)| d.blocks()).min().unwrap_or(0);
        let geo = Self::plan(level, devs.len(), smallest, head)?;
        let members = devs
            .into_iter()
            .map(|(uuid, dev)| ParityMember {
                uuid,
                health: Health::Ok,
                dev: Some(dev),
            })
            .collect();
        Self::open_with(fmt, id, geo, members, Some(0), intent)
    }

    /// A set as the caller recorded it: its layout, its members and the
    /// synchronization mark (`None` when the parity is computed). A
    /// member without a device is failed and the set runs exposed; one
    /// recorded failed that is back, or that the others recorded as
    /// failed after the caller's record, is stale and is rebuilt. The
    /// last epoch is repaired before anything is read.
    pub fn open(
        fmt: Format,
        id: [u8; 16],
        geo: Geometry,
        members: Vec<ParityMember>,
        sync: Option<u64>,
    ) -> Result<Self, SetError> {
        Self::open_with(fmt, id, geo, members, sync, None)
    }

    /// [`open`](Self::open), and with an intent, a map kept for each
    /// member that has a store while it is out of service: from the
    /// moment it fails or leaves, never cleared until it is in step
    /// again. A member back with its map is rebuilt in the map's
    /// regions only (from the first block, whatever mark was recorded);
    /// one with no valid map, or replaced by another disk, is rebuilt
    /// whole. A member recorded in step and missing now, that the others
    /// did not record failed, gets a map of what was in flight.
    pub fn open_with(
        fmt: Format,
        id: [u8; 16],
        geo: Geometry,
        members: Vec<ParityMember>,
        sync: Option<u64>,
        mut intent: Option<ParityIntent>,
    ) -> Result<Self, SetError> {
        let n = members.len();
        if n != geo.n {
            return Err(SetError::Unusable(format!(
                "{n} members for a layout of {}",
                geo.n
            )));
        }
        if intent.as_ref().is_some_and(|i| i.region_blocks == 0) {
            return Err(SetError::Unusable("a region of no blocks".into()));
        }
        let mut events = Vec::new();
        let mut kept = Vec::new();
        let mut recorded = Vec::with_capacity(n);
        let mut healths = Vec::with_capacity(n);
        let mut devs = Vec::with_capacity(n);
        let mut uuids = Vec::with_capacity(n);
        for (i, m) in members.into_iter().enumerate() {
            if let Some(d) = &m.dev
                && d.blocks() <= geo.end()
            {
                return Err(SetError::refused(
                    i,
                    format!("{} blocks, the layout needs {}", d.blocks(), geo.end() + 1),
                ));
            }
            if let Some(pi) = &mut intent {
                let store = pi.away.get_mut(i).and_then(Option::take);
                kept.push(
                    Watch::read_kept(pi.magic, id, m.uuid, geo.blocks, pi.region_blocks, store)
                        .map_err(SetError::Intent)?,
                );
            }
            let mapped = kept.get(i).is_some_and(Kept::has_map);
            let health = Self::opening_health(i, &m, mapped, sync.is_some(), &mut events)?;
            recorded.push(m.health);
            healths.push(health);
            uuids.push(m.uuid);
            devs.push(m.dev);
        }
        let watch = intent.map(|pi| {
            let flight = in_flight(&fmt, id, &geo, &devs, pi.region_blocks);
            Arc::new(Mutex::new(Watch::new(
                pi.magic,
                id,
                geo.blocks,
                pi.region_blocks,
                kept,
                uuids.clone(),
                healths.clone(),
                flight,
            )))
        });
        let engine_members = Self::engine_members(devs, &uuids, &healths, watch.as_ref());
        let mut engine = Engine::new(fmt, id, 1, geo, engine_members, sync);
        // A member that failed after the caller's last record is
        // recorded on the others.
        let failed = engine.recorded_failures();
        for (i, m) in engine.members.iter_mut().enumerate() {
            if failed & (1 << i) != 0 && m.health == Health::Ok && m.dev.is_some() {
                m.health = Health::Rebuilding(0);
                events.push(SetEvent::Stale { member: i });
            }
        }
        if let Some(w) = &watch {
            Self::missing_maps(&engine, &mut away::lock(w), &recorded, failed);
        }
        let unknown = engine
            .members
            .iter()
            .filter(|m| m.health != Health::Ok)
            .count();
        if unknown > geo.k() {
            return Err(SetError::Unusable(format!(
                "{unknown} members out of step; the set can lose {}",
                geo.k()
            )));
        }
        engine.recover()?;
        // A member found missing is recorded on the others, as one that
        // fails while the set runs is: the caller's record may be lost.
        if events.iter().any(|e| matches!(e, SetEvent::Missing { .. })) {
            engine.record_failed();
        }
        let set = Self {
            engine: Arc::new(Mutex::new(engine)),
            geo,
            rows: geo.rows_end() - geo.head,
            events: Arc::new(Mutex::new(events)),
            away: watch,
        };
        if let Some(w) = &set.away {
            set.settle(&set.lock());
            // The repair made what was in flight consistent.
            away::lock(w).flushed();
        }
        Ok(set)
    }

    /// What member `i` as recorded is at open, before the others' record
    /// is read; `mapped` when its map is kept.
    fn opening_health(
        i: usize,
        m: &ParityMember,
        mapped: bool,
        syncing: bool,
        events: &mut Vec<SetEvent>,
    ) -> Result<Health, SetError> {
        Ok(match (m.health, m.dev.is_some()) {
            (Health::Ok, true) => Health::Ok,
            (Health::Ok, false) => {
                if syncing {
                    return Err(SetError::refused(
                        i,
                        "missing while the set is synchronizing",
                    ));
                }
                events.push(SetEvent::Missing { member: i });
                Health::Failed
            }
            // With a map, a rebuild starts again over the map: the
            // member may have failed again after the mark was recorded.
            (Health::Rebuilding(mark), true) if mapped && mark > 0 => {
                events.push(SetEvent::Stale { member: i });
                Health::Rebuilding(0)
            }
            (Health::Rebuilding(mark), true) => Health::Rebuilding(mark),
            (Health::Failed, true) => {
                events.push(SetEvent::Stale { member: i });
                Health::Rebuilding(0)
            }
            (Health::Rebuilding(_) | Health::Failed, false) => Health::Failed,
        })
    }

    /// The engine's members, each device under the watch when there is one.
    fn engine_members(
        devs: Vec<Option<Box<dyn BlockDev>>>,
        uuids: &[[u8; 16]],
        healths: &[Health],
        watch: Option<&Arc<Mutex<Watch>>>,
    ) -> Vec<Member> {
        let mut out = Vec::with_capacity(devs.len());
        for (i, dev) in devs.into_iter().enumerate() {
            let pages = dev.as_deref().map_or(0, BlockDev::blocks);
            let dev = match watch {
                Some(w) => dev.map(|d| Watch::wrap(w, i, d)),
                None => dev,
            };
            out.push(Member {
                unit_uuid: uuids[i],
                dev,
                health: healths[i],
                pages,
            });
        }
        out
    }

    /// At open, once the others' record is read: a member recorded in
    /// step and missing now, that the others did not record failed, was
    /// in step when the set last stopped but for what was in flight.
    fn missing_maps(engine: &Engine, g: &mut Watch, recorded: &[Health], failed: u32) {
        for (i, m) in engine.members.iter().enumerate() {
            g.health[i] = m.health;
        }
        for (i, m) in engine.members.iter().enumerate() {
            if m.dev.is_none() && recorded[i] == Health::Ok && failed & (1 << i) == 0 {
                g.left(i);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// After the engine ran: each member's map follows its health.
    fn settle(&self, e: &Engine) {
        if let Some(w) = &self.away {
            let health: Vec<Health> = e.members.iter().map(|m| m.health).collect();
            away::lock(w).settle(&health);
        }
    }

    /// The layout, which the caller records.
    #[must_use]
    pub fn geometry(&self) -> Geometry {
        self.geo
    }

    /// The members now, in order: what the caller records.
    #[must_use]
    pub fn members(&self) -> Vec<ParityMemberInfo> {
        self.lock()
            .members
            .iter()
            .map(|m| ParityMemberInfo {
                uuid: m.unit_uuid,
                health: m.health,
                attached: m.dev.is_some(),
            })
            .collect()
    }

    /// The regions of the map kept for `member` while it is out of
    /// service or being rebuilt from it; `None` when none is.
    #[must_use]
    pub fn away_regions(&self, member: usize) -> Option<Vec<u64>> {
        let w = self.away.as_ref()?;
        if member >= self.geo.n {
            return None;
        }
        away::lock(w).regions(member)
    }

    /// Events since the last call.
    #[must_use]
    pub fn drain_events(&self) -> Vec<SetEvent> {
        let mut out =
            std::mem::take(&mut *self.events.lock().unwrap_or_else(PoisonError::into_inner));
        for e in self.lock().drain_events() {
            out.push(match e {
                EngineEvent::Failed { member, what } => SetEvent::Failed { member, what },
                EngineEvent::Healed { member, lba } => SetEvent::Healed { member, lba },
            });
        }
        out
    }

    /// The work to run next: a rebuild before the synchronization.
    #[must_use]
    pub fn work(&self) -> Option<ParityWork> {
        let e = self.lock();
        for (i, m) in e.members.iter().enumerate() {
            if let Health::Rebuilding(done) = m.health
                && m.dev.is_some()
            {
                return Some(ParityWork::Rebuild(i, done));
            }
        }
        e.sync.map(ParityWork::Sync)
    }

    /// Run up to `budget` blocks of the work that is due; `None` when
    /// there is none. The caller records the members and the mark as it
    /// goes, and again when a piece of work completes. A member rebuilt
    /// from its map skips the blocks outside it, so its mark jumps.
    pub fn step(&self, budget: u64) -> Result<Option<ParityProgress>, SetError> {
        let Some(work) = self.work() else {
            return Ok(None);
        };
        let mut e = self.lock();
        let mapped = match (&self.away, work) {
            (Some(w), ParityWork::Rebuild(m, _)) => away::lock(w).keeping(m),
            _ => false,
        };
        let r = match work {
            ParityWork::Sync(_) => e.sync_step(budget).map(ParityWork::Sync),
            ParityWork::Rebuild(m, _) if mapped => self
                .mapped_step(&mut e, m, budget)
                .map(|d| ParityWork::Rebuild(m, d)),
            ParityWork::Rebuild(m, _) => {
                e.rebuild_step(m, budget).map(|d| ParityWork::Rebuild(m, d))
            }
        };
        self.settle(&e);
        let work = r?;
        let (ParityWork::Sync(done) | ParityWork::Rebuild(_, done)) = work;
        let complete = done >= self.geo.blocks;
        if complete {
            info!("parity set: {work:?} complete");
        }
        Ok(Some(ParityProgress {
            work,
            blocks: self.geo.blocks,
            complete,
        }))
    }

    /// Rebuild up to `budget` blocks of `member` in its map's regions
    /// from its mark; the blocks between are in step. Returns the mark.
    fn mapped_step(&self, e: &mut Engine, member: usize, budget: u64) -> Result<u64, BlockError> {
        let w = self.away.as_ref().expect("maps kept");
        let Health::Rebuilding(mark) = e.members[member].health else {
            return Ok(self.geo.blocks);
        };
        // Its old slots name stripes it was not in step for; the engine
        // zeroes them at mark 0, and the mark is about to pass 0.
        if !away::lock(w).zeroed[member] {
            e.flush()?;
            let dev = e.members[member]
                .dev
                .as_deref()
                .ok_or_else(|| no_member("rebuild", 0))?;
            let zero = vec![0u8; PAGE_SIZE];
            for i in 0..2 {
                dev.write(self.geo.slot(i), &zero)?;
            }
            dev.flush()?;
            away::lock(w).zeroed[member] = true;
        }
        let next = away::lock(w).next_blocks(member, mark);
        let Some((start, stop)) = next else {
            return Ok(Self::rebuilt(e, member, self.geo.blocks));
        };
        e.members[member].health = Health::Rebuilding(start);
        let end = e.rebuild_step(member, budget.max(1).min(stop - start))?;
        let more = away::lock(w).next_blocks(member, end).is_some();
        if end < self.geo.blocks && !more {
            return Ok(Self::rebuilt(e, member, self.geo.blocks));
        }
        Ok(end)
    }

    /// A member rebuilt from its map is in step, which the others record.
    fn rebuilt(e: &mut Engine, member: usize, blocks: u64) -> u64 {
        e.members[member].health = Health::Ok;
        e.record_failed();
        blocks
    }

    /// The synchronization mark: `None` when the parity is computed.
    #[must_use]
    pub fn sync(&self) -> Option<u64> {
        self.lock().sync
    }

    /// Take a member's device away (it was pulled, or is to be). The
    /// set must be able to go on without it.
    pub fn detach(&self, member: usize) -> Result<Option<Box<dyn BlockDev>>, SetError> {
        if member >= self.geo.n {
            return Err(SetError::refused(member, "no such member"));
        }
        let mut e = self.lock();
        let Some(w) = &self.away else {
            return Ok(e.remove_member(member)?);
        };
        let flushed = e.flush();
        self.settle(&e);
        flushed?;
        away::lock(w).flushed();
        if e.members[member].health != Health::Failed {
            away::lock(w).left(member);
        }
        let out = e.remove_member(member);
        self.settle(&e);
        let watched = out?;
        Ok(watched.and_then(|_| away::lock(w).take_dev(member)))
    }

    /// Put a device in the place of a failed member; it is rebuilt by
    /// [`step`](Self::step). The caller records its identity.
    pub fn replace(
        &self,
        member: usize,
        dev: Box<dyn BlockDev>,
        uuid: [u8; 16],
    ) -> Result<(), SetError> {
        if member >= self.geo.n {
            return Err(SetError::refused(member, "no such member"));
        }
        let mut e = self.lock();
        if e.members[member].health != Health::Failed {
            return Err(SetError::refused(member, "not failed"));
        }
        let pages = dev.blocks();
        if pages <= self.geo.end() {
            return Err(SetError::refused(
                member,
                format!("{pages} blocks, the layout needs {}", self.geo.end() + 1),
            ));
        }
        let dev = match &self.away {
            Some(w) => {
                away::lock(w).replaced(member, uuid);
                Watch::wrap(w, member, dev)
            }
            None => dev,
        };
        let m = &mut e.members[member];
        m.dev = Some(dev);
        m.health = Health::Rebuilding(0);
        m.unit_uuid = uuid;
        m.pages = pages;
        info!("parity set: member {member} replaced; rebuilding");
        self.settle(&e);
        Ok(())
    }

    /// The members a hot spare could replace now.
    #[must_use]
    pub fn failed(&self) -> Vec<usize> {
        self.lock()
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.health == Health::Failed)
            .map(|(i, _)| i)
            .collect()
    }

    /// Give the first failed member the smallest hot spare that fits.
    /// The member it replaced and its new identity; the caller records
    /// them and runs the rebuild.
    pub fn take_spare(&self, spares: &mut Vec<Spare>) -> Option<(usize, [u8; 16])> {
        let member = *self.failed().first()?;
        let s = spare::take(spares, self.geo.end() + 1)?;
        let uuid = s.uuid;
        match self.replace(member, s.dev, uuid) {
            Ok(()) => Some((member, uuid)),
            Err(e) => {
                warn!("parity set: hot spare not used: {e}");
                None
            }
        }
    }

    /// Stripes whose parity does not match their data.
    pub fn verify(&self) -> Result<Vec<u64>, SetError> {
        let mut e = self.lock();
        let r = e.verify();
        self.settle(&e);
        Ok(r?)
    }

    /// The member and its block that hold block `lba` of the set.
    fn locate(&self, lba: u64) -> (usize, u64) {
        (
            usize::try_from(lba / self.rows).expect("few members"),
            self.geo.head + lba % self.rows,
        )
    }

    /// The regions blocks `lba..lba + count` of the set touch: a run,
    /// first and last, for each member's rows they cross.
    fn regions_of(&self, lba: u64, count: u64, rb: u64) -> Vec<(u64, u64)> {
        let w = self.geo.w() as u64;
        let mut out = Vec::new();
        let (mut at, end) = (lba, lba + count);
        while at < end {
            let stop = end.min((at / self.rows + 1) * self.rows);
            let first = (at % self.rows) / w;
            let last = ((stop - 1) % self.rows) / w;
            out.push((first / rb, last / rb));
            at = stop;
        }
        out
    }
}

impl BlockDev for ParitySet {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.rows * self.geo.n as u64
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks(), lba, buf.len())?;
        let mut e = self.lock();
        let mut r = Ok(());
        for (i, chunk) in (0..count).zip(buf.chunks_mut(PAGE_SIZE)) {
            let (member, at) = self.locate(lba + i);
            match e.read_page(member, at) {
                Ok(p) => chunk.copy_from_slice(&p),
                Err(x) => {
                    r = Err(x);
                    break;
                }
            }
        }
        self.settle(&e);
        r
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks(), lba, buf.len())?;
        let mut e = self.lock();
        if let Some(w) = &self.away {
            let mut g = away::lock(w);
            let regions = self.regions_of(lba, count, g.region_blocks());
            g.begin(&regions).map_err(|source| BlockError::Io {
                op: "write-intent",
                lba,
                source,
            })?;
        }
        let mut r = Ok(());
        for (i, chunk) in (0..count).zip(buf.chunks(PAGE_SIZE)) {
            let (member, at) = self.locate(lba + i);
            if let Err(x) = e.write_page(member, at, chunk) {
                r = Err(x);
                break;
            }
        }
        self.settle(&e);
        r
    }

    fn flush(&self) -> blockdev::Result<()> {
        let mut e = self.lock();
        let r = e.flush();
        self.settle(&e);
        if r.is_ok()
            && let Some(w) = &self.away
        {
            away::lock(w).flushed();
        }
        r
    }
}

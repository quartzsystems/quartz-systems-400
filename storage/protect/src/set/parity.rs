// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parity set as one block device: the members' rows end to end.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use blockdev::{BLOCK_SIZE, BlockDev, check_range};
use tracing::{info, warn};

use crate::PAGE_SIZE;
use crate::parity::{Engine, EngineEvent, Format, Geometry, Health, Level, MAX_MEMBERS, Member};
use crate::set::{SetError, SetEvent};
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
        Self::open(fmt, id, geo, members, Some(0))
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
        if members.len() != geo.n {
            return Err(SetError::Unusable(format!(
                "{} members for a layout of {}",
                members.len(),
                geo.n
            )));
        }
        let mut events = Vec::new();
        let mut engine_members = Vec::with_capacity(members.len());
        for (i, m) in members.into_iter().enumerate() {
            if let Some(d) = &m.dev
                && d.blocks() <= geo.end()
            {
                return Err(SetError::refused(
                    i,
                    format!("{} blocks, the layout needs {}", d.blocks(), geo.end() + 1),
                ));
            }
            let health = match (m.health, m.dev.is_some()) {
                (Health::Ok, true) => Health::Ok,
                (Health::Ok, false) => {
                    if sync.is_some() {
                        return Err(SetError::refused(
                            i,
                            "missing while the set is synchronizing",
                        ));
                    }
                    events.push(SetEvent::Missing { member: i });
                    Health::Failed
                }
                (Health::Rebuilding(mark), true) => Health::Rebuilding(mark),
                (Health::Failed, true) => {
                    events.push(SetEvent::Stale { member: i });
                    Health::Rebuilding(0)
                }
                (Health::Rebuilding(_) | Health::Failed, false) => Health::Failed,
            };
            let pages = m.dev.as_deref().map_or(0, BlockDev::blocks);
            engine_members.push(Member {
                unit_uuid: m.uuid,
                dev: m.dev,
                health,
                pages,
            });
        }
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
        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            geo,
            rows: geo.rows_end() - geo.head,
            events: Arc::new(Mutex::new(events)),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Engine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
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
    /// goes, and again when a piece of work completes.
    pub fn step(&self, budget: u64) -> Result<Option<ParityProgress>, SetError> {
        let Some(work) = self.work() else {
            return Ok(None);
        };
        let mut e = self.lock();
        let work = match work {
            ParityWork::Sync(_) => ParityWork::Sync(e.sync_step(budget)?),
            ParityWork::Rebuild(m, _) => ParityWork::Rebuild(m, e.rebuild_step(m, budget)?),
        };
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
        Ok(self.lock().remove_member(member)?)
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
        let m = &mut e.members[member];
        m.dev = Some(dev);
        m.health = Health::Rebuilding(0);
        m.unit_uuid = uuid;
        m.pages = pages;
        info!("parity set: member {member} replaced; rebuilding");
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
        Ok(self.lock().verify()?)
    }

    /// The member and its block that hold block `lba` of the set.
    fn locate(&self, lba: u64) -> (usize, u64) {
        (
            usize::try_from(lba / self.rows).expect("few members"),
            self.geo.head + lba % self.rows,
        )
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
        for (i, chunk) in (0..count).zip(buf.chunks_mut(PAGE_SIZE)) {
            let (member, at) = self.locate(lba + i);
            chunk.copy_from_slice(&e.read_page(member, at)?);
        }
        Ok(())
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks(), lba, buf.len())?;
        let mut e = self.lock();
        for (i, chunk) in (0..count).zip(buf.chunks(PAGE_SIZE)) {
            let (member, at) = self.locate(lba + i);
            e.write_page(member, at, chunk)?;
        }
        Ok(())
    }

    fn flush(&self) -> blockdev::Result<()> {
        self.lock().flush()
    }
}

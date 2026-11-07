// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Device parity protection in software: RAID 5 and RAID 6 sets of
//! units, kept by the store as IBM i's RAID controllers keep them. Each
//! member stays its own span; the I/O layer sees a [`ParityDev`] in
//! place of the member's device, and every page passes through the
//! set's [`Engine`]. See `docs/STORE_FORMAT.md` §11.

pub mod engine;
pub mod geometry;
pub mod gf;
pub mod ops;
pub mod ppl;
pub mod table;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use blockdev::{BLOCK_SIZE, BlockDev, check_range};

use crate::alloc::Allocator;
use crate::asp::UnitEvent;
use crate::asp::io::UnitDev;
use crate::asp::layout::Layout;
use crate::asp::superblock::Superblock;
use crate::page::PAGE_SIZE;
use crate::{Result, StoreError};
pub use engine::{Engine, EngineEvent, Fault, Health, Member};
pub use geometry::Geometry;
pub use table::{Level, MemberState, ParityTable, ParityTag, ParityWork, WorkKind};

/// A member's device as the I/O layer sees it.
pub struct ParityDev {
    engine: Arc<Mutex<Engine>>,
    member: usize,
    blocks: u64,
}

fn lock(e: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(PoisonError::into_inner)
}

impl BlockDev for ParityDev {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let mut e = lock(&self.engine);
        for (i, chunk) in (0..count).zip(buf.chunks_mut(PAGE_SIZE)) {
            chunk.copy_from_slice(&e.read_page(self.member, lba + i)?);
        }
        Ok(())
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let mut e = lock(&self.engine);
        for (i, chunk) in (0..count).zip(buf.chunks(PAGE_SIZE)) {
            e.write_page(self.member, lba + i, chunk)?;
        }
        Ok(())
    }

    fn flush(&self) -> blockdev::Result<()> {
        lock(&self.engine).flush()
    }
}

/// One set at run time.
pub struct SetHandle {
    /// Set number.
    pub set: u8,
    /// The engine its members' devices share.
    pub engine: Arc<Mutex<Engine>>,
}

impl SetHandle {
    /// The engine, locked.
    pub fn lock(&self) -> MutexGuard<'_, Engine> {
        lock(&self.engine)
    }

    /// A member's device for the I/O layer.
    #[must_use]
    pub fn device(&self, member: usize, blocks: u64) -> Box<dyn BlockDev> {
        Box::new(ParityDev {
            engine: Arc::clone(&self.engine),
            member,
            blocks,
        })
    }
}

/// The sets of an ASP.
#[derive(Default)]
pub struct ParitySets {
    /// By set number.
    pub sets: Vec<SetHandle>,
}

impl ParitySets {
    /// A set.
    #[must_use]
    pub fn get(&self, set: u8) -> Option<&SetHandle> {
        self.sets.iter().find(|s| s.set == set)
    }

    /// Whether a member's unit is attached; true for units in no set.
    #[must_use]
    pub fn attached(&self, unit_uuid: &[u8; 16]) -> bool {
        for s in &self.sets {
            let e = s.lock();
            if let Some(m) = e.members.iter().find(|m| &m.unit_uuid == unit_uuid) {
                return m.dev.is_some();
            }
        }
        true
    }

    /// The work to run next: per set in order, a rebuild before a
    /// synchronization.
    #[must_use]
    pub fn current_work(&self) -> Option<ParityWork> {
        for s in &self.sets {
            let e = s.lock();
            for (i, m) in e.members.iter().enumerate() {
                if let Health::Rebuilding(done) = m.health
                    && m.dev.is_some()
                {
                    return Some(ParityWork {
                        set: s.set,
                        kind: WorkKind::Rebuild,
                        member: u8::try_from(i).expect("small"),
                        done,
                    });
                }
            }
            if let Some(done) = e.sync {
                return Some(ParityWork {
                    set: s.set,
                    kind: WorkKind::Sync,
                    member: 0,
                    done,
                });
            }
        }
        None
    }

    /// Copy the members' states and the work into the superblock.
    pub fn publish(&self, sb: &mut Superblock) {
        for s in &self.sets {
            let e = s.lock();
            for m in &e.members {
                if let Some(t) = sb.parity.tag_mut(&m.unit_uuid) {
                    t.state = match m.health {
                        Health::Ok if e.sync.is_some() => MemberState::Syncing,
                        Health::Ok => MemberState::Active,
                        Health::Rebuilding(_) => MemberState::Rebuilding,
                        Health::Failed => MemberState::Failed,
                    };
                }
            }
        }
        sb.parity.work = self.current_work();
    }
}

/// The unit number of a UUID in the table.
pub(crate) fn unit_of(sb: &Superblock, uuid: &[u8; 16]) -> Option<u16> {
    sb.units
        .iter()
        .position(|u| &u.unit_uuid == uuid)
        .map(|i| u16::try_from(i).expect("table size"))
}

/// The geometry of a set from the table: its level, member count and
/// smallest member.
pub(crate) fn geometry_of(sb: &Superblock, set: u8) -> Result<Geometry> {
    let members = sb.parity.members(set);
    let bad = |what: String| StoreError::Corrupt { page: 1, what };
    let level = members
        .first()
        .map(|t| t.level)
        .ok_or_else(|| bad(format!("parity set {set} is empty")))?;
    let mut smallest = u64::MAX;
    for (i, t) in members.iter().enumerate() {
        if usize::from(t.member) != i || t.level != level {
            return Err(bad(format!(
                "parity set {set}: member numbers or levels disagree"
            )));
        }
        let unit = unit_of(sb, &t.unit_uuid)
            .ok_or_else(|| bad(format!("parity set {set}: a member is not in the table")))?;
        smallest = smallest.min(sb.units[usize::from(unit)].usable_pages);
    }
    Geometry::new(level, members.len(), smallest)
        .ok_or_else(|| bad(format!("parity set {set}: members too small or too few")))
}

/// At open, before anything reads the units: build each set's engine
/// over the devices found, decide what a missing or returned member
/// means, repair the last epoch, and put the members' devices in the
/// I/O layer's place.
pub(crate) fn attach(
    units: &mut [UnitDev],
    sb: &mut Superblock,
) -> Result<(ParitySets, Vec<UnitEvent>)> {
    let mut sets = ParitySets::default();
    let mut events = Vec::new();
    let work = sb.parity.work;
    for set in sb.parity.sets() {
        let geo = geometry_of(sb, set)?;
        let tags: Vec<ParityTag> = sb.parity.members(set).into_iter().cloned().collect();
        let mut members = Vec::with_capacity(tags.len());
        let mut sync = None;
        for (i, t) in tags.iter().enumerate() {
            let unit = unit_of(sb, &t.unit_uuid).expect("checked");
            let dev = units[usize::from(unit)].dev.take();
            let mark = |kind: WorkKind| {
                work.filter(|w| {
                    w.set == set
                        && w.kind == kind
                        && (kind == WorkKind::Sync || usize::from(w.member) == i)
                })
                .map_or(0, |w| w.done)
            };
            let missing = || StoreError::UnitMissing {
                unit,
                what: format!("serial {}", sb.units[usize::from(unit)].serial_text()),
            };
            let health = match (t.state, dev.is_some()) {
                (MemberState::Active | MemberState::Syncing, true) => Health::Ok,
                (MemberState::Syncing, false) => return Err(missing()),
                (MemberState::Active, false) => {
                    events.push(UnitEvent::ParityExposed { unit });
                    Health::Failed
                }
                (MemberState::Rebuilding, true) => Health::Rebuilding(mark(WorkKind::Rebuild)),
                (MemberState::Failed, true) => {
                    events.push(UnitEvent::ParityStale { unit });
                    Health::Rebuilding(0)
                }
                (MemberState::Rebuilding | MemberState::Failed, false) => Health::Failed,
            };
            if t.state == MemberState::Syncing {
                sync = Some(mark(WorkKind::Sync));
            }
            members.push(Member {
                unit_uuid: t.unit_uuid,
                dev,
                health,
                pages: sb.units[usize::from(unit)].pages,
            });
        }
        let mut engine = Engine::new(sb.asp_uuid, set, geo, members, sync);
        // A member that failed after the last checkpoint is recorded on
        // the others: it is stale even though the table says active.
        let failed = engine.recorded_failures();
        for (i, t) in tags.iter().enumerate() {
            let m = &mut engine.members[i];
            if failed & (1 << i) != 0 && t.state == MemberState::Active && m.dev.is_some() {
                m.health = Health::Rebuilding(0);
                events.push(UnitEvent::ParityStale {
                    unit: unit_of(sb, &t.unit_uuid).expect("checked"),
                });
            }
        }
        let members = &engine.members;
        let unknown = members.iter().filter(|m| m.health != Health::Ok).count();
        if unknown > geo.k() {
            let lost = members
                .iter()
                .find(|m| m.health == Health::Failed)
                .or_else(|| members.iter().find(|m| m.health != Health::Ok))
                .and_then(|m| unit_of(sb, &m.unit_uuid))
                .unwrap_or(0);
            return Err(StoreError::UnitMissing {
                unit: lost,
                what: format!(
                    "parity set {set} has {unknown} units out of step; it can lose {}",
                    geo.k()
                ),
            });
        }
        engine.recover().map_err(StoreError::from)?;
        let handle = SetHandle {
            set,
            engine: Arc::new(Mutex::new(engine)),
        };
        for (i, t) in tags.iter().enumerate() {
            let unit = unit_of(sb, &t.unit_uuid).expect("checked");
            let pages = sb.units[usize::from(unit)].pages;
            units[usize::from(unit)].dev = Some(handle.device(i, pages));
        }
        sets.sets.push(handle);
    }
    sets.publish(sb);
    Ok((sets, events))
}

/// Hold the tail of every member: parity, slots and any rest.
pub(crate) fn hold_all(sb: &Superblock, layout: &Layout, alloc: &mut Allocator) -> Result<()> {
    for set in sb.parity.sets() {
        let geo = geometry_of(sb, set)?;
        for t in sb.parity.members(set) {
            let unit = unit_of(sb, &t.unit_uuid).expect("checked");
            let span = layout.span_of_unit(unit).expect("every unit has a span");
            let pages = layout.span(span).pages;
            alloc.hold(span, geo.rows_end(), pages - 1 - geo.rows_end());
        }
    }
    Ok(())
}

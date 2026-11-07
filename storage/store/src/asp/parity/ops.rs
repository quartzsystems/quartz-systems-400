// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Forming and dissolving sets, including and excluding a unit,
//! replacing a failed one, the background work, and what the displays
//! show. Every change here is published by the caller's checkpoint.

use std::sync::{Arc, Mutex};

use blockdev::BlockDev;
use tracing::info;

use crate::asp::Asp;
use crate::asp::io::IoEvent;
use crate::asp::parity::{
    Engine, EngineEvent, Fault, Geometry, Health, Level, Member, MemberState, ParityTag, SetHandle,
    WorkKind, geometry_of, table::MAX_MEMBERS, unit_of,
};
use crate::asp::superblock::serial_bytes;
use crate::asp::unit::{UnitHeader, UnitState};
use crate::obj::ObjectId;
use crate::page::{Page, PageType};
use crate::{Refusal, Result, StoreError};

/// A member as the displays show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    /// Unit number.
    pub unit: u16,
    /// Position in the set.
    pub member: usize,
    /// State in the unit table.
    pub state: MemberState,
    /// Whether the unit is attached.
    pub attached: bool,
    /// IBM's status text: `DPY/Active`, `DPY/Failed`, ...
    pub status: &'static str,
    /// Percent rebuilt, while a rebuild runs on it.
    pub rebuild_percent: Option<u8>,
}

/// A set as the displays show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetInfo {
    /// Set number.
    pub set: u8,
    /// Level.
    pub level: Level,
    /// The layout.
    pub geometry: Geometry,
    /// Members in order.
    pub members: Vec<MemberInfo>,
    /// Percent synchronized, while the set is being synchronized.
    pub sync_percent: Option<u8>,
}

/// How far the background work has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParityProgress {
    /// The set.
    pub set: u8,
    /// Sync or rebuild.
    pub kind: WorkKind,
    /// The unit rebuilt.
    pub unit: Option<u16>,
    /// Blocks done.
    pub done: u64,
    /// Blocks in the set.
    pub blocks: u64,
    /// Percent.
    pub percent: u8,
    /// This piece of work is finished.
    pub complete: bool,
}

fn percent(done: u64, blocks: u64) -> u8 {
    u8::try_from((done * 100).checked_div(blocks).unwrap_or(100)).unwrap_or(100)
}

fn no_set(set: u8) -> StoreError {
    StoreError::Refused {
        unit: None,
        reason: Refusal::NoParitySet(set),
    }
}

fn not_active(set: u8) -> StoreError {
    StoreError::Refused {
        unit: None,
        reason: Refusal::ParitySetNotActive(set),
    }
}

impl Asp {
    /// The set a unit belongs to.
    #[must_use]
    pub fn parity_set_of(&self, unit: u16) -> Option<u8> {
        let e = self.sb.units.get(usize::from(unit))?;
        self.sb.parity.tag(&e.unit_uuid).map(|t| t.set)
    }

    /// Pages of a unit the set holds for parity.
    #[must_use]
    pub fn parity_held(&self, unit: u16) -> u64 {
        let (Some(set), Some(span)) = (self.parity_set_of(unit), self.layout().span_of_unit(unit))
        else {
            return 0;
        };
        geometry_of(&self.sb, set).map_or(0, |geo| {
            self.layout()
                .span(span)
                .pages
                .saturating_sub(1 + geo.rows_end())
        })
    }

    /// Every set, for the displays.
    #[must_use]
    pub fn parity_sets(&self) -> Vec<SetInfo> {
        let mut out = Vec::new();
        for h in &self.parity.sets {
            let e = h.lock();
            let unknown = e.members.iter().filter(|m| m.health != Health::Ok).count();
            let members = e
                .members
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let status = match m.health {
                        Health::Failed => "DPY/Failed",
                        Health::Rebuilding(_) => "DPY/Rebuild",
                        Health::Ok if e.sync.is_some() => "DPY/Unprotected",
                        Health::Ok if unknown == 0 => "DPY/Active",
                        Health::Ok if unknown < e.geo.k() => "DPY/Degraded",
                        Health::Ok => "DPY/Unprotected",
                    };
                    MemberInfo {
                        unit: unit_of(&self.sb, &m.unit_uuid).unwrap_or(u16::MAX),
                        member: i,
                        state: self
                            .sb
                            .parity
                            .tag(&m.unit_uuid)
                            .map_or(MemberState::Failed, |t| t.state),
                        attached: m.dev.is_some(),
                        status,
                        rebuild_percent: match m.health {
                            Health::Rebuilding(d) => Some(percent(d, e.geo.blocks)),
                            _ => None,
                        },
                    }
                })
                .collect();
            out.push(SetInfo {
                set: h.set,
                level: e.geo.level,
                geometry: e.geo,
                members,
                sync_percent: e.sync.map(|d| percent(d, e.geo.blocks)),
            });
        }
        out
    }

    /// Check a unit can join a set.
    fn eligible(&self, unit: u16) -> Result<()> {
        let e = self
            .sb
            .units
            .get(usize::from(unit))
            .ok_or_else(|| StoreError::UnitMissing {
                unit,
                what: "no such unit".into(),
            })?;
        if e.mirrored() {
            return Err(StoreError::refused(unit, Refusal::Mirrored));
        }
        if let Some(t) = self.sb.parity.tag(&e.unit_uuid) {
            return Err(StoreError::refused(unit, Refusal::InParitySet(t.set)));
        }
        if self.io.unit_state(unit) != UnitState::Unprotected || !self.io.unit_attached(unit) {
            return Err(StoreError::refused(unit, Refusal::NotAttached));
        }
        Ok(())
    }

    /// The geometry for these units, checking that the pages parity is
    /// to take hold no data. The set uses the smallest unit's size; a
    /// larger unit's rest is held unused, as IBM's controllers do.
    fn plan_set(&self, level: Level, units: &[u16]) -> Result<Geometry> {
        if units.len() < level.min_units() || units.len() > MAX_MEMBERS {
            return Err(StoreError::Refused {
                unit: None,
                reason: Refusal::ParityUnits {
                    level: if level == Level::Raid6 {
                        "RAID 6"
                    } else {
                        "RAID 5"
                    },
                    least: level.min_units(),
                    most: MAX_MEMBERS,
                    got: units.len(),
                },
            });
        }
        let smallest = units
            .iter()
            .map(|&u| self.sb.units[usize::from(u)].usable_pages)
            .min()
            .unwrap_or(0);
        let geo = Geometry::new(level, units.len(), smallest).ok_or_else(|| {
            StoreError::UnitTooSmall {
                pages: smallest,
                need: 1 + 3 + 2 * crate::asp::parity::geometry::SLOT_PAGES + units.len() as u64,
            }
        })?;
        for &u in units {
            let span = self.span_of(u)?;
            let pages = self.layout().span(span).pages;
            let used = self
                .alloc
                .in_use(span, geo.rows_end(), pages - 1 - geo.rows_end());
            if used > 0 {
                return Err(StoreError::refused(u, Refusal::ParityShareInUse(used)));
            }
            if self.ifs_in_span(span, geo.rows_end()) {
                return Err(StoreError::refused(u, Refusal::HoldsIfs));
            }
        }
        Ok(geo)
    }

    /// Form a set over raw devices now in the I/O layer, synchronizing.
    fn form(&mut self, set: u8, level: Level, units: &[u16], geo: Geometry) {
        let mut members = Vec::with_capacity(units.len());
        for (i, &u) in units.iter().enumerate() {
            let entry = self.sb.units[usize::from(u)].clone();
            let dev = self.io.detach(u, UnitState::Unprotected);
            members.push(Member {
                unit_uuid: entry.unit_uuid,
                dev,
                health: Health::Ok,
                pages: entry.pages,
            });
            self.sb.parity.tags.push(ParityTag {
                unit_uuid: entry.unit_uuid,
                set,
                member: u8::try_from(i).expect("at most 18"),
                level,
                state: MemberState::Syncing,
            });
            let span = self.layout().span_of_unit(u).expect("unit has a span");
            let pages = self.layout().span(span).pages;
            self.alloc
                .hold(span, geo.rows_end(), pages - 1 - geo.rows_end());
        }
        let engine = Engine::new(self.sb.asp_uuid, set, geo, members, Some(0));
        let handle = SetHandle {
            set,
            engine: Arc::new(Mutex::new(engine)),
        };
        for (i, &u) in units.iter().enumerate() {
            let pages = self.sb.units[usize::from(u)].pages;
            self.io
                .attach(u, handle.device(i, pages), UnitState::Unprotected);
        }
        let at = self.parity.sets.partition_point(|s| s.set < set);
        self.parity.sets.insert(at, handle);
        self.sb.update_protection();
    }

    /// Dissolve a set: the held writes made, the raw devices back in the
    /// I/O layer, the held pages released at the next checkpoint.
    /// Returns the members' units in order.
    fn dissolve(&mut self, set: u8) -> Result<Vec<u16>> {
        let at = self
            .parity
            .sets
            .iter()
            .position(|s| s.set == set)
            .ok_or_else(|| no_set(set))?;
        let geo = geometry_of(&self.sb, set)?;
        let handle = self.parity.sets.remove(at);
        let (uuids, devs) = {
            let mut e = handle.lock();
            let uuids: Vec<[u8; 16]> = e.members.iter().map(|m| m.unit_uuid).collect();
            (uuids, e.take_devices()?)
        };
        let mut units = Vec::new();
        for (uuid, dev) in uuids.iter().zip(devs) {
            let unit = unit_of(&self.sb, uuid).expect("member in table");
            let span = self.span_of(unit)?;
            let pages = self.layout().span(span).pages;
            self.alloc
                .unhold(span, geo.rows_end(), pages - 1 - geo.rows_end());
            match dev {
                Some(d) => self.io.attach(unit, d, UnitState::Unprotected),
                None => {
                    let _ = self.io.detach(unit, UnitState::Missing);
                }
            }
            units.push(unit);
        }
        self.sb.parity.tags.retain(|t| t.set != set);
        if self.sb.parity.work.is_some_and(|w| w.set == set) {
            self.sb.parity.work = None;
        }
        self.sb.update_protection();
        Ok(units)
    }

    /// Whether every member of a set is in step and the set is not
    /// being synchronized.
    fn set_healthy(&self, set: u8) -> Result<()> {
        let h = self.parity.get(set).ok_or_else(|| no_set(set))?;
        let e = h.lock();
        if e.sync.is_some() || e.members.iter().any(|m| m.health != Health::Ok) {
            return Err(not_active(set));
        }
        Ok(())
    }

    /// Start device parity protection over `units`. The set
    /// synchronizes in the background ([`parity_step`](Self::parity_step))
    /// after the caller's checkpoint.
    pub fn start_parity(&mut self, level: Level, units: &[u16]) -> Result<u8> {
        let mut units = units.to_vec();
        units.sort_unstable();
        units.dedup();
        for &u in &units {
            self.eligible(u)?;
        }
        let set = self.sb.parity.free_set().ok_or(StoreError::Refused {
            unit: None,
            reason: Refusal::ParitySetsFull,
        })?;
        let geo = self.plan_set(level, &units)?;
        self.form(set, level, &units, geo);
        info!(
            "asp {}: parity set {set} ({level}) started over units {units:?}",
            self.sb.asp_number
        );
        Ok(set)
    }

    /// Stop device parity protection for a set. Every member must be in
    /// step (a synchronizing set may stop).
    pub fn stop_parity(&mut self, set: u8) -> Result<()> {
        {
            let h = self.parity.get(set).ok_or_else(|| no_set(set))?;
            let e = h.lock();
            if e.members.iter().any(|m| m.health != Health::Ok) {
                return Err(not_active(set));
            }
        }
        self.dissolve(set)?;
        info!("asp {}: parity set {set} stopped", self.sb.asp_number);
        Ok(())
    }

    /// Include a unit in an active set. The set is synchronized again
    /// over the new layout.
    pub fn include_parity(&mut self, set: u8, unit: u16) -> Result<()> {
        self.eligible(unit)?;
        self.set_healthy(set)?;
        let level = geometry_of(&self.sb, set)?.level;
        let mut units: Vec<u16> = self
            .sb
            .parity
            .members(set)
            .iter()
            .map(|t| unit_of(&self.sb, &t.unit_uuid).expect("in table"))
            .collect();
        units.push(unit);
        let geo = self.plan_set(level, &units)?;
        self.dissolve(set)?;
        self.form(set, level, &units, geo);
        info!(
            "asp {}: unit {unit} included in parity set {set}",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Exclude a unit from its set; it stays in the ASP unprotected. The
    /// rest are synchronized again over the new layout.
    pub fn exclude_parity(&mut self, unit: u16) -> Result<()> {
        let set = self
            .parity_set_of(unit)
            .ok_or_else(|| StoreError::refused(unit, Refusal::NotInParitySet))?;
        self.set_healthy(set)?;
        let level = geometry_of(&self.sb, set)?.level;
        let rest: Vec<u16> = self
            .sb
            .parity
            .members(set)
            .iter()
            .map(|t| unit_of(&self.sb, &t.unit_uuid).expect("in table"))
            .filter(|&u| u != unit)
            .collect();
        let geo = self.plan_set(level, &rest)?;
        self.dissolve(set)?;
        self.form(set, level, &rest, geo);
        info!(
            "asp {}: unit {unit} excluded from parity set {set}",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Put a new device in the place of a failed member. It is rebuilt
    /// in the background after the caller's checkpoint.
    pub fn replace_parity_unit(
        &mut self,
        unit: u16,
        dev: Box<dyn BlockDev>,
        unit_uuid: [u8; 16],
        serial: &str,
    ) -> Result<()> {
        let set = self
            .parity_set_of(unit)
            .ok_or_else(|| StoreError::refused(unit, Refusal::NotInParitySet))?;
        let entry = self.sb.units[usize::from(unit)].clone();
        let handle = self.parity.get(set).expect("set exists");
        let member = {
            let e = handle.lock();
            let i = e
                .members
                .iter()
                .position(|m| m.unit_uuid == entry.unit_uuid)
                .expect("member");
            if e.members[i].health != Health::Failed {
                return Err(StoreError::refused(unit, Refusal::NotFailed));
            }
            i
        };
        let pages = dev.blocks();
        if pages < entry.usable_pages {
            return Err(StoreError::UnitTooSmall {
                pages,
                need: entry.usable_pages,
            });
        }
        let header = UnitHeader {
            unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: unit,
            pages,
            mirror_of: None,
        };
        write_header_raw(dev.as_ref(), &header)?;
        {
            let mut e = handle.lock();
            let m = &mut e.members[member];
            m.dev = Some(dev);
            m.health = Health::Rebuilding(0);
            m.unit_uuid = unit_uuid;
            m.pages = pages;
        }
        let device = handle.device(member, pages);
        self.io.attach(unit, device, UnitState::Unprotected);
        if let Some(t) = self.sb.parity.tag_mut(&entry.unit_uuid) {
            t.unit_uuid = unit_uuid;
            t.state = MemberState::Rebuilding;
        }
        let e = &mut self.sb.units[usize::from(unit)];
        e.unit_uuid = unit_uuid;
        e.pages = pages;
        e.serial = serial_bytes(serial);
        info!(
            "asp {}: unit {unit} of parity set {set} replaced; rebuilding",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Run up to `budget` blocks of the synchronization or rebuild that
    /// is due. `None` when there is none. The caller checkpoints when a
    /// piece of work completes.
    pub fn parity_step(&mut self, budget: u64) -> Result<Option<ParityProgress>> {
        let Some(w) = self.parity.current_work() else {
            return Ok(None);
        };
        let handle = self.parity.get(w.set).expect("work names a set");
        let (done, blocks, unit) = {
            let mut e = handle.lock();
            let done = match w.kind {
                WorkKind::Sync => e.sync_step(budget)?,
                WorkKind::Rebuild => e.rebuild_step(usize::from(w.member), budget)?,
            };
            let unit = (w.kind == WorkKind::Rebuild)
                .then(|| unit_of(&self.sb, &e.members[usize::from(w.member)].unit_uuid))
                .flatten();
            (done, e.geo.blocks, unit)
        };
        let complete = done >= blocks;
        if complete {
            self.parity.publish(&mut self.sb);
            info!(
                "asp {}: parity set {} {}",
                self.sb.asp_number,
                w.set,
                match w.kind {
                    WorkKind::Sync => "synchronized",
                    WorkKind::Rebuild => "unit rebuilt",
                }
            );
        }
        Ok(Some(ParityProgress {
            set: w.set,
            kind: w.kind,
            unit,
            done,
            blocks,
            percent: percent(done, blocks),
            complete,
        }))
    }

    /// Arm or clear the test hook of a set.
    pub fn parity_fault(&mut self, set: u8, fault: Option<Fault>) {
        if let Some(h) = self.parity.get(set) {
            h.lock().fault = fault;
        }
    }

    /// Stripes of a set whose parity does not match their data.
    pub fn parity_verify(&mut self, set: u8) -> Result<Vec<u64>> {
        let h = self.parity.get(set).ok_or_else(|| no_set(set))?;
        Ok(h.lock().verify()?)
    }

    /// What the sets did on their own, as I/O events.
    pub fn drain_parity_events(&mut self) -> Vec<IoEvent> {
        let mut out = Vec::new();
        for h in &self.parity.sets {
            let mut e = h.lock();
            let uuids: Vec<[u8; 16]> = e.members.iter().map(|m| m.unit_uuid).collect();
            for ev in e.drain_events() {
                match ev {
                    EngineEvent::Failed { member, what } => {
                        let unit = unit_of(&self.sb, &uuids[member]).unwrap_or(u16::MAX);
                        out.push(IoEvent::UnitFailed { unit, what });
                    }
                    EngineEvent::Healed { member, lba } => {
                        let unit = unit_of(&self.sb, &uuids[member]).unwrap_or(u16::MAX);
                        let page = self
                            .layout()
                            .span_of_unit(unit)
                            .map_or(lba, |s| self.layout().logical(s, lba));
                        out.push(IoEvent::Healed { page, unit });
                    }
                }
            }
        }
        out
    }
}

/// A unit header at page 0 and the last page of a raw device, both
/// superblock slots zeroed.
fn write_header_raw(dev: &dyn BlockDev, header: &UnitHeader) -> Result<()> {
    let mut page = Page::zeroed();
    header.encode(page.body_mut());
    page.seal(PageType::UnitHeader, ObjectId::NONE, 0);
    dev.write(0, page.bytes())?;
    dev.write(header.pages - 1, page.bytes())?;
    let zero = Page::zeroed();
    dev.write(1, zero.bytes())?;
    dev.write(2, zero.bytes())?;
    dev.flush()?;
    Ok(())
}

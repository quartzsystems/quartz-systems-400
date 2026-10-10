// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An auxiliary storage pool: units (block devices) with a unit header
//! each and two superblock copies each, grouped into spans that make one
//! logical page space. A span is one unprotected unit or a mirrored
//! pair. See `docs/STORE_FORMAT.md` §4, §5 and §7.

pub mod io;
pub mod layout;
pub mod superblock;
pub mod unit;

use blockdev::BlockDev;
use tracing::{debug, info, warn};

use crate::alloc::{Allocator, Run, UnitStats};
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{Page, PageType};
use crate::{Result, StoreError};
use io::{AspIo, UnitDev};
use layout::{Layout, MAX_UNITS, RESERVED_HEAD};
use superblock::{Superblock, UnitEntry};
use unit::{UnitHeader, UnitState};

/// What `format` needs beyond the devices.
#[derive(Debug, Clone)]
pub struct FormatParams {
    /// ASP number (1 for SYSBAS, 0 for the temporary ASP).
    pub asp_number: u16,
    /// Identity of the ASP.
    pub asp_uuid: [u8; 16],
    /// One UUID per device, in order.
    pub unit_uuids: Vec<[u8; 16]>,
    /// The devices' serial numbers, in order; missing ones are blank.
    pub unit_serials: Vec<String>,
    /// WAL ring length in pages.
    pub wal_pages: u64,
    /// Install time, Unix seconds.
    pub now: u64,
}

/// Pages a unit must have beyond the WAL and the reserved pages.
pub const MIN_UNIT_SPARE: u64 = 64;

/// Something `open` found out about a unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitEvent {
    /// Half of a pair was not found; the pair runs on the other half.
    Missing {
        /// The unit.
        unit: u16,
    },
    /// A half recorded as missing or failed is back; its data is stale
    /// and it stays suspended until it is resumed.
    Stale {
        /// The unit.
        unit: u16,
    },
    /// A half is marked resuming: the copy from its partner must run.
    Resuming {
        /// The unit.
        unit: u16,
    },
}

/// A unit as the configuration displays see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitInfo {
    /// Unit number.
    pub unit: u16,
    /// The table entry.
    pub entry: UnitEntry,
    /// Pages of the span in use.
    pub used_pages: u64,
    /// Whether the device is attached now.
    pub attached: bool,
}

/// An open ASP.
pub struct Asp {
    io: AspIo,
    sb: Superblock,
    alloc: Allocator,
}

impl Asp {
    /// Write a new ASP over `devs` (in unit order) and open it. Any
    /// previous contents are lost.
    pub fn format(devs: Vec<Box<dyn BlockDev>>, params: &FormatParams) -> Result<Self> {
        if devs.is_empty() || devs.len() != params.unit_uuids.len() || devs.len() > MAX_UNITS {
            return Err(StoreError::Corrupt {
                page: 0,
                what: format!(
                    "{} devices for {} unit UUIDs",
                    devs.len(),
                    params.unit_uuids.len()
                ),
            });
        }
        let sizes: Vec<u64> = devs.iter().map(|d| d.as_ref().blocks()).collect();
        for (i, &pages) in sizes.iter().enumerate() {
            let need = min_pages(i == 0, params.wal_pages);
            if pages < need {
                return Err(StoreError::Corrupt {
                    page: 0,
                    what: format!("unit {i} has {pages} pages, needs at least {need}"),
                });
            }
        }
        let layout = Layout::new(&sizes);
        let mut io = AspIo::new(devs, layout.clone());
        let mut units = Vec::new();
        for (i, &pages) in sizes.iter().enumerate() {
            let unit = u16::try_from(i).expect("checked");
            let header = UnitHeader {
                unit_uuid: params.unit_uuids[i],
                asp_uuid: params.asp_uuid,
                asp_number: params.asp_number,
                unit_number: unit,
                pages,
                mirror_of: None,
            };
            write_unit_header(&mut io, unit, &header)?;
            units.push(UnitEntry::unprotected(
                params.unit_uuids[i],
                pages,
                layout.span(unit).start,
                params.unit_serials.get(i).map_or("", String::as_str),
            ));
        }
        let sb = Superblock {
            flags: 0,
            generation: 0,
            asp_uuid: params.asp_uuid,
            asp_number: params.asp_number,
            release: release::RELEASE,
            protection: superblock::PROTECTION_NONE,
            alloc_root: None,
            free_log_head: None,
            objdir_root: None,
            next_serial: 1,
            wal_start: RESERVED_HEAD,
            wal_pages: params.wal_pages,
            wal_head: 0,
            wal_tail: 0,
            last_txn: 0,
            install_time: params.now,
            last_rclstg_time: 0,
            qsys: ObjectId::NONE,
            sysval: ObjectId::NONE,
            units,
        };
        let alloc = Allocator::format(layout, params.wal_pages);
        let mut asp = Self { io, sb, alloc };
        asp.checkpoint()?;
        info!(
            "asp {}: formatted {} unit(s), {} pages, {} free",
            asp.sb.asp_number,
            asp.layout().unit_count(),
            asp.layout().total_pages(),
            asp.alloc.total_free()
        );
        Ok(asp)
    }

    /// Open an ASP from whatever units were found, in any order. A
    /// missing half of a pair is tolerated and reported; a missing
    /// unprotected unit, or both halves, is [`StoreError::UnitMissing`].
    pub fn open(devs: Vec<Box<dyn BlockDev>>) -> Result<(Self, Vec<UnitEvent>)> {
        if devs.is_empty() {
            return Err(StoreError::Corrupt {
                page: 0,
                what: "no units".into(),
            });
        }
        // Unit headers: page 0, else the backup at the last page.
        let mut found: Vec<(UnitHeader, Box<dyn BlockDev>)> = Vec::new();
        let mut first_err = None;
        for dev in devs {
            match read_unit_header(dev.as_ref()) {
                Ok(header) => found.push((header, dev)),
                Err(e) => {
                    warn!("asp: a device has no readable unit header: {e}");
                    first_err.get_or_insert(e);
                }
            }
        }
        if found.is_empty() {
            return Err(first_err.expect("at least one device"));
        }
        // The ASP most of the devices belong to; the rest are foreign.
        let asp_uuid = found
            .iter()
            .map(|(h, _)| h.asp_uuid)
            .max_by_key(|u| found.iter().filter(|(h, _)| h.asp_uuid == *u).count())
            .expect("not empty");
        found.retain(|(h, _)| h.asp_uuid == asp_uuid);
        // Newest valid superblock across every unit and both slots.
        let mut best: Option<Superblock> = None;
        for (_, dev) in &found {
            for slot in 1..=2u64 {
                let mut page = Page::zeroed();
                if dev.read(slot, page.bytes_mut()).is_err()
                    || page.verify_type(slot, PageType::Superblock).is_err()
                {
                    continue;
                }
                let Ok(sb) = Superblock::decode(page.body(), slot) else {
                    continue;
                };
                if best.as_ref().is_none_or(|b| sb.generation > b.generation) {
                    best = Some(sb);
                }
            }
        }
        let Some(mut sb) = best else {
            return Err(StoreError::Corrupt {
                page: 1,
                what: "no valid superblock on any unit".into(),
            });
        };
        if !release::RELEASE.is_compatible_store(sb.release) {
            return Err(StoreError::ReleaseMismatch {
                store: sb.release.to_string(),
                running: release::RELEASE.to_string(),
            });
        }
        if sb.asp_uuid != asp_uuid {
            return Err(StoreError::Corrupt {
                page: 1,
                what: "superblock does not describe these units".into(),
            });
        }
        // Match the devices to the table.
        let mut units: Vec<UnitDev> = Vec::with_capacity(sb.units.len());
        for (i, entry) in sb.units.iter().enumerate() {
            let unit = u16::try_from(i).expect("table size");
            let at = found.iter().position(|(h, _)| h.unit_uuid == entry.unit_uuid);
            let dev = match at {
                Some(at) => {
                    let (h, dev) = found.remove(at);
                    if h.unit_number != unit || h.pages != entry.pages || dev.blocks() != h.pages {
                        return Err(StoreError::Corrupt {
                            page: 0,
                            what: format!("unit table disagrees with the header of unit {unit}"),
                        });
                    }
                    Some(dev)
                }
                None => None,
            };
            units.push(UnitDev {
                dev,
                state: entry.state,
            });
        }
        for (h, _) in &found {
            warn!("asp: unit {} is not in the unit table; ignored", h.unit_number);
        }
        let layout = Layout::from_table(&sb.units)?;
        let events = resolve_states(&layout, &mut units, &mut sb)?;
        let mut io = AspIo::with_units(units, layout.clone());
        let alloc = Allocator::load(&mut io, layout, sb.alloc_root, sb.free_log_head)?;
        debug!(
            "asp {}: opened at generation {}, {} free pages",
            sb.asp_number,
            sb.generation,
            alloc.total_free()
        );
        Ok((Self { io, sb, alloc }, events))
    }

    /// Write the allocator's tree and log, then the superblock on every
    /// attached unit, then flush. Pages freed since the last checkpoint
    /// become allocatable afterwards.
    pub fn checkpoint(&mut self) -> Result<()> {
        let generation = self.sb.generation + 1;
        let roots = self.alloc.prepare_checkpoint(&mut self.io, generation)?;
        // Everything the superblock will point at, and every in-place
        // write the WAL tail will no longer cover, must be durable first.
        self.io.flush()?;
        self.sb.generation = generation;
        self.sb.alloc_root = roots.alloc_root;
        self.sb.free_log_head = roots.free_log_head;
        for (i, entry) in self.sb.units.iter_mut().enumerate() {
            entry.state = self.io.unit_state(u16::try_from(i).expect("table size"));
        }
        self.sb.update_protection();
        let slot = 1 + generation % 2;
        let mut page = Page::zeroed();
        self.sb.encode(page.body_mut());
        page.seal(PageType::Superblock, ObjectId::NONE, generation);
        for unit in 0..self.io.unit_count() {
            if !self.io.unit_state(unit).present() || !self.io.unit_attached(unit) {
                continue;
            }
            if let Err(e) = self.io.write_unit_raw(unit, slot, &mut page) {
                if self.io.unit_state(unit).mirrored() {
                    warn!("asp: superblock write on unit {unit} failed: {e}");
                    self.io.set_unit_state(unit, UnitState::NotOperational);
                    continue;
                }
                return Err(e);
            }
        }
        self.io.flush()?;
        self.alloc.finish_checkpoint();
        Ok(())
    }

    /// Append an unprotected unit as a new span. The caller checkpoints.
    pub fn add_unit(&mut self, dev: Box<dyn BlockDev>, unit_uuid: [u8; 16], serial: &str) -> Result<u16> {
        if self.sb.units.len() >= MAX_UNITS {
            return Err(StoreError::Corrupt {
                page: 0,
                what: format!("the unit table holds at most {MAX_UNITS} units"),
            });
        }
        let pages = dev.blocks();
        let need = min_pages(false, 0);
        if pages < need {
            return Err(StoreError::UnitTooSmall { pages, need });
        }
        let unit = self.io.unit_count();
        let header = UnitHeader {
            unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: unit,
            pages,
            mirror_of: None,
        };
        self.io.push_unit(UnitDev {
            dev: Some(dev),
            state: UnitState::Unprotected,
        });
        write_unit_header(&mut self.io, unit, &header)?;
        let start = self.layout().total_pages();
        let mut layout = self.layout().clone();
        layout.push(unit, pages);
        self.io.set_layout(layout.clone());
        self.alloc.add_unit(layout, pages);
        self.sb
            .units
            .push(UnitEntry::unprotected(unit_uuid, pages, start, serial));
        info!("asp {}: unit {unit} added, {pages} pages", self.sb.asp_number);
        Ok(unit)
    }

    /// Keep the allocator off a span so a checkpoint moves its own
    /// pages elsewhere, before the unit is removed.
    pub fn exclude_unit(&mut self, unit: u16, excluded: bool) -> Result<()> {
        let span = self.span_of(unit)?;
        self.alloc.exclude(span, excluded);
        Ok(())
    }

    /// Whether a unit may leave the configuration: the last one, not
    /// unit 0, not half of a pair.
    pub fn check_removable(&self, unit: u16) -> Result<()> {
        let last = self.io.unit_count() - 1;
        if unit != last || unit == 0 {
            return Err(StoreError::UnitInUse {
                unit,
                what: "only the last unit can be removed".into(),
            });
        }
        if self.sb.units[usize::from(unit)].mirrored() {
            return Err(StoreError::UnitInUse {
                unit,
                what: "the unit is half of a mirrored pair".into(),
            });
        }
        Ok(())
    }

    /// Remove the last unit. It must be unprotected and hold nothing;
    /// its header is wiped so the device is no longer configured. The
    /// caller checkpoints.
    pub fn remove_unit(&mut self, unit: u16) -> Result<Box<dyn BlockDev>> {
        self.check_removable(unit)?;
        let span = self.span_of(unit)?;
        let used = self.alloc.span_used(span);
        if used > 0 {
            return Err(StoreError::UnitInUse {
                unit,
                what: format!("{used} page(s) are in use"),
            });
        }
        self.alloc.pop_unit()?;
        let mut layout = self.layout().clone();
        layout.pop();
        self.io.set_layout(layout);
        self.sb.units.pop();
        let dev = self.io.pop_unit().expect("unit exists").dev;
        let Some(dev) = dev else {
            return Err(StoreError::UnitMissing {
                unit,
                what: "unit is not attached".into(),
            });
        };
        wipe_unit(dev.as_ref())?;
        info!("asp {}: unit {unit} removed", self.sb.asp_number);
        Ok(dev)
    }

    /// The span a unit backs.
    pub fn span_of(&self, unit: u16) -> Result<u16> {
        self.layout()
            .span_of_unit(unit)
            .ok_or_else(|| StoreError::UnitMissing {
                unit,
                what: "no such unit".into(),
            })
    }

    /// The unit table with usage and attachment.
    #[must_use]
    pub fn units(&self) -> Vec<UnitInfo> {
        self.sb
            .units
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let unit = u16::try_from(i).expect("table size");
                let used_pages = self
                    .layout()
                    .span_of_unit(unit)
                    .map_or(0, |span| self.alloc.span_used(span));
                UnitInfo {
                    unit,
                    entry: entry.clone(),
                    used_pages,
                    attached: self.io.unit_attached(unit),
                }
            })
            .collect()
    }

    /// The page layout.
    #[must_use]
    pub fn layout(&self) -> &Layout {
        self.io.layout()
    }

    /// The superblock as last written or as about to be written.
    #[must_use]
    pub fn superblock(&self) -> &Superblock {
        &self.sb
    }

    /// The superblock, for the layers that publish roots through it.
    pub fn superblock_mut(&mut self) -> &mut Superblock {
        &mut self.sb
    }

    /// Page I/O on the units.
    pub fn io(&mut self) -> &mut AspIo {
        &mut self.io
    }

    /// Allocate up to `pages` contiguous pages. See [`Allocator::alloc`].
    pub fn alloc(&mut self, pages: u64) -> Result<Run> {
        self.alloc.alloc(pages)
    }

    /// Grow a run in place. See [`Allocator::extend`].
    pub fn extend(&mut self, end: PageNo, want: u64) -> u64 {
        self.alloc.extend(end, want)
    }

    /// Free a run. See [`Allocator::free`].
    pub fn free(&mut self, start: PageNo, len: u64) -> Result<()> {
        self.alloc.free(start, len)
    }

    /// The allocator, for the transaction layer.
    pub fn allocator_mut(&mut self) -> &mut Allocator {
        &mut self.alloc
    }

    /// Free pages now, the checkpoint reserve included.
    #[must_use]
    pub fn total_free(&self) -> u64 {
        self.alloc.total_free()
    }

    /// Pages `alloc` may still hand out.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.alloc.available()
    }

    /// Pages held by the allocator's own tree and log.
    #[must_use]
    pub fn metadata_pages(&self) -> u64 {
        self.alloc.metadata_pages()
    }

    /// Per-span statistics.
    #[must_use]
    pub fn unit_stats(&self) -> Vec<UnitStats> {
        self.alloc.unit_stats()
    }

    /// The allocator, for tests and tools.
    #[must_use]
    pub fn allocator(&self) -> &Allocator {
        &self.alloc
    }
}

/// Pages a unit needs: the reserved head and tail, the spare, and the
/// WAL on unit 0.
fn min_pages(first: bool, wal_pages: u64) -> u64 {
    RESERVED_HEAD + 1 + MIN_UNIT_SPARE + if first { wal_pages } else { 0 }
}

/// Decide what to do with the units found and not found. Updates the
/// table's states for the next checkpoint.
fn resolve_states(
    layout: &Layout,
    units: &mut [UnitDev],
    sb: &mut Superblock,
) -> Result<Vec<UnitEvent>> {
    let mut events = Vec::new();
    let missing = |unit: u16, sb: &Superblock| StoreError::UnitMissing {
        unit,
        what: format!("serial {}", sb.units[usize::from(unit)].serial_text()),
    };
    for span in layout.spans() {
        let present = |u: u16| units[usize::from(u)].dev.is_some();
        match span.partner {
            None => {
                if !present(span.primary) {
                    return Err(missing(span.primary, sb));
                }
            }
            Some(partner) => {
                let pair = [span.primary, partner];
                let here: Vec<u16> = pair.iter().copied().filter(|&u| present(u)).collect();
                match here.as_slice() {
                    [] => return Err(missing(span.primary, sb)),
                    [only] => {
                        let other = if *only == span.primary { partner } else { span.primary };
                        if units[usize::from(*only)].state != UnitState::Active {
                            // The half we have is the stale one.
                            return Err(missing(other, sb));
                        }
                        units[usize::from(other)].state = UnitState::Missing;
                        sb.units[usize::from(other)].state = UnitState::Missing;
                        events.push(UnitEvent::Missing { unit: other });
                    }
                    _ => {
                        for u in pair {
                            let state = &mut units[usize::from(u)].state;
                            match *state {
                                UnitState::Missing | UnitState::NotOperational => {
                                    *state = UnitState::Suspended;
                                    sb.units[usize::from(u)].state = UnitState::Suspended;
                                    events.push(UnitEvent::Stale { unit: u });
                                }
                                UnitState::Resuming => events.push(UnitEvent::Resuming { unit: u }),
                                _ => {}
                            }
                        }
                        if pair
                            .iter()
                            .all(|&u| units[usize::from(u)].state != UnitState::Active)
                        {
                            return Err(StoreError::Corrupt {
                                page: 1,
                                what: format!(
                                    "neither half of the pair at unit {} is active",
                                    span.primary
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(events)
}

/// The unit header from page 0, or from the backup at the last page.
fn read_unit_header(dev: &dyn BlockDev) -> Result<UnitHeader> {
    let last = dev.blocks().saturating_sub(1);
    let mut first_err = None;
    for lba in [0, last] {
        let mut page = Page::zeroed();
        dev.read(lba, page.bytes_mut())?;
        match page
            .verify_type(lba, PageType::UnitHeader)
            .and_then(|_| UnitHeader::decode(page.body(), lba))
        {
            Ok(h) => return Ok(h),
            Err(e) => first_err.get_or_insert(e),
        };
    }
    Err(first_err.expect("two attempts"))
}

/// Write a unit's header at page 0 and at its last page, and zero both
/// superblock slots so a stale copy from an earlier format cannot win.
pub(crate) fn write_unit_header(io: &mut AspIo, unit: u16, header: &UnitHeader) -> Result<()> {
    let mut page = Page::zeroed();
    header.encode(page.body_mut());
    page.seal(PageType::UnitHeader, ObjectId::NONE, 0);
    io.write_unit_raw(unit, 0, &mut page)?;
    io.write_unit_raw(unit, header.pages - 1, &mut page)?;
    let mut zero = Page::zeroed();
    io.write_unit_raw(unit, 1, &mut zero)?;
    io.write_unit_raw(unit, 2, &mut zero)?;
    io.flush_unit(unit)
}

/// Zero a device's header pages and superblock slots: no longer a unit.
pub(crate) fn wipe_unit(dev: &dyn BlockDev) -> Result<()> {
    let zero = Page::zeroed();
    for lba in [0, 1, 2, dev.blocks() - 1] {
        dev.write(lba, zero.bytes())?;
    }
    dev.flush()?;
    Ok(())
}

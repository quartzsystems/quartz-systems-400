// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An auxiliary storage pool: one or more units (block devices) with a
//! unit header each, two superblock copies each, and one logical page
//! space. See `docs/STORE_FORMAT.md` §4, §5 and §7.

pub mod io;
pub mod layout;
pub mod superblock;
pub mod unit;

use blockdev::BlockDev;
use tracing::{debug, info};

use crate::alloc::{Allocator, Run, UnitStats};
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{Page, PageType};
use crate::{Result, StoreError};
use io::AspIo;
use layout::{Layout, RESERVED_HEAD};
use superblock::Superblock;
use unit::UnitHeader;

/// What `format` needs beyond the devices.
#[derive(Debug, Clone)]
pub struct FormatParams {
    /// ASP number (1 for SYSBAS, 0 for the temporary ASP).
    pub asp_number: u16,
    /// Identity of the ASP.
    pub asp_uuid: [u8; 16],
    /// One UUID per device, in order.
    pub unit_uuids: Vec<[u8; 16]>,
    /// WAL ring length in pages.
    pub wal_pages: u64,
    /// Install time, Unix seconds.
    pub now: u64,
}

/// Pages a unit must have beyond the WAL and the reserved pages.
pub const MIN_UNIT_SPARE: u64 = 64;

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
        if devs.is_empty()
            || devs.len() != params.unit_uuids.len()
            || devs.len() > layout::MAX_UNITS
        {
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
            let need =
                RESERVED_HEAD + 1 + MIN_UNIT_SPARE + if i == 0 { params.wal_pages } else { 0 };
            if pages < need {
                return Err(StoreError::Corrupt {
                    page: 0,
                    what: format!("unit {i} has {pages} pages, needs at least {need}"),
                });
            }
        }
        let layout = Layout::new(&sizes);
        let mut io = AspIo::new(devs, layout.clone());
        for (unit, &pages) in sizes.iter().enumerate() {
            let unit_number = u16::try_from(unit).expect("checked");
            let header = UnitHeader {
                unit_uuid: params.unit_uuids[unit],
                asp_uuid: params.asp_uuid,
                asp_number: params.asp_number,
                unit_number,
                pages,
                mirror_of: None,
            };
            let mut page = Page::zeroed();
            header.encode(page.body_mut());
            io.write_page(
                layout.logical(unit_number, 0),
                &mut page,
                PageType::UnitHeader,
                ObjectId::NONE,
                0,
            )?;
            io.write_page(
                layout.logical(unit_number, pages - 1),
                &mut page,
                PageType::UnitHeader,
                ObjectId::NONE,
                0,
            )?;
            // Zero both superblock slots so a stale copy from an earlier
            // format cannot win.
            let mut zero = Page::zeroed();
            io.write_raw(layout.logical(unit_number, 1), &mut zero)?;
            io.write_raw(layout.logical(unit_number, 2), &mut zero)?;
        }
        let sb = Superblock {
            flags: 0,
            generation: 0,
            asp_uuid: params.asp_uuid,
            asp_number: params.asp_number,
            release: release::RELEASE,
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
            units: sizes
                .iter()
                .enumerate()
                .map(|(i, &pages)| superblock::UnitEntry {
                    unit_uuid: params.unit_uuids[i],
                    pages,
                    start: layout.span(u16::try_from(i).expect("checked")).start,
                })
                .collect(),
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

    /// Open an ASP from its units, in any order.
    pub fn open(devs: Vec<Box<dyn BlockDev>>) -> Result<Self> {
        if devs.is_empty() {
            return Err(StoreError::Corrupt {
                page: 0,
                what: "no units".into(),
            });
        }
        // Unit headers: page 0, else the backup at the last page.
        let mut found: Vec<(UnitHeader, Box<dyn BlockDev>)> = Vec::new();
        for dev in devs {
            let header = read_unit_header(dev.as_ref())?;
            if header.pages != dev.blocks() {
                return Err(StoreError::Corrupt {
                    page: 0,
                    what: format!(
                        "unit {} header says {} pages, device has {}",
                        header.unit_number,
                        header.pages,
                        dev.blocks()
                    ),
                });
            }
            found.push((header, dev));
        }
        found.sort_by_key(|(h, _)| h.unit_number);
        let asp_uuid = found[0].0.asp_uuid;
        for (i, (h, _)) in found.iter().enumerate() {
            if usize::from(h.unit_number) != i || h.asp_uuid != asp_uuid {
                return Err(StoreError::Corrupt {
                    page: 0,
                    what: format!("unit set is incomplete or mixed at unit {}", h.unit_number),
                });
            }
        }
        let sizes: Vec<u64> = found.iter().map(|(h, _)| h.pages).collect();
        let layout = Layout::new(&sizes);
        let headers: Vec<UnitHeader> = found.iter().map(|(h, _)| h.clone()).collect();
        let mut io = AspIo::new(found.into_iter().map(|(_, d)| d).collect(), layout.clone());
        // Newest valid superblock across every unit and both slots.
        let mut best: Option<Superblock> = None;
        for unit in 0..layout.unit_count() {
            for slot in 1..=2u64 {
                let pno = layout.logical(unit, slot);
                let Ok(page) = io.read_page(pno) else {
                    continue;
                };
                if page.trailer().page_type != PageType::Superblock.as_u16() {
                    continue;
                }
                let Ok(sb) = Superblock::decode(page.body(), pno) else {
                    continue;
                };
                if best.as_ref().is_none_or(|b| sb.generation > b.generation) {
                    best = Some(sb);
                }
            }
        }
        let Some(sb) = best else {
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
        if sb.asp_uuid != asp_uuid || sb.units.len() != headers.len() {
            return Err(StoreError::Corrupt {
                page: 1,
                what: "superblock does not describe these units".into(),
            });
        }
        for (entry, h) in sb.units.iter().zip(&headers) {
            if entry.unit_uuid != h.unit_uuid || entry.pages != h.pages {
                return Err(StoreError::Corrupt {
                    page: 1,
                    what: format!("unit table disagrees with unit {}", h.unit_number),
                });
            }
        }
        let alloc = Allocator::load(&mut io, layout, sb.alloc_root, sb.free_log_head)?;
        debug!(
            "asp {}: opened at generation {}, {} free pages",
            sb.asp_number,
            sb.generation,
            alloc.total_free()
        );
        Ok(Self { io, sb, alloc })
    }

    /// Write the allocator's tree and log, then the superblock on every
    /// unit, then flush. Pages freed since the last checkpoint become
    /// allocatable afterwards.
    pub fn checkpoint(&mut self) -> Result<()> {
        let generation = self.sb.generation + 1;
        let roots = self.alloc.prepare_checkpoint(&mut self.io, generation)?;
        self.sb.generation = generation;
        self.sb.alloc_root = roots.alloc_root;
        self.sb.free_log_head = roots.free_log_head;
        let slot = 1 + generation % 2;
        let mut page = Page::zeroed();
        self.sb.encode(page.body_mut());
        for unit in 0..self.layout().unit_count() {
            let pno = self.layout().logical(unit, slot);
            self.io.write_page(
                pno,
                &mut page,
                PageType::Superblock,
                ObjectId::NONE,
                generation,
            )?;
        }
        self.io.flush()?;
        self.alloc.finish_checkpoint();
        Ok(())
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

    /// Per-unit statistics.
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

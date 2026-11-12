// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hot spares: disks held outside every span, ready to take the place
//! of a failed mirrored half or parity set member. See
//! `docs/STORE_FORMAT.md` §4, "Hot spares".

use blockdev::BlockDev;
use tracing::info;

use crate::asp::Asp;
use crate::asp::parity::{Health, unit_of};
use crate::asp::superblock::MAX_SPARES;
use crate::asp::unit::{UnitHeader, UnitState};
use crate::obj::ObjectId;
use crate::page::{Page, PageType};
use crate::{Refusal, Result, StoreError};

/// `unit_number` in a hot spare's header: it is in no span.
pub const SPARE_UNIT: u16 = 0xFFFF;

/// A hot spare the ASP holds.
pub(crate) struct SpareDev {
    /// Its unit UUID, as the superblock lists it.
    pub uuid: [u8; 16],
    /// The device, when it was found.
    pub dev: Option<Box<dyn BlockDev>>,
}

/// A hot spare as the displays show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpareInfo {
    /// Its unit UUID.
    pub unit_uuid: [u8; 16],
    /// Size in pages; 0 when the device is missing.
    pub pages: u64,
    /// Whether the device is attached now.
    pub attached: bool,
}

/// A failed unit a hot spare can take the place of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failed {
    /// The unit.
    pub unit: u16,
    /// Pages the replacement needs.
    pub need: u64,
    /// In a parity set (else half of a pair).
    pub parity: bool,
}

/// What a hot spare was used for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpareUsed {
    /// The unit it replaced.
    pub unit: u16,
    /// The spare's UUID, now the unit's.
    pub unit_uuid: [u8; 16],
    /// In a parity set (rebuilt by the parity work) or half of a pair
    /// (resumed from its partner by the caller).
    pub parity: bool,
}

impl Asp {
    /// The hot spares, in the order the superblock lists them.
    #[must_use]
    pub fn hot_spares(&self) -> Vec<SpareInfo> {
        self.spares
            .iter()
            .map(|s| SpareInfo {
                unit_uuid: s.uuid,
                pages: s.dev.as_deref().map_or(0, BlockDev::blocks),
                attached: s.dev.is_some(),
            })
            .collect()
    }

    /// Make a blank disk a hot spare of this ASP. It gets a unit header
    /// numbered [`SPARE_UNIT`]; the caller checkpoints.
    pub fn start_hot_spare(&mut self, dev: Box<dyn BlockDev>, unit_uuid: [u8; 16]) -> Result<()> {
        if self.spares.len() >= MAX_SPARES {
            return Err(StoreError::Refused {
                unit: None,
                reason: Refusal::SparesFull,
            });
        }
        let pages = dev.blocks();
        let need = super::min_pages(false, 0);
        if pages < need {
            return Err(StoreError::UnitTooSmall { pages, need });
        }
        let header = UnitHeader {
            unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: SPARE_UNIT,
            pages,
            mirror_of: None,
        };
        write_spare_header(dev.as_ref(), &header)?;
        self.spares.push(SpareDev {
            uuid: unit_uuid,
            dev: Some(dev),
        });
        self.sb.spares.push(unit_uuid);
        info!(
            "asp {}: hot spare of {pages} pages added",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Stop holding a hot spare. The caller checkpoints, then wipes the
    /// device ([`super::wipe_unit`]) when one comes back.
    pub fn stop_hot_spare(&mut self, unit_uuid: &[u8; 16]) -> Result<Option<Box<dyn BlockDev>>> {
        let at = self
            .spares
            .iter()
            .position(|s| s.uuid == *unit_uuid)
            .ok_or(StoreError::Refused {
                unit: None,
                reason: Refusal::NoSuchSpare,
            })?;
        let spare = self.spares.remove(at);
        self.sb.spares.retain(|u| u != unit_uuid);
        info!("asp {}: hot spare removed", self.sb.asp_number);
        Ok(spare.dev)
    }

    /// Units a hot spare could replace now: halves missing or not
    /// operational beside an active partner, and failed parity members.
    #[must_use]
    pub fn failed_units(&self) -> Vec<Failed> {
        let mut out = Vec::new();
        for (i, e) in self.sb.units.iter().enumerate() {
            let unit = u16::try_from(i).expect("table size");
            if !e.mirrored()
                || !matches!(
                    self.io.unit_state(unit),
                    UnitState::Missing | UnitState::NotOperational
                )
            {
                continue;
            }
            let partner = e.mirror_partner;
            if self.io.unit_state(partner) == UnitState::Active && self.io.unit_attached(partner) {
                out.push(Failed {
                    unit,
                    need: e.usable_pages,
                    parity: false,
                });
            }
        }
        for h in &self.parity.sets {
            let eng = h.lock();
            for m in eng.members.iter().filter(|m| m.health == Health::Failed) {
                if let Some(unit) = unit_of(&self.sb, &m.unit_uuid) {
                    out.push(Failed {
                        unit,
                        need: self.sb.units[usize::from(unit)].usable_pages,
                        parity: true,
                    });
                }
            }
        }
        out.sort_by_key(|f| f.unit);
        out
    }

    /// Take the smallest attached spare of at least `need` pages out of
    /// the list. The caller puts it in a unit's place and checkpoints.
    pub(crate) fn take_spare(&mut self, need: u64) -> Option<([u8; 16], Box<dyn BlockDev>)> {
        let at = protect::spare::smallest_fit(
            self.spares
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.dev.as_ref().map(|d| (i, d.blocks()))),
            need,
        )?;
        let spare = self.spares.remove(at);
        self.sb.spares.retain(|u| *u != spare.uuid);
        spare.dev.map(|d| (spare.uuid, d))
    }

    /// At open: the devices left over that the superblock names as
    /// spares. The others stay with the caller.
    pub(crate) fn attach_spares(
        sb: &crate::asp::superblock::Superblock,
        found: &mut Vec<(UnitHeader, Box<dyn BlockDev>)>,
    ) -> Vec<SpareDev> {
        sb.spares
            .iter()
            .map(|uuid| {
                let at = found
                    .iter()
                    .position(|(h, d)| h.unit_uuid == *uuid && d.blocks() == h.pages);
                SpareDev {
                    uuid: *uuid,
                    dev: at.map(|i| found.remove(i).1),
                }
            })
            .collect()
    }
}

/// A spare's header at page 0 and its last page; both superblock slots
/// zeroed so no older copy can win an open.
fn write_spare_header(dev: &dyn BlockDev, header: &UnitHeader) -> Result<()> {
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

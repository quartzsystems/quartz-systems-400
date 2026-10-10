// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Page I/O over the units of an ASP, addressed by logical page number.
//! A span backed by a mirrored pair is written to both halves and read
//! from one; a page that fails verification on one half is read from
//! the other and rewritten.

use blockdev::BlockDev;
use tracing::warn;

use crate::asp::layout::Layout;
use crate::asp::unit::UnitState;
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{Page, PageType};
use crate::{Result, StoreError};

/// One physical unit: its device, when attached, and its state.
pub struct UnitDev {
    /// The device; `None` for a missing unit.
    pub dev: Option<Box<dyn BlockDev>>,
    /// Protection state.
    pub state: UnitState,
}

/// Something the I/O layer did on its own, for the caller's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoEvent {
    /// A page that failed verification on one half was rewritten from
    /// the other.
    Healed {
        /// Logical page.
        page: PageNo,
        /// The unit rewritten.
        unit: u16,
    },
    /// A half of a pair failed I/O and was taken out of service.
    UnitFailed {
        /// The unit.
        unit: u16,
        /// What failed.
        what: String,
    },
}

/// The units and the mapping onto them.
pub struct AspIo {
    units: Vec<UnitDev>,
    layout: Layout,
    /// Alternates reads between the halves of a pair.
    turn: u64,
    events: Vec<IoEvent>,
}

impl AspIo {
    /// Over `devs` in unit order, every unit unprotected and present.
    #[must_use]
    pub fn new(devs: Vec<Box<dyn BlockDev>>, layout: Layout) -> Self {
        let units = devs
            .into_iter()
            .map(|d| UnitDev {
                dev: Some(d),
                state: UnitState::Unprotected,
            })
            .collect();
        Self::with_units(units, layout)
    }

    /// Over physical units with their states.
    #[must_use]
    pub fn with_units(units: Vec<UnitDev>, layout: Layout) -> Self {
        Self {
            units,
            layout,
            turn: 0,
            events: Vec::new(),
        }
    }

    /// The layout.
    #[must_use]
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Replace the layout after the unit table changed.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Number of physical units.
    #[must_use]
    pub fn unit_count(&self) -> u16 {
        u16::try_from(self.units.len()).expect("at most MAX_UNITS")
    }

    /// A unit's state.
    #[must_use]
    pub fn unit_state(&self, unit: u16) -> UnitState {
        self.units[usize::from(unit)].state
    }

    /// Change a unit's state.
    pub fn set_unit_state(&mut self, unit: u16, state: UnitState) {
        self.units[usize::from(unit)].state = state;
    }

    /// Whether a unit's device is attached.
    #[must_use]
    pub fn unit_attached(&self, unit: u16) -> bool {
        self.units[usize::from(unit)].dev.is_some()
    }

    /// A unit's size in pages, 0 when missing.
    #[must_use]
    pub fn unit_blocks(&self, unit: u16) -> u64 {
        self.units[usize::from(unit)]
            .dev
            .as_ref()
            .map_or(0, blockdev::BlockDev::blocks)
    }

    /// Append a physical unit.
    pub fn push_unit(&mut self, unit: UnitDev) {
        self.units.push(unit);
    }

    /// Remove the last physical unit, giving its device back.
    pub fn pop_unit(&mut self) -> Option<UnitDev> {
        self.units.pop()
    }

    /// Take a unit out of the list, shifting the ones after it down.
    pub fn remove_unit_slot(&mut self, unit: u16) -> Option<Box<dyn BlockDev>> {
        self.units.remove(usize::from(unit)).dev
    }

    /// Write whole pages to one physical unit at a unit-relative page.
    pub fn write_unit_bytes(&mut self, unit: u16, lba: u64, bytes: &[u8]) -> Result<()> {
        match self.dev(unit) {
            Some(d) => Ok(d.write(lba, bytes)?),
            None => Err(StoreError::UnitMissing {
                unit,
                what: "unit is not attached".into(),
            }),
        }
    }

    /// Attach a device to a unit (a replaced or returned half).
    pub fn attach(&mut self, unit: u16, dev: Box<dyn BlockDev>, state: UnitState) {
        let u = &mut self.units[usize::from(unit)];
        u.dev = Some(dev);
        u.state = state;
    }

    /// Detach a unit's device, keeping its table slot.
    pub fn detach(&mut self, unit: u16, state: UnitState) -> Option<Box<dyn BlockDev>> {
        let u = &mut self.units[usize::from(unit)];
        u.state = state;
        u.dev.take()
    }

    /// Events since the last call.
    pub fn drain_events(&mut self) -> Vec<IoEvent> {
        std::mem::take(&mut self.events)
    }

    fn locate(&self, pno: PageNo) -> Result<(u16, u64)> {
        self.layout.locate(pno).ok_or_else(|| StoreError::Corrupt {
            page: pno,
            what: "page number beyond the ASP".into(),
        })
    }

    fn dev(&self, unit: u16) -> Option<&dyn BlockDev> {
        self.units[usize::from(unit)].dev.as_deref()
    }

    /// Members of a span that may serve reads, the preferred one first.
    fn readers(&mut self, span: u16) -> Vec<u16> {
        let mut r: Vec<u16> = self
            .layout
            .span(span)
            .members()
            .filter(|&u| self.unit_state(u).readable() && self.dev(u).is_some())
            .collect();
        if r.len() > 1 {
            self.turn = self.turn.wrapping_add(1);
            if self.turn % 2 == 1 {
                r.rotate_left(1);
            }
        }
        r
    }

    /// Members of a span that must be written.
    fn writers(&self, span: u16) -> Vec<u16> {
        self.layout
            .span(span)
            .members()
            .filter(|&u| self.unit_state(u).writable() && self.dev(u).is_some())
            .collect()
    }

    fn missing(&self, span: u16) -> StoreError {
        let s = self.layout.span(span);
        StoreError::UnitMissing {
            unit: s.primary,
            what: "no half of the unit is available".into(),
        }
    }

    /// Take a failing half out of service. Returns whether the span
    /// still has another half to go on with.
    fn fail_unit(&mut self, span: u16, unit: u16, what: &str) -> bool {
        let others = self
            .layout
            .span(span)
            .members()
            .any(|u| u != unit && self.unit_state(u).writable() && self.dev(u).is_some());
        if others {
            warn!("asp: unit {unit} taken out of service: {what}");
            self.set_unit_state(unit, UnitState::NotOperational);
            self.events.push(IoEvent::UnitFailed {
                unit,
                what: what.to_string(),
            });
        }
        others
    }

    /// Read and verify page `pno`. A page that fails on one half of a
    /// pair is read from the other and the bad half is rewritten.
    pub fn read_page(&mut self, pno: PageNo) -> Result<Page> {
        let (span, lba) = self.locate(pno)?;
        let readers = self.readers(span);
        let mut first_err = None;
        for (i, &unit) in readers.iter().enumerate() {
            let mut page = Page::zeroed();
            let read = self
                .dev(unit)
                .expect("reader is attached")
                .read(lba, page.bytes_mut());
            match read
                .map_err(StoreError::from)
                .and_then(|()| page.verify(pno).map(|_| ()))
            {
                Ok(()) => {
                    if i > 0 {
                        self.heal(span, readers[0], lba, pno, &mut page);
                    }
                    return Ok(page);
                }
                Err(StoreError::Block(e)) => {
                    let what = format!("read page {pno}: {e}");
                    if !self.fail_unit(span, unit, &what) {
                        return Err(StoreError::Block(e));
                    }
                    first_err.get_or_insert(StoreError::Block(e));
                }
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.unwrap_or_else(|| self.missing(span)))
    }

    /// Rewrite a page on the half that could not serve it.
    fn heal(&mut self, span: u16, unit: u16, lba: u64, pno: PageNo, page: &mut Page) {
        if !self.unit_state(unit).writable() {
            return;
        }
        let Some(dev) = self.dev(unit) else {
            return;
        };
        match dev.write(lba, page.bytes()) {
            Ok(()) => {
                warn!("asp: page {pno} rewritten on unit {unit} from its mirror");
                self.events.push(IoEvent::Healed { page: pno, unit });
            }
            Err(e) => {
                self.fail_unit(span, unit, &format!("rewrite page {pno}: {e}"));
            }
        }
    }

    /// Read `count` consecutive pages from `pno` (within one span), as
    /// they are: no checksum verification.
    pub fn read_run(&mut self, pno: PageNo, count: u64) -> Result<blockdev::AlignedBuf> {
        let (span, lba) = self.locate(pno)?;
        let readers = self.readers(span);
        let mut first_err = None;
        for &unit in &readers {
            let mut buf = blockdev::AlignedBuf::new(usize::try_from(count).expect("count fits"));
            match self
                .dev(unit)
                .expect("reader is attached")
                .read(lba, &mut buf)
            {
                Ok(()) => return Ok(buf),
                Err(e) => {
                    let what = format!("read {count} pages at {pno}: {e}");
                    if !self.fail_unit(span, unit, &what) {
                        return Err(e.into());
                    }
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.map_or_else(|| self.missing(span), StoreError::from))
    }

    /// Read `count` pages from `pno` on one specific unit, unverified.
    /// `None` when the unit is not attached.
    pub fn read_run_on(
        &mut self,
        unit: u16,
        pno: PageNo,
        count: u64,
    ) -> Result<Option<blockdev::AlignedBuf>> {
        let (_, lba) = self.locate(pno)?;
        let Some(dev) = self.dev(unit) else {
            return Ok(None);
        };
        let mut buf = blockdev::AlignedBuf::new(usize::try_from(count).expect("count fits"));
        dev.read(lba, &mut buf)?;
        Ok(Some(buf))
    }

    /// Seal and write page `pno`.
    pub fn write_page(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
        generation: u64,
    ) -> Result<()> {
        page.seal(kind, owner, generation);
        self.write_raw(pno, page)
    }

    /// Write page `pno` as is, trailer included, to every half.
    pub fn write_raw(&mut self, pno: PageNo, page: &mut Page) -> Result<()> {
        self.write_bytes(pno, page.bytes())
    }

    /// Write whole pages from `pno` to every half.
    pub fn write_bytes(&mut self, pno: PageNo, bytes: &[u8]) -> Result<()> {
        let (span, lba) = self.locate(pno)?;
        let writers = self.writers(span);
        if writers.is_empty() {
            return Err(self.missing(span));
        }
        let mut failed = Vec::new();
        for &unit in &writers {
            if let Err(e) = self
                .dev(unit)
                .expect("writer is attached")
                .write(lba, bytes)
            {
                failed.push((unit, e));
            }
        }
        if failed.len() == writers.len() {
            return Err(failed.remove(0).1.into());
        }
        for (unit, e) in failed {
            self.fail_unit(span, unit, &format!("write page {pno}: {e}"));
        }
        Ok(())
    }

    /// Write a page to one physical unit at a unit-relative page.
    pub fn write_unit_raw(&mut self, unit: u16, lba: u64, page: &mut Page) -> Result<()> {
        match self.dev(unit) {
            Some(d) => Ok(d.write(lba, page.bytes())?),
            None => Err(StoreError::UnitMissing {
                unit,
                what: "unit is not attached".into(),
            }),
        }
    }

    /// Read and verify a page from one physical unit.
    pub fn read_unit_page(&mut self, unit: u16, lba: u64) -> Result<Page> {
        let Some(d) = self.dev(unit) else {
            return Err(StoreError::UnitMissing {
                unit,
                what: "unit is not attached".into(),
            });
        };
        let mut page = Page::zeroed();
        d.read(lba, page.bytes_mut())?;
        page.verify(lba)?;
        Ok(page)
    }

    /// Flush one physical unit.
    pub fn flush_unit(&mut self, unit: u16) -> Result<()> {
        match self.dev(unit) {
            Some(d) => Ok(d.flush()?),
            None => Ok(()),
        }
    }

    /// Flush every half of a span, waiting for all of them.
    pub fn flush_span(&mut self, span: u16) -> Result<()> {
        let writers = self.writers(span);
        if writers.is_empty() {
            return Err(self.missing(span));
        }
        let mut failed = Vec::new();
        for &unit in &writers {
            if let Err(e) = self.dev(unit).expect("writer is attached").flush() {
                failed.push((unit, e));
            }
        }
        if failed.len() == writers.len() {
            return Err(failed.remove(0).1.into());
        }
        for (unit, e) in failed {
            self.fail_unit(span, unit, &format!("flush: {e}"));
        }
        Ok(())
    }

    /// Flush every unit.
    pub fn flush(&mut self) -> Result<()> {
        for span in 0..self.layout.unit_count() {
            self.flush_span(span)?;
        }
        Ok(())
    }

    /// Tell the units a run is unused.
    pub fn discard(&mut self, start: PageNo, len: u64) -> Result<()> {
        let (span, lba) = self.locate(start)?;
        for unit in self.writers(span) {
            self.dev(unit)
                .expect("writer is attached")
                .discard(lba, len)?;
        }
        Ok(())
    }
}

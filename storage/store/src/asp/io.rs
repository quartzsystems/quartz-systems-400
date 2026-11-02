// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Page I/O over the units of an ASP, addressed by logical page number.
//! A span backed by a mirrored pair is written to both halves and read
//! from one; a page that fails verification on one half is read from
//! the other and rewritten.
//!
//! The units sit behind a lock shared with [`SharedIo`], the handle the
//! IFS volume's server uses from its own thread. Transfers hold the lock
//! for reading (the devices are `Sync`); changing a unit's device or
//! state takes it for writing, never while a transfer of the same thread
//! holds it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

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

/// The units and the mapping, behind the lock.
struct Core {
    units: Vec<UnitDev>,
    layout: Layout,
}

/// What both handles share.
struct Shared {
    core: RwLock<Core>,
    /// Alternates reads between the halves of a pair.
    turn: AtomicU64,
    events: Mutex<Vec<IoEvent>>,
}

/// The units and the mapping onto them.
pub struct AspIo {
    shared: Arc<Shared>,
    /// The layout as the store thread sees it (the same as the core's).
    layout: Layout,
}

/// A handle on the same units for another thread: whole-page transfers
/// with no verification, and flushes. See the module documentation.
#[derive(Clone)]
pub struct SharedIo {
    shared: Arc<Shared>,
}

/// Halves to take out of service once the read lock is let go.
type Fails = Vec<(u16, u16, String)>;

impl Shared {
    fn read(&self) -> RwLockReadGuard<'_, Core> {
        self.core.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Core> {
        self.core.write().unwrap_or_else(PoisonError::into_inner)
    }

    fn push_event(&self, e: IoEvent) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(e);
    }

    /// Take the failed halves out of service.
    fn apply(&self, fails: Fails) {
        if fails.is_empty() {
            return;
        }
        let mut c = self.write();
        for (span, unit, what) in fails {
            if c.others(span, unit) && c.units[usize::from(unit)].state != UnitState::NotOperational {
                warn!("asp: unit {unit} taken out of service: {what}");
                c.units[usize::from(unit)].state = UnitState::NotOperational;
                self.push_event(IoEvent::UnitFailed { unit, what });
            }
        }
    }

    /// Read `count` pages from `pno` unverified, failing over between
    /// halves.
    fn read_run(&self, pno: PageNo, count: u64) -> Result<blockdev::AlignedBuf> {
        let mut fails = Fails::new();
        let r = {
            let c = self.read();
            c.read_run(pno, count, &self.turn, &mut fails)
        };
        self.apply(fails);
        r
    }

    /// Write whole pages from `pno` to every half.
    fn write_bytes(&self, pno: PageNo, bytes: &[u8]) -> Result<()> {
        let mut fails = Fails::new();
        let r = {
            let c = self.read();
            c.write_bytes(pno, bytes, &mut fails)
        };
        self.apply(fails);
        r
    }

    /// Flush every half of a span.
    fn flush_span(&self, span: u16) -> Result<()> {
        let mut fails = Fails::new();
        let r = {
            let c = self.read();
            c.flush_span(span, &mut fails)
        };
        self.apply(fails);
        r
    }

    fn flush(&self) -> Result<()> {
        let spans = self.read().layout.unit_count();
        for span in 0..spans {
            self.flush_span(span)?;
        }
        Ok(())
    }
}

impl Core {
    fn locate(&self, pno: PageNo) -> Result<(u16, u64)> {
        self.layout.locate(pno).ok_or_else(|| StoreError::Corrupt {
            page: pno,
            what: "page number beyond the ASP".into(),
        })
    }

    fn dev(&self, unit: u16) -> Option<&dyn BlockDev> {
        self.units[usize::from(unit)].dev.as_deref()
    }

    fn state(&self, unit: u16) -> UnitState {
        self.units[usize::from(unit)].state
    }

    /// Members of a span that may serve reads, the preferred one first.
    fn readers(&self, span: u16, turn: &AtomicU64) -> Vec<u16> {
        let mut r: Vec<u16> = self
            .layout
            .span(span)
            .members()
            .filter(|&u| self.state(u).readable() && self.dev(u).is_some())
            .collect();
        if r.len() > 1 && turn.fetch_add(1, Ordering::Relaxed).is_multiple_of(2) {
            r.rotate_left(1);
        }
        r
    }

    /// Members of a span that must be written.
    fn writers(&self, span: u16) -> Vec<u16> {
        self.layout
            .span(span)
            .members()
            .filter(|&u| self.state(u).writable() && self.dev(u).is_some())
            .collect()
    }

    fn missing(&self, span: u16) -> StoreError {
        let s = self.layout.span(span);
        StoreError::UnitMissing {
            unit: s.primary,
            what: "no half of the unit is available".into(),
        }
    }

    /// Whether a span has a writable half besides `unit`.
    fn others(&self, span: u16, unit: u16) -> bool {
        self.layout
            .span(span)
            .members()
            .any(|u| u != unit && self.state(u).writable() && self.dev(u).is_some())
    }

    /// Record a failing half to take out of service; whether the span
    /// can go on without it (no other half failed in this call).
    fn fail(&self, span: u16, unit: u16, what: String, fails: &mut Fails) -> bool {
        let others = self.layout.span(span).members().any(|u| {
            u != unit
                && self.state(u).writable()
                && self.dev(u).is_some()
                && !fails.iter().any(|f| f.1 == u)
        });
        if others {
            fails.push((span, unit, what));
        }
        others
    }

    fn read_run(
        &self,
        pno: PageNo,
        count: u64,
        turn: &AtomicU64,
        fails: &mut Fails,
    ) -> Result<blockdev::AlignedBuf> {
        let (span, lba) = self.locate(pno)?;
        let mut first_err = None;
        for unit in self.readers(span, turn) {
            let mut buf = blockdev::AlignedBuf::new(usize::try_from(count).expect("count fits"));
            match self.dev(unit).expect("reader is attached").read(lba, &mut buf) {
                Ok(()) => return Ok(buf),
                Err(e) => {
                    let what = format!("read {count} pages at {pno}: {e}");
                    if !self.fail(span, unit, what, fails) {
                        return Err(e.into());
                    }
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.map_or_else(|| self.missing(span), StoreError::from))
    }

    fn write_bytes(&self, pno: PageNo, bytes: &[u8], fails: &mut Fails) -> Result<()> {
        let (span, lba) = self.locate(pno)?;
        let writers = self.writers(span);
        if writers.is_empty() {
            return Err(self.missing(span));
        }
        let mut failed = Vec::new();
        for &unit in &writers {
            if let Err(e) = self.dev(unit).expect("writer is attached").write(lba, bytes) {
                failed.push((unit, e));
            }
        }
        if failed.len() == writers.len() {
            return Err(failed.remove(0).1.into());
        }
        for (unit, e) in failed {
            self.fail(span, unit, format!("write page {pno}: {e}"), fails);
        }
        Ok(())
    }

    fn flush_span(&self, span: u16, fails: &mut Fails) -> Result<()> {
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
            self.fail(span, unit, format!("flush: {e}"), fails);
        }
        Ok(())
    }
}

impl SharedIo {
    /// Read `count` consecutive pages from `pno` (within one span), as
    /// they are: no checksum verification.
    pub fn read_run(&self, pno: PageNo, count: u64) -> Result<blockdev::AlignedBuf> {
        self.shared.read_run(pno, count)
    }

    /// Write whole pages from `pno` (within one span) to every half.
    pub fn write_bytes(&self, pno: PageNo, bytes: &[u8]) -> Result<()> {
        self.shared.write_bytes(pno, bytes)
    }

    /// Flush every unit: held parity writes made, write caches flushed.
    pub fn flush(&self) -> Result<()> {
        self.shared.flush()
    }
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
            shared: Arc::new(Shared {
                core: RwLock::new(Core {
                    units,
                    layout: layout.clone(),
                }),
                turn: AtomicU64::new(0),
                events: Mutex::new(Vec::new()),
            }),
            layout,
        }
    }

    /// A handle on the same units for another thread.
    #[must_use]
    pub fn shared(&self) -> SharedIo {
        SharedIo {
            shared: Arc::clone(&self.shared),
        }
    }

    /// The layout.
    #[must_use]
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// Replace the layout after the unit table changed.
    pub fn set_layout(&mut self, layout: Layout) {
        self.shared.write().layout = layout.clone();
        self.layout = layout;
    }

    /// Number of physical units.
    #[must_use]
    pub fn unit_count(&self) -> u16 {
        u16::try_from(self.shared.read().units.len()).expect("at most MAX_UNITS")
    }

    /// A unit's state.
    #[must_use]
    pub fn unit_state(&self, unit: u16) -> UnitState {
        self.shared.read().state(unit)
    }

    /// Change a unit's state.
    pub fn set_unit_state(&mut self, unit: u16, state: UnitState) {
        self.shared.write().units[usize::from(unit)].state = state;
    }

    /// Whether a unit's device is attached.
    #[must_use]
    pub fn unit_attached(&self, unit: u16) -> bool {
        self.shared.read().dev(unit).is_some()
    }

    /// A unit's size in pages, 0 when missing.
    #[must_use]
    pub fn unit_blocks(&self, unit: u16) -> u64 {
        self.shared.read().dev(unit).map_or(0, BlockDev::blocks)
    }

    /// Append a physical unit.
    pub fn push_unit(&mut self, unit: UnitDev) {
        self.shared.write().units.push(unit);
    }

    /// Remove the last physical unit, giving its device back.
    pub fn pop_unit(&mut self) -> Option<UnitDev> {
        self.shared.write().units.pop()
    }

    /// Take a unit out of the list, shifting the ones after it down.
    pub fn remove_unit_slot(&mut self, unit: u16) -> Option<Box<dyn BlockDev>> {
        self.shared.write().units.remove(usize::from(unit)).dev
    }

    /// Write whole pages to one physical unit at a unit-relative page.
    pub fn write_unit_bytes(&mut self, unit: u16, lba: u64, bytes: &[u8]) -> Result<()> {
        match self.shared.read().dev(unit) {
            Some(d) => Ok(d.write(lba, bytes)?),
            None => Err(StoreError::UnitMissing {
                unit,
                what: "unit is not attached".into(),
            }),
        }
    }

    /// Attach a device to a unit (a replaced or returned half).
    pub fn attach(&mut self, unit: u16, dev: Box<dyn BlockDev>, state: UnitState) {
        let mut c = self.shared.write();
        let u = &mut c.units[usize::from(unit)];
        u.dev = Some(dev);
        u.state = state;
    }

    /// Detach a unit's device, keeping its table slot.
    pub fn detach(&mut self, unit: u16, state: UnitState) -> Option<Box<dyn BlockDev>> {
        let mut c = self.shared.write();
        let u = &mut c.units[usize::from(unit)];
        u.state = state;
        u.dev.take()
    }

    /// Events since the last call.
    pub fn drain_events(&mut self) -> Vec<IoEvent> {
        std::mem::take(
            &mut *self
                .shared
                .events
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// Read and verify page `pno`. A page that fails on one half of a
    /// pair is read from the other and the bad half is rewritten.
    pub fn read_page(&mut self, pno: PageNo) -> Result<Page> {
        let mut fails = Fails::new();
        let r = {
            let c = self.shared.read();
            Self::read_page_in(&c, &self.shared, pno, &mut fails)
        };
        self.shared.apply(fails);
        r
    }

    fn read_page_in(c: &Core, shared: &Shared, pno: PageNo, fails: &mut Fails) -> Result<Page> {
        let (span, lba) = c.locate(pno)?;
        let readers = c.readers(span, &shared.turn);
        let mut first_err = None;
        for (i, &unit) in readers.iter().enumerate() {
            let mut page = Page::zeroed();
            let read = c.dev(unit).expect("reader is attached").read(lba, page.bytes_mut());
            match read
                .map_err(StoreError::from)
                .and_then(|()| page.verify(pno).map(|_| ()))
            {
                Ok(()) => {
                    if i > 0 {
                        Self::heal(c, shared, span, readers[0], lba, pno, &page, fails);
                    }
                    return Ok(page);
                }
                Err(StoreError::Block(e)) => {
                    let what = format!("read page {pno}: {e}");
                    if !c.fail(span, unit, what, fails) {
                        return Err(StoreError::Block(e));
                    }
                    first_err.get_or_insert(StoreError::Block(e));
                }
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.unwrap_or_else(|| c.missing(span)))
    }

    /// Rewrite a page on the half that could not serve it.
    #[allow(clippy::too_many_arguments)]
    fn heal(c: &Core, shared: &Shared, span: u16, unit: u16, lba: u64, pno: PageNo, page: &Page, fails: &mut Fails) {
        if !c.state(unit).writable() || fails.iter().any(|f| f.1 == unit) {
            return;
        }
        let Some(dev) = c.dev(unit) else {
            return;
        };
        match dev.write(lba, page.bytes()) {
            Ok(()) => {
                warn!("asp: page {pno} rewritten on unit {unit} from its mirror");
                shared.push_event(IoEvent::Healed { page: pno, unit });
            }
            Err(e) => {
                c.fail(span, unit, format!("rewrite page {pno}: {e}"), fails);
            }
        }
    }

    /// Read `count` consecutive pages from `pno` (within one span), as
    /// they are: no checksum verification.
    pub fn read_run(&mut self, pno: PageNo, count: u64) -> Result<blockdev::AlignedBuf> {
        self.shared.read_run(pno, count)
    }

    /// Read `count` pages from `pno` on one specific unit, unverified.
    /// `None` when the unit is not attached.
    pub fn read_run_on(
        &mut self,
        unit: u16,
        pno: PageNo,
        count: u64,
    ) -> Result<Option<blockdev::AlignedBuf>> {
        let c = self.shared.read();
        let (_, lba) = c.locate(pno)?;
        let Some(dev) = c.dev(unit) else {
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
        self.shared.write_bytes(pno, bytes)
    }

    /// Write a page to one physical unit at a unit-relative page.
    pub fn write_unit_raw(&mut self, unit: u16, lba: u64, page: &mut Page) -> Result<()> {
        self.write_unit_bytes(unit, lba, page.bytes())
    }

    /// Read and verify a page from one physical unit.
    pub fn read_unit_page(&mut self, unit: u16, lba: u64) -> Result<Page> {
        let c = self.shared.read();
        let Some(d) = c.dev(unit) else {
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
        match self.shared.read().dev(unit) {
            Some(d) => Ok(d.flush()?),
            None => Ok(()),
        }
    }

    /// Flush every half of a span, waiting for all of them.
    pub fn flush_span(&mut self, span: u16) -> Result<()> {
        self.shared.flush_span(span)
    }

    /// Flush every unit.
    pub fn flush(&mut self) -> Result<()> {
        self.shared.flush()
    }

    /// Tell the units a run is unused.
    pub fn discard(&mut self, start: PageNo, len: u64) -> Result<()> {
        let c = self.shared.read();
        let (span, lba) = c.locate(start)?;
        for unit in c.writers(span) {
            c.dev(unit).expect("writer is attached").discard(lba, len)?;
        }
        Ok(())
    }
}

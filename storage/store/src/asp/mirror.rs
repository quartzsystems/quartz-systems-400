// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mirrored protection: forming and dissolving pairs, suspending and
//! resuming a half, replacing a lost half, and the resume copy that
//! brings a half back in step, chunk by chunk, with its progress kept
//! in a journal outside the store so a crash continues where it was.

use blockdev::BlockDev;
use tracing::info;

use crate::asp::io::UnitDev;
use crate::asp::layout::RESERVED_HEAD;
use crate::asp::superblock::{NO_PARTNER, UnitEntry, serial_bytes};
use crate::asp::unit::{UnitHeader, UnitState};
use crate::asp::{Asp, MIN_UNIT_SPARE, UnitInfo, wipe_unit, write_unit_header};
use crate::{Refusal, Result, StoreError};

pub use protect::mirror::{RESUME_CHUNK, ResumeProgress};

/// Magic of a resume journal.
pub const RESUME_MAGIC: &[u8; 8] = b"QS400RSM";

/// Which chunks of a span have been copied onto a resuming half: the
/// `protect` crate's map under the store's magic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeState(protect::mirror::ResumeState);

impl std::ops::Deref for ResumeState {
    type Target = protect::mirror::ResumeState;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for ResumeState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ResumeState {
    /// Nothing copied yet.
    #[must_use]
    pub fn new(asp_uuid: [u8; 16], unit_uuid: [u8; 16], span_pages: u64) -> Self {
        Self(protect::mirror::ResumeState::new(
            asp_uuid, unit_uuid, span_pages,
        ))
    }

    /// Bytes for a journal: magic, UUIDs, sizes, bitmap, then one byte,
    /// 1 for a background resume (absent in older journals: 0).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.0.encode(RESUME_MAGIC)
    }

    /// From journal bytes.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        protect::mirror::ResumeState::decode(RESUME_MAGIC, bytes).map(Self)
    }
}

/// Keeps a resume's state across a crash.
pub trait ResumeJournal {
    /// The state saved for `unit_uuid`, if any.
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState>;
    /// Make `state` durable.
    fn save(&mut self, state: &ResumeState) -> Result<()>;
    /// Forget the saved state.
    fn clear(&mut self) -> Result<()>;
}

/// A journal in memory, for tests and the temporary ASP.
#[derive(Debug, Default)]
pub struct MemJournal {
    /// The saved state.
    pub saved: Option<ResumeState>,
}

impl ResumeJournal for MemJournal {
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState> {
        self.saved.clone().filter(|s| s.unit_uuid == unit_uuid)
    }
    fn save(&mut self, state: &ResumeState) -> Result<()> {
        self.saved = Some(state.clone());
        Ok(())
    }
    fn clear(&mut self) -> Result<()> {
        self.saved = None;
        Ok(())
    }
}

/// A resume in progress.
pub struct Resume {
    unit: u16,
    span: u16,
    state: ResumeState,
    since_save: u64,
}

impl Resume {
    /// Mark it an operator's resume, which runs with the system up, and
    /// record that in the journal.
    pub fn set_background(&mut self, journal: &mut dyn ResumeJournal) -> Result<()> {
        if !self.state.background {
            self.state.background = true;
            journal.save(&self.state)?;
        }
        Ok(())
    }

    /// Progress so far.
    #[must_use]
    pub fn progress(&self) -> ResumeProgress {
        ResumeProgress {
            unit: self.unit,
            chunks_done: self.state.chunks_done(),
            chunks: self.state.chunks,
            percent: self.state.percent(),
            complete: self.state.complete(),
        }
    }
}

impl Asp {
    /// Pair `unit` with a new device: the device becomes the resuming
    /// half, the span shrinks to the smaller of the two when it must and
    /// can. The caller checkpoints and then runs the resume.
    pub fn start_mirror(
        &mut self,
        unit: u16,
        dev: Box<dyn BlockDev>,
        unit_uuid: [u8; 16],
        serial: &str,
    ) -> Result<u16> {
        let entry = self.unit_entry(unit)?.clone();
        if entry.mirrored() {
            return Err(StoreError::refused(unit, Refusal::Mirrored));
        }
        if self.sb.units.len() >= crate::asp::layout::MAX_UNITS {
            return Err(StoreError::Corrupt {
                page: 0,
                what: "the unit table is full".into(),
            });
        }
        let pages = dev.blocks();
        let need = RESERVED_HEAD + 1 + MIN_UNIT_SPARE;
        if pages < need {
            return Err(StoreError::UnitTooSmall { pages, need });
        }
        let span = self.span_of(unit)?;
        let usable = entry.usable_pages.min(pages);
        if usable < entry.usable_pages {
            // The larger is truncated to the smaller: only when the tail
            // of the span holds nothing.
            if self.ifs_in_span(span, usable - 1) {
                return Err(StoreError::refused(unit, Refusal::HoldsIfs));
            }
            self.alloc.truncate_span(span, usable)?;
        }
        let new = u16::try_from(self.sb.units.len()).expect("checked");
        let header = UnitHeader {
            unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: new,
            pages,
            mirror_of: Some(entry.unit_uuid),
        };
        self.io.push_unit(UnitDev {
            dev: Some(dev),
            state: UnitState::Resuming,
        });
        write_unit_header(&mut self.io, new, &header)?;
        let mut old = UnitHeader {
            unit_uuid: entry.unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: unit,
            pages: entry.pages,
            mirror_of: Some(unit_uuid),
        };
        rewrite_unit_header(&mut self.io, unit, &mut old)?;
        let mut layout = self.layout().clone();
        layout.set_pair(span, Some(new), usable);
        self.io.set_layout(layout.clone());
        self.alloc.set_layout(layout);
        self.io.set_unit_state(unit, UnitState::Active);
        self.sb.units.push(UnitEntry {
            unit_uuid,
            pages,
            start: entry.start,
            usable_pages: usable,
            mirror_partner: unit,
            state: UnitState::Resuming,
            serial: serial_bytes(serial),
        });
        let a = &mut self.sb.units[usize::from(unit)];
        a.mirror_partner = new;
        a.usable_pages = usable;
        a.state = UnitState::Active;
        self.sb.update_protection();
        info!(
            "asp {}: unit {unit} paired with new unit {new}, {usable} usable pages",
            self.sb.asp_number
        );
        Ok(new)
    }

    /// Dissolve a pair, keeping `keep` as an unprotected unit. The other
    /// half leaves the configuration and its header is wiped. The caller
    /// checkpoints.
    pub fn stop_mirror(&mut self, keep: u16) -> Result<Option<Box<dyn BlockDev>>> {
        let entry = self.unit_entry(keep)?.clone();
        if !entry.mirrored() {
            return Err(StoreError::refused(keep, Refusal::NotMirrored));
        }
        if self.io.unit_state(keep) != UnitState::Active {
            return Err(StoreError::refused(
                keep,
                Refusal::HalfState(self.io.unit_state(keep).to_string()),
            ));
        }
        let other = entry.mirror_partner;
        let span = self.span_of(keep)?;
        // The kept half stands alone.
        let mut header = UnitHeader {
            unit_uuid: entry.unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: keep,
            pages: entry.pages,
            mirror_of: None,
        };
        rewrite_unit_header(&mut self.io, keep, &mut header)?;
        let dev = self.drop_unit_entry(other)?;
        if let Some(d) = dev.as_ref() {
            wipe_unit(d.as_ref())?;
        }
        let keep = if other < keep { keep - 1 } else { keep };
        let mut layout = self.layout().clone();
        layout.set_pair(span, None, entry.usable_pages);
        self.io.set_layout(layout.clone());
        self.alloc.set_layout(layout);
        let e = &mut self.sb.units[usize::from(keep)];
        e.mirror_partner = NO_PARTNER;
        e.state = UnitState::Unprotected;
        self.io.set_unit_state(keep, UnitState::Unprotected);
        self.sb.update_protection();
        info!(
            "asp {}: mirroring stopped, unit {keep} kept",
            self.sb.asp_number
        );
        Ok(dev)
    }

    /// Stop writing to a half. Its partner must be active.
    pub fn suspend(&mut self, unit: u16) -> Result<()> {
        let entry = self.unit_entry(unit)?.clone();
        let partner = self.active_partner(unit, &entry)?;
        if self.io.unit_state(unit) != UnitState::Active {
            return Err(StoreError::refused(
                unit,
                Refusal::HalfState(self.io.unit_state(unit).to_string()),
            ));
        }
        self.set_state(unit, UnitState::Suspended);
        info!(
            "asp {}: unit {unit} suspended, unit {partner} carries the span",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Start bringing a half back in step with its partner. Continues a
    /// resume the journal remembers. The copy itself is done by
    /// [`resume_step`](Self::resume_step).
    pub fn resume(&mut self, unit: u16, journal: &mut dyn ResumeJournal) -> Result<Resume> {
        let entry = self.unit_entry(unit)?.clone();
        let _ = self.active_partner(unit, &entry)?;
        if !self.io.unit_attached(unit) {
            return Err(StoreError::refused(unit, Refusal::NotAttached));
        }
        match self.io.unit_state(unit) {
            UnitState::Suspended | UnitState::Resuming | UnitState::NotOperational => {}
            other => {
                return Err(StoreError::refused(
                    unit,
                    Refusal::HalfState(other.to_string()),
                ));
            }
        }
        let span = self.span_of(unit)?;
        let pages = self.layout().span(span).pages;
        let state = journal
            .load(entry.unit_uuid)
            .filter(|s| s.asp_uuid == self.sb.asp_uuid && s.chunks == pages.div_ceil(RESUME_CHUNK))
            .unwrap_or_else(|| ResumeState::new(self.sb.asp_uuid, entry.unit_uuid, pages));
        if self.io.unit_state(unit) != UnitState::Resuming {
            self.set_state(unit, UnitState::Resuming);
        }
        journal.save(&state)?;
        info!(
            "asp {}: unit {unit} resuming, {} of {} chunks already copied",
            self.sb.asp_number,
            state.chunks_done(),
            state.chunks
        );
        Ok(Resume {
            unit,
            span,
            state,
            since_save: 0,
        })
    }

    /// Copy up to `budget` chunks of the span onto the resuming half:
    /// every allocated page, reserved pages excluded. When the last
    /// chunk lands the half is active again and the journal is cleared.
    /// The caller checkpoints after completion.
    pub fn resume_step(
        &mut self,
        resume: &mut Resume,
        journal: &mut dyn ResumeJournal,
        budget: u64,
    ) -> Result<ResumeProgress> {
        let span = self.layout().span(resume.span);
        let source = span
            .members()
            .find(|&u| u != resume.unit && self.io.unit_state(u) == UnitState::Active)
            .ok_or_else(|| StoreError::UnitMissing {
                unit: resume.unit,
                what: "the active half is gone".into(),
            })?;
        let mut io = Copy {
            io: &mut self.io,
            alloc: &self.alloc,
            journal: &mut *journal,
            source,
            target: resume.unit,
            start: span.start,
            pages: span.pages,
        };
        let complete = protect::mirror::copy_step(
            &mut io,
            &mut resume.state,
            &mut resume.since_save,
            span.pages,
            budget,
        )?;
        if complete {
            self.set_state(resume.unit, UnitState::Active);
            journal.clear()?;
            info!("asp {}: unit {} resumed", self.sb.asp_number, resume.unit);
        }
        Ok(resume.progress())
    }

    /// Put a new device in the place of a missing or failed half. It
    /// becomes the resuming half. The caller checkpoints and resumes.
    pub fn replace_unit(
        &mut self,
        unit: u16,
        dev: Box<dyn BlockDev>,
        unit_uuid: [u8; 16],
        serial: &str,
    ) -> Result<()> {
        let entry = self.unit_entry(unit)?.clone();
        let partner = self.active_partner(unit, &entry)?;
        if !matches!(
            self.io.unit_state(unit),
            UnitState::Missing | UnitState::NotOperational | UnitState::Suspended
        ) {
            return Err(StoreError::refused(
                unit,
                Refusal::HalfState(self.io.unit_state(unit).to_string()),
            ));
        }
        let pages = dev.blocks();
        if pages < entry.usable_pages {
            return Err(StoreError::UnitTooSmall {
                pages,
                need: entry.usable_pages,
            });
        }
        let partner_uuid = self.sb.units[usize::from(partner)].unit_uuid;
        self.io.attach(unit, dev, UnitState::Resuming);
        let header = UnitHeader {
            unit_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: unit,
            pages,
            mirror_of: Some(partner_uuid),
        };
        write_unit_header(&mut self.io, unit, &header)?;
        let mut partner_header = UnitHeader {
            unit_uuid: partner_uuid,
            asp_uuid: self.sb.asp_uuid,
            asp_number: self.sb.asp_number,
            unit_number: partner,
            pages: self.sb.units[usize::from(partner)].pages,
            mirror_of: Some(unit_uuid),
        };
        rewrite_unit_header(&mut self.io, partner, &mut partner_header)?;
        let e = &mut self.sb.units[usize::from(unit)];
        e.unit_uuid = unit_uuid;
        e.pages = pages;
        e.serial = serial_bytes(serial);
        e.state = UnitState::Resuming;
        info!(
            "asp {}: unit {unit} replaced, resuming from unit {partner}",
            self.sb.asp_number
        );
        Ok(())
    }

    /// Mark an attached half missing (the device went away).
    pub fn set_state(&mut self, unit: u16, state: UnitState) {
        self.io.set_unit_state(unit, state);
        if let Some(e) = self.sb.units.get_mut(usize::from(unit)) {
            e.state = state;
        }
    }

    /// A unit's view, as the displays want it.
    pub fn unit_info(&self, unit: u16) -> Result<UnitInfo> {
        self.units()
            .into_iter()
            .nth(usize::from(unit))
            .ok_or_else(|| StoreError::UnitMissing {
                unit,
                what: "no such unit".into(),
            })
    }

    fn unit_entry(&self, unit: u16) -> Result<&UnitEntry> {
        self.sb
            .units
            .get(usize::from(unit))
            .ok_or_else(|| StoreError::UnitMissing {
                unit,
                what: "no such unit".into(),
            })
    }

    /// The partner of a half, which must be active.
    fn active_partner(&self, unit: u16, entry: &UnitEntry) -> Result<u16> {
        if !entry.mirrored() {
            return Err(StoreError::refused(unit, Refusal::NotMirrored));
        }
        let partner = entry.mirror_partner;
        if self.io.unit_state(partner) != UnitState::Active || !self.io.unit_attached(partner) {
            return Err(StoreError::refused(
                partner,
                Refusal::HalfState(self.io.unit_state(partner).to_string()),
            ));
        }
        Ok(partner)
    }

    /// Take a unit out of the table, renumbering the ones after it and
    /// every reference to them. Spans are unchanged. Returns the device.
    pub(crate) fn drop_unit_entry(&mut self, unit: u16) -> Result<Option<Box<dyn BlockDev>>> {
        let dev = self.io.remove_unit_slot(unit);
        self.sb.units.remove(usize::from(unit));
        for e in &mut self.sb.units {
            if e.mirror_partner != NO_PARTNER && e.mirror_partner > unit {
                e.mirror_partner -= 1;
            }
        }
        // Rewrite the headers of the renumbered units with final numbers
        // and partners.
        let table = self.sb.units.clone();
        for (i, e) in table.iter().enumerate() {
            let number = u16::try_from(i).expect("table size");
            if number < unit || !self.io.unit_attached(number) {
                continue;
            }
            let mut h = UnitHeader {
                unit_uuid: e.unit_uuid,
                asp_uuid: self.sb.asp_uuid,
                asp_number: self.sb.asp_number,
                unit_number: number,
                pages: e.pages,
                mirror_of: (e.mirror_partner != NO_PARTNER)
                    .then(|| table[usize::from(e.mirror_partner)].unit_uuid),
            };
            rewrite_unit_header(&mut self.io, number, &mut h)?;
        }
        let mut layout = self.layout().clone();
        layout.renumber_after(unit);
        self.io.set_layout(layout.clone());
        self.alloc.set_layout(layout);
        Ok(dev)
    }
}

/// One step of a resume: the active half read, the resuming half
/// written, every allocated page of the span and none of the reserved.
struct Copy<'a> {
    io: &'a mut crate::asp::io::AspIo,
    alloc: &'a crate::alloc::Allocator,
    journal: &'a mut dyn ResumeJournal,
    source: u16,
    target: u16,
    /// The span's first logical page and its size.
    start: u64,
    pages: u64,
}

impl protect::mirror::ResumeIo for Copy<'_> {
    type Error = StoreError;

    fn read(&mut self, lba: u64, count: u64) -> Result<blockdev::AlignedBuf> {
        self.io
            .read_run_on(self.source, self.start + lba, count)?
            .ok_or_else(|| StoreError::UnitMissing {
                unit: self.source,
                what: "the active half is not attached".into(),
            })
    }

    fn wanted(&mut self, lba: u64) -> bool {
        lba >= RESERVED_HEAD && lba < self.pages - 1 && !self.alloc.is_free(self.start + lba)
    }

    fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<()> {
        self.io.write_unit_bytes(self.target, lba, bytes)
    }

    fn flush(&mut self) -> Result<()> {
        self.io.flush_unit(self.target)
    }

    fn save(&mut self, state: &protect::mirror::ResumeState) -> Result<()> {
        self.journal.save(&ResumeState(state.clone()))
    }
}

/// Rewrite a unit's header pages without touching its superblock slots.
pub(crate) fn rewrite_unit_header(
    io: &mut crate::asp::io::AspIo,
    unit: u16,
    header: &mut UnitHeader,
) -> Result<()> {
    let mut page = crate::page::Page::zeroed();
    header.encode(page.body_mut());
    page.seal(
        crate::page::PageType::UnitHeader,
        crate::obj::ObjectId::NONE,
        0,
    );
    io.write_unit_raw(unit, 0, &mut page)?;
    io.write_unit_raw(unit, header.pages - 1, &mut page)?;
    io.flush_unit(unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_state_round_trips() {
        let mut s = ResumeState::new([1; 16], [2; 16], 1000);
        assert_eq!(s.chunks, 4);
        assert_eq!(s.percent(), 0);
        s.set_done(1);
        s.set_done(3);
        assert_eq!(s.chunks_done(), 2);
        assert_eq!(s.percent(), 50);
        assert!(!s.complete());
        let back = ResumeState::decode(&s.encode()).unwrap();
        assert_eq!(back, s);
        s.background = true;
        assert!(ResumeState::decode(&s.encode()).unwrap().background);
        let bytes = s.encode();
        assert!(
            !ResumeState::decode(&bytes[..bytes.len() - 1])
                .unwrap()
                .background,
            "an older journal"
        );
        s.background = false;
        assert!(ResumeState::decode(b"nothing").is_none());
        s.set_done(0);
        s.set_done(2);
        assert!(s.complete());
        assert_eq!(s.percent(), 100);
    }
}

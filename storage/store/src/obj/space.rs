// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An object's extents as one byte space of 4064-byte pages, read in
//! any span. A write changes partial pages in place through patches; a
//! span of whole pages long enough goes to fresh pages that take the
//! old ones' place in the extent list. See the format spec §10.

use crate::alloc::Run;
use crate::obj::ObjectId;
use crate::obj::header::{self, ObjectHeader};
use crate::obj::ops;
use crate::page::pager::{PageNo, Pager};
use crate::page::{BODY_SIZE, Page, PageType};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes per page of a byte space.
pub const PAGE_BYTES: u64 = BODY_SIZE as u64;
/// A whole-page span at least this long is relocated rather than
/// patched: fresh pages cost one write each, a patch that and the WAL.
pub const RELOCATE_MIN_PAGES: u64 = 4;
/// Extents an object may have in all: the inline slots and one
/// overflow page.
pub const MAX_EXTENTS: usize = header::EXTENT_SLOTS + header::OVERFLOW_SLOTS;

/// The byte space of one object.
#[derive(Debug, Clone)]
pub struct Space {
    /// The object.
    pub id: ObjectId,
    /// Its header page.
    pub header_page: PageNo,
    /// Its extents, in order.
    pub extents: Vec<Run>,
}

impl Space {
    /// Load an object's space.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<(Self, ObjectHeader)> {
        let (header_page, h) = ops::read_header(txn, id)?;
        let extents = ops::all_extents(txn, &h)?;
        Ok((
            Self {
                id,
                header_page,
                extents,
            },
            h,
        ))
    }

    /// Pages in the space.
    #[must_use]
    pub fn pages(&self) -> u64 {
        self.extents.iter().map(|r| r.len).sum()
    }

    /// Bytes in the space.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.pages() * PAGE_BYTES
    }

    /// The page holding byte `off` and the offset within its body.
    #[must_use]
    pub fn locate(&self, off: u64) -> Option<(PageNo, usize)> {
        let mut index = off / PAGE_BYTES;
        for r in &self.extents {
            if index < r.len {
                return Some((
                    r.start + index,
                    usize::try_from(off % PAGE_BYTES).expect("in page"),
                ));
            }
            index -= r.len;
        }
        None
    }

    fn out_of_space(&self, off: u64, len: usize) -> StoreError {
        StoreError::Corrupt {
            page: self.header_page,
            what: format!(
                "{off}+{len} is outside the {}-byte space of {}",
                self.bytes(),
                self.id
            ),
        }
    }

    /// Read `len` bytes at `off`.
    pub fn read(&self, txn: &mut Txn<'_>, off: u64, len: usize) -> Result<Vec<u8>> {
        if off + len as u64 > self.bytes() {
            return Err(self.out_of_space(off, len));
        }
        let mut out = Vec::with_capacity(len);
        let mut at = off;
        while out.len() < len {
            let (page, in_page) = self.locate(at).expect("checked");
            let take = (BODY_SIZE - in_page).min(len - out.len());
            let p = txn.read(page)?;
            p.verify_type(page, PageType::ObjectData)?;
            out.extend_from_slice(&p.body()[in_page..in_page + take]);
            at += take as u64;
        }
        Ok(out)
    }

    /// Change bytes at `off`, moving whole pages. Partial pages are
    /// patched in place; a span of at least `RELOCATE_MIN_PAGES` whole
    /// pages is written to fresh pages that replace the old ones in the
    /// extent list, unless the list would then not fit, when it is
    /// patched too. The space is current afterwards; any other copy of
    /// it is stale.
    pub fn write_relocating(&mut self, txn: &mut Txn<'_>, off: u64, bytes: &[u8]) -> Result<()> {
        let end = off + bytes.len() as u64;
        if end > self.bytes() {
            return Err(self.out_of_space(off, bytes.len()));
        }
        let first_full = off.div_ceil(PAGE_BYTES);
        let end_full = end / PAGE_BYTES;
        if end_full <= first_full || end_full - first_full < RELOCATE_MIN_PAGES {
            return self.write(txn, off, bytes);
        }
        let at = |index: u64| usize::try_from(index * PAGE_BYTES - off).expect("in bytes");
        if !self.relocate(txn, first_full, end_full, &bytes[at(first_full)..at(end_full)])? {
            return self.write(txn, off, bytes);
        }
        let head = at(first_full);
        if head > 0 {
            self.write(txn, off, &bytes[..head])?;
        }
        let tail = at(end_full);
        if tail < bytes.len() {
            self.write(txn, end_full * PAGE_BYTES, &bytes[tail..])?;
        }
        Ok(())
    }

    /// Change bytes at `off` in place, page by page, through patches.
    pub fn write(&self, txn: &mut Txn<'_>, off: u64, bytes: &[u8]) -> Result<()> {
        if off + bytes.len() as u64 > self.bytes() {
            return Err(self.out_of_space(off, bytes.len()));
        }
        let mut done = 0;
        while done < bytes.len() {
            let (page, in_page) = self.locate(off + done as u64).expect("checked");
            let take = (BODY_SIZE - in_page).min(bytes.len() - done);
            txn.patch(page, in_page, &bytes[done..done + take])?;
            done += take;
        }
        Ok(())
    }

    /// Write pages `first..end` of the space, whose new contents are
    /// `bytes`, to fresh pages and put those in the extent list in the
    /// old pages' place; the old pages are freed. `Ok(false)` when the
    /// list would outgrow its slots, with nothing changed.
    fn relocate(&mut self, txn: &mut Txn<'_>, first: u64, end: u64, bytes: &[u8]) -> Result<bool> {
        let mut extents: Vec<Run> = Vec::new();
        // Fresh runs and the index of the first space page each holds.
        let mut fresh: Vec<(Run, u64)> = Vec::new();
        let mut freed: Vec<Run> = Vec::new();
        let mut base = 0;
        for r in &self.extents {
            let lo = first.max(base);
            let hi = end.min(base + r.len);
            if lo >= hi {
                push_run(&mut extents, *r);
                base += r.len;
                continue;
            }
            if lo > base {
                push_run(
                    &mut extents,
                    Run {
                        start: r.start,
                        len: lo - base,
                    },
                );
            }
            let mut index = lo;
            while index < hi {
                let run = match txn.alloc_run(hi - index) {
                    Ok(run) => run,
                    Err(e) => {
                        give_back(txn, &fresh)?;
                        return Err(e);
                    }
                };
                push_run(&mut extents, run);
                fresh.push((run, index));
                index += run.len;
            }
            freed.push(Run {
                start: r.start + (lo - base),
                len: hi - lo,
            });
            if hi < base + r.len {
                push_run(
                    &mut extents,
                    Run {
                        start: r.start + (hi - base),
                        len: base + r.len - hi,
                    },
                );
            }
            base += r.len;
        }
        if extents.len() > MAX_EXTENTS {
            give_back(txn, &fresh)?;
            return Ok(false);
        }
        for (run, index) in &fresh {
            for i in 0..run.len {
                let from = usize::try_from((index + i - first) * PAGE_BYTES).expect("in bytes");
                let mut p = Page::zeroed();
                p.body_mut().copy_from_slice(&bytes[from..from + BODY_SIZE]);
                txn.write(run.start + i, &mut p, PageType::ObjectData, self.id)?;
            }
        }
        for r in &freed {
            txn.free_run(r.start, r.len)?;
        }
        let (page, mut h) = ops::read_header(txn, self.id)?;
        ops::save_extents(txn, page, &mut h, &extents)?;
        self.extents = extents;
        Ok(true)
    }

    /// Grow the space to hold at least `bytes`, filling new pages with
    /// `fill`. Returns the space after growth.
    pub fn ensure(txn: &mut Txn<'_>, id: ObjectId, bytes: u64, fill: u8) -> Result<Self> {
        let (space, _) = Self::load(txn, id)?;
        if space.bytes() >= bytes {
            return Ok(space);
        }
        let need = bytes.div_ceil(PAGE_BYTES) - space.pages();
        for run in ops::grow(txn, id, need)? {
            for page in run.start..run.start + run.len {
                let mut p = Page::zeroed();
                p.body_mut().fill(fill);
                txn.write(page, &mut p, PageType::ObjectData, id)?;
            }
        }
        Ok(Self::load(txn, id)?.0)
    }

    /// Shrink the space to `bytes` (rounded up to pages).
    pub fn truncate(txn: &mut Txn<'_>, id: ObjectId, bytes: u64) -> Result<Self> {
        let (space, _) = Self::load(txn, id)?;
        let keep = bytes.div_ceil(PAGE_BYTES);
        if keep < space.pages() {
            ops::shrink(txn, id, space.pages() - keep)?;
        }
        Ok(Self::load(txn, id)?.0)
    }
}

/// Append a run, merging it with the last when they are adjacent.
fn push_run(extents: &mut Vec<Run>, run: Run) {
    if run.len == 0 {
        return;
    }
    match extents.last_mut() {
        Some(last) if last.start + last.len == run.start => last.len += run.len,
        _ => extents.push(run),
    }
}

/// Return fresh runs the relocation will not use.
fn give_back(txn: &mut Txn<'_>, fresh: &[(Run, u64)]) -> Result<()> {
    for (run, _) in fresh {
        txn.free_run(run.start, run.len)?;
    }
    Ok(())
}

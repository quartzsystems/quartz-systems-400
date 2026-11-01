// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Held pages: runs of a span the allocator never hands out, never
//! lists as free in its tree and never frees on a rebuild. Device
//! parity holds the tail of each member (`docs/STORE_FORMAT.md` §11).
//! Holds are not persisted: the ASP sets them again at open.

use super::{Allocator, Extent};

impl Allocator {
    /// Hold `len` pages of a span from `lba`. Free pages stop being
    /// free at once; allocated ones (the last checkpoint's tree and log,
    /// pages freed in this window) stay out once released.
    pub fn hold(&mut self, span: u16, lba: u64, len: u64) {
        if len == 0 {
            return;
        }
        let e = Extent { unit: span, lba, len };
        self.held.remove(e);
        self.held.insert(e);
        let bm = &mut self.bitmaps[usize::from(span)];
        for p in lba..(lba + len).min(bm.pages()) {
            if bm.is_free(p) {
                bm.clear(p, 1);
            }
        }
    }

    /// Stop holding. The pages become free at the next checkpoint, like
    /// freed pages: until then the last durable state may still reach
    /// them.
    pub fn unhold(&mut self, span: u16, lba: u64, len: u64) {
        if len == 0 {
            return;
        }
        let e = Extent { unit: span, lba, len };
        let held = self.held.clone();
        self.held.remove(e);
        let meta = self.metadata_extents();
        for h in held.iter().filter(|h| h.unit == span) {
            let lo = h.lba.max(lba);
            let hi = h.end().min(lba + len);
            for p in lo..hi {
                let one = Extent { unit: span, lba: p, len: 1 };
                if !meta.overlaps(one) && !self.pending.overlaps(one) && !self.bitmaps[usize::from(span)].is_free(p) {
                    self.pending.insert(one);
                }
            }
        }
    }

    /// Pages of a span from `lba` that hold something: allocated, not
    /// held, not the allocator's own tree or log, not freed in this
    /// window. Holding them would lose data.
    #[must_use]
    pub fn in_use(&self, span: u16, lba: u64, len: u64) -> u64 {
        let bm = &self.bitmaps[usize::from(span)];
        let meta = self.metadata_extents();
        (lba..(lba + len).min(bm.pages().saturating_sub(1)))
            .filter(|&p| {
                let one = Extent { unit: span, lba: p, len: 1 };
                !bm.is_free(p)
                    && !self.held.overlaps(one)
                    && !meta.overlaps(one)
                    && !self.pending.overlaps(one)
            })
            .count() as u64
    }

    /// Whether a logical page is held.
    #[must_use]
    pub fn is_held(&self, pno: crate::page::pager::PageNo) -> bool {
        self.layout
            .locate(pno)
            .is_some_and(|(unit, lba)| self.held.overlaps(Extent { unit, lba, len: 1 }))
    }

    /// Pages held on a span.
    #[must_use]
    pub fn held_pages(&self, span: u16) -> u64 {
        self.held.iter().filter(|e| e.unit == span).map(|e| e.len).sum()
    }

    /// The tree and log pages of the last checkpoint, as runs.
    fn metadata_extents(&self) -> super::RangeSet {
        let mut out = super::RangeSet::default();
        for pno in self.tree_pages.iter().chain(&self.log_pages) {
            if let Some((unit, lba)) = self.layout.locate(*pno) {
                let one = Extent { unit, lba, len: 1 };
                if !out.overlaps(one) {
                    out.insert(one);
                }
            }
        }
        out
    }

    /// Clear the free bits of every held page (after a rebuild).
    pub(super) fn clear_held(&mut self) {
        for e in self.held.clone().iter() {
            let bm = &mut self.bitmaps[usize::from(e.unit)];
            for p in e.lba..e.end().min(bm.pages()) {
                if bm.is_free(p) {
                    bm.clear(p, 1);
                }
            }
        }
    }

    /// Take the held pages out of a set of runs.
    pub(super) fn without_held(&self, set: &mut super::RangeSet) {
        for e in self.held.iter() {
            set.remove(e);
        }
    }
}

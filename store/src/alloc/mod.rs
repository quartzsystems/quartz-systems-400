// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Free-space management. Between checkpoints the per-unit bitmaps are
//! the truth; a checkpoint writes a fresh free-space tree from them and
//! a free log for the few pages the tree cannot describe. See
//! `docs/STORE_FORMAT.md` §7.

pub mod bitmap;
mod chunk;
pub mod freelog;
mod ranges;
pub mod tree;

use tracing::debug;

use crate::asp::io::AspIo;
use crate::asp::layout::{Layout, RESERVED_HEAD};
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::{Result, StoreError};
use bitmap::Bitmap;
use chunk::ChunkPager;
pub use chunk::ReadOnly;
use ranges::RangeSet;
use tree::FreeTree;

/// A run of pages in unit coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Extent {
    /// Unit.
    pub unit: u16,
    /// First page, unit-relative.
    pub lba: u64,
    /// Length in pages.
    pub len: u64,
}

impl Extent {
    /// One past the last page.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.lba + self.len
    }
}

/// A run of pages in logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// First page.
    pub start: PageNo,
    /// Length in pages.
    pub len: u64,
}

/// Free-space figures for one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitStats {
    /// Unit number.
    pub unit: u16,
    /// Size in pages.
    pub pages: u64,
    /// Free now.
    pub free: u64,
    /// Freed since the last checkpoint, free after the next.
    pub pending: u64,
}

/// What a checkpoint publishes in the superblock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointRoots {
    /// Free-space tree root.
    pub alloc_root: Option<PageNo>,
    /// First free-log page.
    pub free_log_head: Option<PageNo>,
}

/// Pages kept back from `alloc` so a checkpoint on a full ASP can still
/// write its tree and log: 1/64 of the ASP within these bounds.
const RESERVE_MIN: u64 = 64;
const RESERVE_MAX: u64 = 16384;
/// Most chunks one checkpoint draws its pages from.
const MAX_CHUNKS: usize = 8;

/// The allocator of one ASP.
pub struct Allocator {
    layout: Layout,
    bitmaps: Vec<Bitmap>,
    /// Allocated since the last checkpoint, for the WAL.
    allocs: RangeSet,
    /// Freed since the last checkpoint; allocatable after the next.
    pending: RangeSet,
    /// Pages of the tree written by the last checkpoint.
    tree_pages: Vec<PageNo>,
    /// Pages of the free log written by the last checkpoint.
    log_pages: Vec<PageNo>,
    alloc_root: Option<PageNo>,
    /// Pages only a checkpoint may take.
    reserve: u64,
    /// In the middle of a checkpoint: the state to publish at the end.
    staged: Option<Staged>,
}

/// Checkpoint results waiting for the superblock write.
struct Staged {
    /// Free once the superblock is durable.
    release: RangeSet,
    tree_pages: Vec<PageNo>,
    log_pages: Vec<PageNo>,
}

impl Allocator {
    /// Everything allocated: the state of a unit set with no store yet.
    fn blank(layout: Layout) -> Self {
        let bitmaps = layout
            .spans()
            .iter()
            .map(|s| Bitmap::new(s.pages))
            .collect();
        let reserve = (layout.total_pages() / 64).clamp(RESERVE_MIN, RESERVE_MAX);
        Self {
            reserve,
            layout,
            bitmaps,
            allocs: RangeSet::default(),
            pending: RangeSet::default(),
            tree_pages: Vec::new(),
            log_pages: Vec::new(),
            alloc_root: None,
            staged: None,
        }
    }

    /// For a fresh ASP: every non-reserved page is free; the first
    /// checkpoint writes the tree.
    #[must_use]
    pub fn format(layout: Layout, wal_pages: u64) -> Self {
        let mut a = Self::blank(layout);
        for (unit, span) in a.layout.spans().iter().enumerate() {
            let head = RESERVED_HEAD + if unit == 0 { wal_pages } else { 0 };
            a.bitmaps[unit].set(head, span.pages - 1 - head);
        }
        a
    }

    /// Rebuild from the tree and the free log of a checkpoint.
    pub fn load(
        io: &mut AspIo,
        layout: Layout,
        alloc_root: Option<PageNo>,
        free_log_head: Option<PageNo>,
    ) -> Result<Self> {
        let mut a = Self::blank(layout);
        a.alloc_root = alloc_root;
        let tree = FreeTree::open(alloc_root, ObjectId::NONE);
        let mut reader = ReadOnly(io);
        for e in tree::scan(&tree, &mut reader)? {
            a.mark_free(e)?;
        }
        a.tree_pages = tree.pages(&mut reader)?;
        let (entries, pages) = freelog::read(io, free_log_head)?;
        for e in entries {
            a.mark_free(e)?;
        }
        a.log_pages = pages;
        Ok(a)
    }

    fn mark_free(&mut self, e: Extent) -> Result<()> {
        let u = usize::from(e.unit);
        let bad = |what: String| StoreError::Corrupt {
            page: self.layout.logical(e.unit, e.lba),
            what,
        };
        if u >= self.bitmaps.len()
            || e.len == 0
            || e.end() > self.layout.span(e.unit).pages - 1
            || e.lba < RESERVED_HEAD
        {
            return Err(bad(format!(
                "free run {e:?} outside the unit's allocatable pages"
            )));
        }
        if !self.bitmaps[u].is_clear(e.lba, e.len) {
            return Err(bad(format!("free run {e:?} overlaps another")));
        }
        self.bitmaps[u].set(e.lba, e.len);
        Ok(())
    }

    /// Up to `pages` contiguous pages, at least one: first fit in the
    /// unit with the most free space, else the largest run there.
    pub fn alloc(&mut self, pages: u64) -> Result<Run> {
        let available = self.available();
        if available == 0 {
            return Err(StoreError::NoSpace { asp: 0 });
        }
        let pages = pages.clamp(1, available);
        let unit = (0..self.bitmaps.len())
            .max_by_key(|&u| self.bitmaps[u].free())
            .expect("at least one unit");
        let bm = &mut self.bitmaps[unit];
        if bm.free() == 0 {
            return Err(StoreError::NoSpace { asp: 0 });
        }
        let (lba, len) = match bm.first_fit(pages) {
            Some(lba) => (lba, pages),
            None => bm.largest_run(pages).expect("free > 0"),
        };
        bm.clear(lba, len);
        let unit = u16::try_from(unit).expect("unit");
        self.allocs.insert(Extent { unit, lba, len });
        Ok(Run {
            start: self.layout.logical(unit, lba),
            len,
        })
    }

    /// One page.
    pub fn alloc_page(&mut self) -> Result<PageNo> {
        Ok(self.alloc(1)?.start)
    }

    /// Grow a run whose last page is `end - 1` by up to `want` pages in
    /// place. Returns how many were added, possibly zero.
    pub fn extend(&mut self, end: PageNo, want: u64) -> u64 {
        let Some((unit, lba)) = self.layout.locate(end) else {
            return 0;
        };
        let want = want.min(self.available());
        let bm = &mut self.bitmaps[usize::from(unit)];
        let got = bm.free_run_from(lba, want);
        if got > 0 {
            bm.clear(lba, got);
            self.allocs.insert(Extent {
                unit,
                lba,
                len: got,
            });
        }
        got
    }

    /// Give a run back. It becomes allocatable after the next
    /// checkpoint. Freeing a page that is free, reserved or already
    /// pending is a corruption error.
    pub fn free(&mut self, start: PageNo, len: u64) -> Result<()> {
        let bad = |what: &str| StoreError::Corrupt {
            page: start,
            what: format!("free of {len} pages at {start}: {what}"),
        };
        let Some((unit, lba)) = self.layout.locate(start) else {
            return Err(bad("beyond the ASP"));
        };
        let span = self.layout.span(unit);
        if len == 0 || lba < RESERVED_HEAD || lba + len > span.pages - 1 {
            return Err(bad("reserved or beyond the unit"));
        }
        if !self.bitmaps[usize::from(unit)].is_clear(lba, len) {
            return Err(bad("already free"));
        }
        let e = Extent { unit, lba, len };
        if self.pending.overlaps(e) {
            return Err(bad("already freed"));
        }
        self.pending.insert(e);
        Ok(())
    }

    /// Redo an allocation from the WAL: the pages must be free.
    pub fn replay_alloc(&mut self, e: Extent) -> Result<()> {
        let bm = self
            .bitmaps
            .get_mut(usize::from(e.unit))
            .ok_or_else(|| StoreError::Corrupt {
                page: 0,
                what: format!("WAL allocation on unit {}", e.unit),
            })?;
        if e.len == 0 || e.end() > bm.pages() || (e.lba..e.end()).any(|p| !bm.is_free(p)) {
            return Err(StoreError::Corrupt {
                page: self.layout.logical(e.unit, e.lba),
                what: format!("WAL allocation {e:?} of pages that are not free"),
            });
        }
        bm.clear(e.lba, e.len);
        self.allocs.insert(e);
        Ok(())
    }

    /// Redo a free from the WAL.
    pub fn replay_free(&mut self, e: Extent) -> Result<()> {
        self.free(self.layout.logical(e.unit, e.lba), e.len)
    }

    /// Undo an allocation of an aborted transaction: nothing durable
    /// refers to the pages, so they are free at once.
    pub fn rollback_alloc(&mut self, start: PageNo, len: u64) {
        let (unit, lba) = self.layout.locate(start).expect("own run");
        self.bitmaps[usize::from(unit)].set(lba, len);
        self.allocs.remove(Extent { unit, lba, len });
    }

    /// Undo a free of an aborted transaction.
    pub fn rollback_free(&mut self, start: PageNo, len: u64) {
        let (unit, lba) = self.layout.locate(start).expect("own run");
        self.pending.remove(Extent { unit, lba, len });
    }

    /// Unit coordinates of a logical run.
    #[must_use]
    pub fn extent_of(&self, run: Run) -> Extent {
        let (unit, lba) = self.layout.locate(run.start).expect("own run");
        Extent {
            unit,
            lba,
            len: run.len,
        }
    }

    /// Whether a logical page is free now.
    #[must_use]
    pub fn is_free(&self, pno: PageNo) -> bool {
        self.layout
            .locate(pno)
            .is_some_and(|(unit, lba)| self.bitmaps[usize::from(unit)].is_free(lba))
    }

    /// Replace the free set wholesale (RCLSTG): every page not in `free`
    /// is allocated. Pending frees and the allocation log are dropped;
    /// the previous tree and log pages stay allocated until the next
    /// checkpoint releases them, so `free` must not include them.
    pub fn rebuild(&mut self, free: &[Extent]) -> Result<()> {
        for bm in &mut self.bitmaps {
            *bm = Bitmap::new(bm.pages());
        }
        self.allocs = RangeSet::default();
        self.pending = RangeSet::default();
        for e in free {
            self.mark_free(*e)?;
        }
        Ok(())
    }

    /// Pages of the free-space tree as of the last checkpoint.
    #[must_use]
    pub fn tree_pages(&self) -> &[PageNo] {
        &self.tree_pages
    }

    /// Pages of the free log as of the last checkpoint.
    #[must_use]
    pub fn log_pages(&self) -> &[PageNo] {
        &self.log_pages
    }

    /// Free pages now, all units, the checkpoint reserve included.
    #[must_use]
    pub fn total_free(&self) -> u64 {
        self.bitmaps.iter().map(Bitmap::free).sum()
    }

    /// Pages `alloc` may still hand out: free minus the reserve.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.total_free().saturating_sub(self.reserve)
    }

    /// Pages kept for checkpoints.
    #[must_use]
    pub fn reserve(&self) -> u64 {
        self.reserve
    }

    /// Pages of the tree and the log of the last checkpoint.
    #[must_use]
    pub fn metadata_pages(&self) -> u64 {
        (self.tree_pages.len() + self.log_pages.len()) as u64
    }

    /// Per-unit figures.
    #[must_use]
    pub fn unit_stats(&self) -> Vec<UnitStats> {
        self.bitmaps
            .iter()
            .enumerate()
            .map(|(u, bm)| {
                let unit = u16::try_from(u).expect("unit");
                UnitStats {
                    unit,
                    pages: self.layout.span(unit).pages,
                    free: bm.free(),
                    pending: self
                        .pending
                        .iter()
                        .filter(|e| e.unit == unit)
                        .map(|e| e.len)
                        .sum(),
                }
            })
            .collect()
    }

    /// Free runs now, in unit coordinates, ascending.
    #[must_use]
    pub fn free_extents(&self) -> Vec<Extent> {
        let mut out = Vec::new();
        for (u, bm) in self.bitmaps.iter().enumerate() {
            let unit = u16::try_from(u).expect("unit");
            out.extend(bm.runs().map(|(lba, len)| Extent { unit, lba, len }));
        }
        out
    }

    /// Allocations since the last checkpoint, for the WAL.
    #[must_use]
    pub fn allocs_since_checkpoint(&self) -> Vec<Extent> {
        self.allocs.iter().collect()
    }

    /// Frees since the last checkpoint, for the WAL.
    #[must_use]
    pub fn frees_since_checkpoint(&self) -> Vec<Extent> {
        self.pending.iter().collect()
    }

    /// The tree root as of the last checkpoint.
    #[must_use]
    pub fn alloc_root(&self) -> Option<PageNo> {
        self.alloc_root
    }

    /// Write the tree and the free log for a checkpoint with
    /// `generation`. The caller writes the superblock with the returned
    /// roots, flushes, and then calls [`finish_checkpoint`](Self::finish_checkpoint).
    ///
    /// The new tree describes what is free once this checkpoint is
    /// durable: the free runs now, this window's frees, and the pages of
    /// the previous tree and log. Its own pages come from chunks taken
    /// from the free runs first; the unused tail of each chunk goes into
    /// the log.
    pub fn prepare_checkpoint(
        &mut self,
        io: &mut AspIo,
        generation: u64,
    ) -> Result<CheckpointRoots> {
        assert!(self.staged.is_none(), "checkpoint already prepared");
        let mut free = RangeSet::default();
        for e in self.free_extents() {
            free.insert(e);
        }
        for e in self.pending.iter() {
            free.insert(e);
        }
        for pno in self.tree_pages.iter().chain(&self.log_pages) {
            let (unit, lba) = self.layout.locate(*pno).expect("own page");
            free.insert(Extent { unit, lba, len: 1 });
        }
        // Chunks: enough for the tree over `free` and one log page.
        let mut chunks: Vec<Extent> = Vec::new();
        let mut have = 0;
        loop {
            let need = FreeTree::bulk_load_pages(free.len() + chunks.len(), 8) + 1;
            if have >= need {
                break;
            }
            if chunks.len() >= MAX_CHUNKS {
                return Err(StoreError::NoSpace { asp: 0 });
            }
            let Some(c) = self.take_chunk(need - have) else {
                return Err(StoreError::NoSpace { asp: 0 });
            };
            free.remove(c);
            have += c.len;
            chunks.push(c);
        }
        let mut cp = ChunkPager::new(io, &self.layout, chunks, generation);
        let entries: Vec<((u16, u64), u64)> =
            free.iter().map(|e| ((e.unit, e.lba), e.len)).collect();
        let runs = entries.len();
        let tree = FreeTree::bulk_load(&mut cp, ObjectId::NONE, entries)?;
        let tree_pages = cp.taken().to_vec();
        // One log page, then whatever is left of the chunks is its
        // content. The log is written even when empty so the page is
        // accounted for.
        let log_page = cp.alloc()?;
        let tails = cp.tails();
        let head = freelog::write(&mut cp, &[log_page], &tails)?;
        drop(cp);
        let mut release = self.pending.clone();
        for pno in self.tree_pages.iter().chain(&self.log_pages) {
            let (unit, lba) = self.layout.locate(*pno).expect("own page");
            release.insert(Extent { unit, lba, len: 1 });
        }
        for e in &tails {
            release.insert(*e);
        }
        debug!(
            "allocator: checkpoint {generation}: {} free runs, {} tree pages, {} tail runs",
            runs,
            tree_pages.len(),
            tails.len()
        );
        self.alloc_root = tree.root();
        self.staged = Some(Staged {
            release,
            tree_pages,
            log_pages: head.into_iter().collect(),
        });
        Ok(CheckpointRoots {
            alloc_root: self.alloc_root,
            free_log_head: head,
        })
    }

    /// The superblock is durable: pages freed in the window, the old
    /// tree and log, and the chunk tails are allocatable now.
    pub fn finish_checkpoint(&mut self) {
        let staged = self.staged.take().expect("prepare_checkpoint first");
        for e in staged.release.iter() {
            self.bitmaps[usize::from(e.unit)].set(e.lba, e.len);
        }
        self.tree_pages = staged.tree_pages;
        self.log_pages = staged.log_pages;
        self.pending = RangeSet::default();
        self.allocs = RangeSet::default();
    }

    /// Take up to `want` contiguous pages from the unit with the most
    /// free space, for a checkpoint.
    fn take_chunk(&mut self, want: u64) -> Option<Extent> {
        let unit = (0..self.bitmaps.len()).max_by_key(|&u| self.bitmaps[u].free())?;
        let (lba, len) = self.bitmaps[unit].largest_run(want)?;
        self.bitmaps[unit].clear(lba, len);
        Some(Extent {
            unit: u16::try_from(unit).expect("unit"),
            lba,
            len,
        })
    }
}

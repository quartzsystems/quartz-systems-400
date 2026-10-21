// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where pages come from. The B+tree and the object layer talk to a
//! [`Pager`]: read a verified page, take a fresh page, write one, give
//! one back. The transaction layer implements it for real; [`MemPager`]
//! is the test implementation with commit and crash.

use blockdev::{BlockDev, MemBlockDev};

use crate::obj::ObjectId;
use crate::page::{PAGE_SIZE, Page, PageType};
use crate::{Result, StoreError};

/// Logical page number within an ASP.
pub type PageNo = u64;

/// Page supplier for the trees and objects.
pub trait Pager {
    /// Read page `pno`, verifying its checksum.
    fn read(&mut self, pno: PageNo) -> Result<Page>;

    /// Take a fresh page. Its contents are undefined until written.
    fn alloc(&mut self) -> Result<PageNo>;

    /// Seal `page` as `kind` owned by `owner` with this transaction's
    /// generation, and write it to `pno`.
    fn write(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
    ) -> Result<()>;

    /// Give `pno` back. It is not reused before the next checkpoint, so
    /// a reader of the old tree never sees a recycled page.
    fn free(&mut self, pno: PageNo) -> Result<()>;

    /// Whether `pno` was allocated by the transaction in flight. Such a
    /// page is invisible until commit, so a tree may rewrite it in place
    /// instead of taking another.
    fn owns(&self, pno: PageNo) -> bool {
        let _ = pno;
        false
    }
}

/// Allocator state the test pager restores on crash.
#[derive(Clone)]
struct AllocState {
    next: PageNo,
    free: Vec<PageNo>,
}

/// A pager over a [`MemBlockDev`] with bump allocation, a free list,
/// and commit/crash for recovery tests. Page 0 is never handed out.
pub struct MemPager {
    dev: MemBlockDev,
    state: AllocState,
    committed: AllocState,
    /// Freed in the current transaction; reusable after commit.
    pending_free: Vec<PageNo>,
    /// Allocated in the current transaction.
    owned: std::collections::HashSet<PageNo>,
    generation: u64,
}

impl MemPager {
    /// A pager over a zeroed device of `blocks` pages.
    #[must_use]
    pub fn new(blocks: u64) -> Self {
        Self::over(MemBlockDev::new(blocks))
    }

    /// A pager over an existing device.
    #[must_use]
    pub fn over(dev: MemBlockDev) -> Self {
        let state = AllocState {
            next: 1,
            free: Vec::new(),
        };
        Self {
            dev,
            committed: state.clone(),
            state,
            pending_free: Vec::new(),
            owned: std::collections::HashSet::new(),
            generation: 1,
        }
    }

    /// Make everything written so far durable, recycle the pages freed
    /// in this transaction, and start the next generation.
    pub fn commit(&mut self) -> Result<()> {
        self.dev.flush()?;
        self.state.free.append(&mut self.pending_free);
        self.committed = self.state.clone();
        self.owned.clear();
        self.generation += 1;
        Ok(())
    }

    /// Lose every write since the last commit, as a power pull would.
    pub fn crash(&mut self) {
        self.dev.drop_unflushed();
        self.state = self.committed.clone();
        self.pending_free.clear();
        self.owned.clear();
    }

    /// The device, for tests that corrupt pages behind the pager's back.
    #[must_use]
    pub fn device(&self) -> &MemBlockDev {
        &self.dev
    }

    /// Pages handed out and not freed, as of now.
    #[must_use]
    pub fn in_use(&self) -> u64 {
        self.state.next - 1 - self.state.free.len() as u64 - self.pending_free.len() as u64
    }

    /// Generation stamped on pages written now.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Pager for MemPager {
    fn read(&mut self, pno: PageNo) -> Result<Page> {
        let mut page = Page::zeroed();
        self.dev.read(pno, page.bytes_mut())?;
        page.verify(pno)?;
        Ok(page)
    }

    fn alloc(&mut self) -> Result<PageNo> {
        if let Some(p) = self.state.free.pop() {
            self.owned.insert(p);
            return Ok(p);
        }
        if self.state.next >= self.dev.blocks() {
            return Err(StoreError::NoSpace { asp: 0 });
        }
        let p = self.state.next;
        self.state.next += 1;
        self.owned.insert(p);
        Ok(p)
    }

    fn write(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
    ) -> Result<()> {
        page.seal(kind, owner, self.generation);
        debug_assert_eq!(page.bytes().len(), PAGE_SIZE);
        self.dev.write(pno, page.bytes())?;
        Ok(())
    }

    fn free(&mut self, pno: PageNo) -> Result<()> {
        if self.owned.remove(&pno) {
            // Never visible to anyone: back to the free list at once.
            self.state.free.push(pno);
        } else {
            self.pending_free.push(pno);
        }
        Ok(())
    }

    fn owns(&self, pno: PageNo) -> bool {
        self.owned.contains(&pno)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_write_read_free_crash() {
        let mut p = MemPager::new(8);
        let a = p.alloc().unwrap();
        let mut page = Page::zeroed();
        page.body_mut()[0] = 1;
        p.write(a, &mut page, PageType::ObjectData, ObjectId::NONE)
            .unwrap();
        assert_eq!(p.read(a).unwrap().body()[0], 1);
        p.commit().unwrap();
        // A freed page is not reused before the commit that follows.
        p.free(a).unwrap();
        let b = p.alloc().unwrap();
        assert_ne!(a, b);
        p.write(b, &mut page, PageType::ObjectData, ObjectId::NONE)
            .unwrap();
        assert_eq!(p.in_use(), 1);
        // Crash: b is lost, a is back in use.
        p.crash();
        assert_eq!(p.in_use(), 1);
        assert!(matches!(p.read(b), Err(StoreError::Checksum { page, .. }) if page == b));
        assert_eq!(p.read(a).unwrap().body()[0], 1);
        p.free(a).unwrap();
        p.commit().unwrap();
        assert_eq!(p.alloc().unwrap(), a, "recycled after commit");
        for _ in 0..6 {
            p.alloc().unwrap();
        }
        assert!(matches!(p.alloc(), Err(StoreError::NoSpace { .. })));
    }
}

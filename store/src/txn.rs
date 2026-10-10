// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A transaction: the [`Pager`] the trees and objects write through,
//! and the in-place patches, roots and serials it publishes at commit.

use std::collections::HashMap;

use crate::alloc::Run;
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::page::{BODY_SIZE, Page, PageType};
use crate::store::Store;
use crate::wal::record::{Patch, WalRecord};
use crate::{Result, StoreError};

/// One transaction. Dropping it without `commit` aborts it.
pub struct Txn<'a> {
    store: &'a mut Store,
    id: u64,
    roots: Vec<(ObjectId, Option<PageNo>)>,
    allocs: Vec<Run>,
    frees: Vec<Run>,
    /// Pages changed in place, as this transaction sees them.
    patched: HashMap<PageNo, Page>,
    patches: Vec<Patch>,
    next_serial: u64,
    done: bool,
}

impl<'a> Txn<'a> {
    pub(crate) fn new(store: &'a mut Store, id: u64) -> Self {
        let next_serial = store.next_serial();
        Self {
            store,
            id,
            roots: Vec::new(),
            allocs: Vec::new(),
            frees: Vec::new(),
            patched: HashMap::new(),
            patches: Vec::new(),
            next_serial,
            done: false,
        }
    }

    /// The transaction ID.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The store.
    pub fn store(&mut self) -> &mut Store {
        self.store
    }

    /// Allocate up to `pages` contiguous pages.
    pub fn alloc_run(&mut self, pages: u64) -> Result<Run> {
        let run = self.store.asp_mut().alloc(pages)?;
        self.allocs.push(run);
        Ok(run)
    }

    /// Grow a run in place by up to `want` pages.
    pub fn extend_run(&mut self, end: PageNo, want: u64) -> u64 {
        let got = self.store.asp_mut().extend(end, want);
        if got > 0 {
            self.allocs.push(Run {
                start: end,
                len: got,
            });
        }
        got
    }

    /// Free a run.
    pub fn free_run(&mut self, start: PageNo, len: u64) -> Result<()> {
        self.store.asp_mut().free(start, len)?;
        for p in start..start + len {
            self.store.cache_remove(p);
            self.patched.remove(&p);
        }
        self.frees.push(Run { start, len });
        Ok(())
    }

    /// Publish a tree root. Owner `NONE` is the object directory.
    pub fn set_root(&mut self, owner: ObjectId, root: Option<PageNo>) {
        self.roots.retain(|(o, _)| *o != owner);
        self.roots.push((owner, root));
    }

    /// The object directory root as this transaction sees it.
    #[must_use]
    pub fn objdir_root(&self) -> Option<PageNo> {
        self.roots
            .iter()
            .find(|(o, _)| o.is_none())
            .map_or(self.store.objdir_root(), |(_, r)| *r)
    }

    /// Take the next object serial.
    pub fn take_serial(&mut self) -> u64 {
        let s = self.next_serial;
        self.next_serial += 1;
        s
    }

    /// Change `bytes` of the body of `page` at `offset` in place. The
    /// page keeps its type and owner. Visible to this transaction's
    /// reads at once, durable at commit.
    pub fn patch(&mut self, page: PageNo, offset: usize, bytes: &[u8]) -> Result<()> {
        if offset + bytes.len() > BODY_SIZE {
            return Err(StoreError::Corrupt {
                page,
                what: format!("patch at {offset}+{} is outside the body", bytes.len()),
            });
        }
        if !self.patched.contains_key(&page) {
            let p = self.store.read_page(page)?;
            self.patched.insert(page, p);
        }
        let p = self.patched.get_mut(&page).expect("inserted");
        p.body_mut()[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.patches.push(Patch {
            page,
            offset: u16::try_from(offset).expect("in body"),
            bytes: bytes.to_vec(),
        });
        Ok(())
    }

    /// Make the transaction durable.
    pub fn commit(mut self) -> Result<()> {
        self.done = true;
        let asp = self.store.asp();
        let allocs = self
            .allocs
            .iter()
            .map(|r| asp.allocator().extent_of(*r))
            .collect();
        let frees = self
            .frees
            .iter()
            .map(|r| asp.allocator().extent_of(*r))
            .collect();
        // Pages first changed since the checkpoint go whole; the rest
        // as patches.
        let mut images = Vec::new();
        let mut patches = Vec::new();
        let mut sealed: Vec<(PageNo, Page)> = Vec::new();
        for (pno, page) in &self.patched {
            let t = page.trailer();
            let mut p = page.clone();
            p.seal(
                PageType::from_u16(t.page_type).unwrap_or(PageType::ObjectData),
                t.owner,
                self.id,
            );
            if self.store.is_imaged(*pno) {
                patches.extend(self.patches.iter().filter(|q| q.page == *pno).cloned());
            } else {
                images.push((*pno, p.bytes().to_vec()));
            }
            sealed.push((*pno, p));
        }
        let record = WalRecord {
            txn: self.id,
            next_serial: self.next_serial,
            roots: std::mem::take(&mut self.roots),
            allocs,
            frees,
            images,
            patches,
        };
        self.store.commit(&record, sealed)
    }

    /// Undo everything: pages allocated go back, frees are cancelled.
    pub fn abort(mut self) {
        self.done = true;
        self.rollback();
    }

    fn rollback(&mut self) {
        self.store.note_abort();
        for r in self.allocs.drain(..) {
            for p in r.start..r.start + r.len {
                self.store.cache_remove(p);
            }
            self.store
                .asp_mut()
                .allocator_mut()
                .rollback_alloc(r.start, r.len);
        }
        for r in self.frees.drain(..) {
            self.store
                .asp_mut()
                .allocator_mut()
                .rollback_free(r.start, r.len);
        }
        self.patched.clear();
        self.patches.clear();
    }
}

impl Drop for Txn<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.rollback();
        }
    }
}

impl Pager for Txn<'_> {
    fn read(&mut self, pno: PageNo) -> Result<Page> {
        if let Some(p) = self.patched.get(&pno) {
            return Ok(p.clone());
        }
        self.store.read_page(pno)
    }

    fn alloc(&mut self) -> Result<PageNo> {
        Ok(self.alloc_run(1)?.start)
    }

    fn write(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
    ) -> Result<()> {
        self.patched.remove(&pno);
        self.store.write_page(pno, page, kind, owner, self.id)
    }

    fn free(&mut self, pno: PageNo) -> Result<()> {
        self.free_run(pno, 1)
    }
}

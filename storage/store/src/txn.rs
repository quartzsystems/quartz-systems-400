// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A transaction: the [`Pager`] the trees and objects write through,
//! and the in-place patches, roots and serials it publishes at commit.

use std::cell::RefMut;
use std::collections::{BTreeSet, HashMap};
use std::ops::{Deref, DerefMut};

use crate::alloc::Run;
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::page::{BODY_SIZE, Page, PageType};
use crate::store::Store;
use crate::wal::record::{Patch, WalRecord};
use crate::{Result, StoreError};

/// How a transaction reaches its store.
enum Host<'a> {
    /// Borrowed for the transaction's life.
    Owned(&'a mut Store),
    /// Borrowed through a cell; given back while the transaction is
    /// suspended (`Txn::suspend`).
    Cell {
        get: Box<dyn Fn() -> RefMut<'a, Store> + 'a>,
        held: Option<RefMut<'a, Store>>,
    },
}

const SUSPENDED: &str = "a suspended transaction is resumed before it is used";

impl Deref for Host<'_> {
    type Target = Store;

    fn deref(&self) -> &Store {
        match self {
            Self::Owned(s) => s,
            Self::Cell { held, .. } => held.as_deref().expect(SUSPENDED),
        }
    }
}

impl DerefMut for Host<'_> {
    fn deref_mut(&mut self) -> &mut Store {
        match self {
            Self::Owned(s) => s,
            Self::Cell { held, .. } => held.as_deref_mut().expect(SUSPENDED),
        }
    }
}

impl Host<'_> {
    fn suspended(&self) -> bool {
        matches!(self, Self::Cell { held: None, .. })
    }
}

/// One transaction. Dropping it without `commit` aborts it.
pub struct Txn<'a> {
    store: Host<'a>,
    id: u64,
    roots: Vec<(ObjectId, Option<PageNo>)>,
    /// Pages allocated by this transaction and not freed again.
    owned: BTreeSet<PageNo>,
    /// Owned pages as written, held back until commit.
    dirty: HashMap<PageNo, Page>,
    frees: Vec<Run>,
    /// Pages changed in place, as this transaction sees them.
    patched: HashMap<PageNo, Page>,
    patches: Vec<Patch>,
    next_serial: u64,
    done: bool,
    /// A page was written in place outside the WAL.
    wrote: bool,
    /// Objects whose pages this transaction changed: stamped at commit.
    touched: BTreeSet<ObjectId>,
}

impl<'a> Txn<'a> {
    pub(crate) fn new(store: &'a mut Store, id: u64) -> Self {
        Self::on(Host::Owned(store), id)
    }

    /// A transaction on a store that lives in a cell: `get` borrows it.
    /// Unlike one from [`Store::begin`], it can give the store back
    /// between reads ([`Txn::suspend`]), so a long read lets others in.
    pub fn lent(get: Box<dyn Fn() -> RefMut<'a, Store> + 'a>) -> Result<Self> {
        let held = get();
        if held.is_quiesced() {
            return Err(StoreError::Quiesced);
        }
        held.check()?;
        let id = held.next_txn_id();
        Ok(Self::on(Host::Cell { get, held: Some(held) }, id))
    }

    fn on(store: Host<'a>, id: u64) -> Self {
        let next_serial = store.next_serial();
        Self {
            store,
            id,
            roots: Vec::new(),
            owned: BTreeSet::new(),
            dirty: HashMap::new(),
            frees: Vec::new(),
            patched: HashMap::new(),
            patches: Vec::new(),
            next_serial,
            done: false,
            wrote: false,
            touched: BTreeSet::new(),
        }
    }

    /// The transaction ID.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The store.
    pub fn layout(&self) -> &crate::asp::layout::Layout {
        self.store.asp().layout()
    }

    /// The store, for callers that need more than a transaction.
    pub fn store(&mut self) -> &mut Store {
        &mut self.store
    }

    /// Whether the transaction has changed nothing yet.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.owned.is_empty()
            && self.frees.is_empty()
            && self.patched.is_empty()
            && self.roots.is_empty()
            && !self.wrote
            && self.next_serial == self.store.next_serial()
    }

    /// Give the store back to its cell so others can use it. What the
    /// transaction reads after [`Txn::resume`] is what others committed
    /// meanwhile. Only a transaction from [`Txn::lent`] that has changed
    /// nothing can; false otherwise.
    pub fn suspend(&mut self) -> bool {
        if self.done || self.store.suspended() || !self.is_clean() {
            return false;
        }
        match &mut self.store {
            Host::Owned(_) => false,
            Host::Cell { held, .. } => {
                if let Some(s) = held.as_mut() {
                    let _ = s.commit_empty();
                }
                *held = None;
                true
            }
        }
    }

    /// Whether the store is given back ([`Txn::suspend`]).
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.store.suspended()
    }

    /// Borrow the store again after [`Txn::suspend`], as a transaction
    /// begun now.
    pub fn resume(&mut self) -> Result<()> {
        if let Host::Cell { get, held } = &mut self.store
            && held.is_none()
        {
            *held = Some(get());
        }
        if self.store.is_quiesced() {
            return Err(StoreError::Quiesced);
        }
        self.store.check()?;
        self.id = self.store.next_txn_id();
        self.next_serial = self.store.next_serial();
        Ok(())
    }

    /// Allocate up to `pages` contiguous pages.
    pub fn alloc_run(&mut self, pages: u64) -> Result<Run> {
        let run = self.store.asp_mut().alloc(pages)?;
        self.owned.extend(run.start..run.start + run.len);
        Ok(run)
    }

    /// Grow a run in place by up to `want` pages.
    pub fn extend_run(&mut self, end: PageNo, want: u64) -> u64 {
        let got = self.store.asp_mut().extend(end, want);
        self.owned.extend(end..end + got);
        got
    }

    /// Free a run. Pages this transaction allocated go straight back;
    /// the rest wait for the next checkpoint.
    pub fn free_run(&mut self, start: PageNo, len: u64) -> Result<()> {
        let mut at = start;
        let end = start + len;
        while at < end {
            let mine = self.owned.contains(&at);
            let mut run_end = at + 1;
            while run_end < end && self.owned.contains(&run_end) == mine {
                run_end += 1;
            }
            let n = run_end - at;
            if mine {
                for p in at..run_end {
                    self.owned.remove(&p);
                    self.dirty.remove(&p);
                }
                self.store.asp_mut().allocator_mut().rollback_alloc(at, n);
            } else {
                self.store.asp_mut().free(at, n)?;
                self.frees.push(Run { start: at, len: n });
            }
            for p in at..run_end {
                self.store.cache_remove(p);
                self.patched.remove(&p);
            }
            at = run_end;
        }
        Ok(())
    }

    /// Owned pages as runs, for the WAL.
    fn owned_runs(&self) -> Vec<Run> {
        let mut runs: Vec<Run> = Vec::new();
        for &p in &self.owned {
            match runs.last_mut() {
                Some(r) if r.start + r.len == p => r.len += 1,
                _ => runs.push(Run { start: p, len: 1 }),
            }
        }
        runs
    }

    /// Publish a tree root. Owner `NONE` is the object directory.
    pub fn set_root(&mut self, owner: ObjectId, root: Option<PageNo>) {
        if owner.is_none() {
            self.store.headers_changed();
        } else {
            self.touched.insert(owner);
        }
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

    /// The header page of `id` found earlier under directory `root`.
    pub(crate) fn header_cached(&self, root: Option<PageNo>, id: ObjectId) -> Option<PageNo> {
        self.store.header_cached(root, id)
    }

    /// Remember the header page of `id` under directory `root`.
    pub(crate) fn header_found(&mut self, root: Option<PageNo>, id: ObjectId, page: PageNo) {
        self.store.header_found(root, id, page);
    }

    /// The next serial this transaction would hand out.
    #[must_use]
    pub fn peek_serial(&self) -> u64 {
        self.next_serial
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
        if let Some(p) = self.dirty.get_mut(&page) {
            // A page this transaction created: it is written whole at
            // commit, so the change needs no log entry of its own.
            p.body_mut()[offset..offset + bytes.len()].copy_from_slice(bytes);
            let t = p.trailer();
            p.seal(
                PageType::from_u16(t.page_type).unwrap_or(PageType::ObjectData),
                t.owner,
                self.id,
            );
            self.touched.insert(t.owner);
            return Ok(());
        }
        if !self.patched.contains_key(&page) {
            let p = self.store.read_page(page)?;
            self.patched.insert(page, p);
        }
        let p = self.patched.get_mut(&page).expect("inserted");
        p.body_mut()[offset..offset + bytes.len()].copy_from_slice(bytes);
        // Keep the private copy verifiable for this transaction's reads.
        let t = p.trailer();
        self.touched.insert(t.owner);
        p.seal(
            PageType::from_u16(t.page_type).unwrap_or(PageType::ObjectData),
            t.owner,
            self.id,
        );
        self.patches.push(Patch {
            page,
            offset: u16::try_from(offset).expect("in body"),
            bytes: bytes.to_vec(),
        });
        Ok(())
    }

    /// Make the transaction durable.
    pub fn commit(mut self) -> Result<()> {
        if self.done {
            // A part commit failed: nothing more is published.
            return Err(StoreError::Broken { op: "transaction" });
        }
        self.done = true;
        if self.store.suspended() {
            return Ok(());
        }
        // A transaction that only read has nothing to log.
        if self.is_clean() {
            return self.store.commit_empty();
        }
        self.publish()
    }

    /// Make what the transaction changed so far durable and go on as a
    /// transaction begun now, clean, so it can be suspended. A long
    /// change commits between rows this way. On failure the transaction
    /// is spent: its `commit` fails too.
    pub fn commit_part(&mut self) -> Result<()> {
        if self.done || self.store.suspended() {
            return Ok(());
        }
        if self.is_clean() {
            return Ok(());
        }
        if let Err(e) = self.publish() {
            self.done = true;
            return Err(e);
        }
        self.roots.clear();
        self.owned.clear();
        self.dirty.clear();
        self.frees.clear();
        self.patched.clear();
        self.patches.clear();
        self.wrote = false;
        self.touched.clear();
        self.id = self.store.next_txn_id();
        self.next_serial = self.store.next_serial();
        Ok(())
    }

    /// Set the change stamp of every object this transaction changed
    /// (`docs/STORE_FORMAT.md` §8) to its ID. An object no longer in
    /// the directory (deleted here) or without a header is skipped.
    fn stamp_changes(&mut self) {
        let ids = std::mem::take(&mut self.touched);
        let stamp = self.id.to_le_bytes();
        for id in ids {
            if id.is_none() {
                continue;
            }
            let Ok(page) = crate::obj::dir::header_page(self, id) else {
                continue;
            };
            let ok = self.read(page).is_ok_and(|p| {
                p.trailer().page_type == PageType::ObjectHeader as u16
                    && ObjectId::from_bytes(&p.body()[16..32]) == id
            });
            if ok {
                let _ = self.patch(page, crate::obj::header::OFF_CHANGE, &stamp);
            }
        }
        self.touched.clear();
    }

    /// Log and write what the transaction changed.
    fn publish(&mut self) -> Result<()> {
        self.stamp_changes();
        let asp = self.store.asp();
        let allocs = self
            .owned_runs()
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
        let fresh: Vec<(PageNo, Page)> = self.dirty.drain().collect();
        self.store.commit(&record, fresh, sealed)
    }

    /// Undo everything: pages allocated go back, frees are cancelled.
    pub fn abort(mut self) {
        self.done = true;
        self.rollback();
    }

    fn rollback(&mut self) {
        // Suspended, it holds nothing to undo.
        if self.store.suspended() {
            return;
        }
        self.store.note_abort();
        if self.roots.iter().any(|(o, _)| o.is_none()) {
            self.store.headers_changed();
        }
        for r in self.owned_runs() {
            for p in r.start..r.start + r.len {
                self.store.cache_remove(p);
            }
            self.store
                .asp_mut()
                .allocator_mut()
                .rollback_alloc(r.start, r.len);
        }
        self.owned.clear();
        self.dirty.clear();
        for r in self.frees.drain(..) {
            self.store
                .asp_mut()
                .allocator_mut()
                .rollback_free(r.start, r.len);
        }
        self.patched.clear();
        self.patches.clear();
        self.touched.clear();
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
        if let Some(p) = self.patched.get(&pno).or_else(|| self.dirty.get(&pno)) {
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
        self.touched.insert(owner);
        if self.owned.contains(&pno) {
            page.seal(kind, owner, self.id);
            self.dirty.insert(pno, page.clone());
            return Ok(());
        }
        self.wrote = true;
        self.store.write_page(pno, page, kind, owner, self.id)
    }

    fn free(&mut self, pno: PageNo) -> Result<()> {
        self.free_run(pno, 1)
    }

    fn owns(&self, pno: PageNo) -> bool {
        self.owned.contains(&pno)
    }
}

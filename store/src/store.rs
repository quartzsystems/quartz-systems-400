// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The store: an ASP with a page cache, a WAL, transactions, checkpoints
//! and recovery.

use std::collections::HashSet;

use blockdev::BlockDev;
use tracing::{info, warn};

use crate::asp::{Asp, FormatParams};
use crate::cache::{CacheStats, PageCache};
use crate::obj::ObjectId;
use crate::obj::lock::LockTable;
use crate::page::pager::PageNo;
use crate::page::{Page, PageType};
use crate::txn::Txn;
use crate::wal::Wal;
use crate::wal::record::WalRecord;
use crate::{Result, StoreError};

/// Tunables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    /// Page cache size in pages. 65536 pages is 256 MiB.
    pub cache_pages: usize,
    /// Checkpoint after this many transactions.
    pub checkpoint_txns: u64,
    /// Checkpoint when this many seconds have passed (`tick`).
    pub checkpoint_secs: u64,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            cache_pages: 65536,
            checkpoint_txns: 256,
            checkpoint_secs: 30,
        }
    }
}

/// What recovery did at open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Recovery {
    /// Transactions replayed from the WAL.
    pub replayed: u64,
    /// Superblock generation found.
    pub generation: u64,
}

/// Counters since open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StoreStats {
    /// Transactions committed.
    pub commits: u64,
    /// Transactions aborted.
    pub aborts: u64,
    /// Checkpoints taken.
    pub checkpoints: u64,
    /// WAL pages written.
    pub wal_pages: u64,
    /// Page images logged.
    pub images: u64,
    /// Patches logged.
    pub patches: u64,
    /// The cache's counters.
    pub cache: CacheStats,
}

/// An open store.
pub struct Store {
    asp: Asp,
    cache: PageCache,
    wal: Wal,
    config: StoreConfig,
    next_txn: u64,
    objdir_root: Option<PageNo>,
    next_serial: u64,
    /// Pages imaged in the WAL since the last checkpoint.
    imaged: HashSet<PageNo>,
    txns_since_checkpoint: u64,
    last_checkpoint_at: u64,
    broken: Option<&'static str>,
    stats: StoreStats,
    locks: LockTable,
}

impl Store {
    /// Format the units and open the new store.
    pub fn format(
        devs: Vec<Box<dyn BlockDev>>,
        params: &FormatParams,
        config: StoreConfig,
    ) -> Result<Self> {
        let asp = Asp::format(devs, params)?;
        let mut s = Self::from_asp(asp, config);
        s.last_checkpoint_at = params.now;
        Ok(s)
    }

    /// Open the units, replay the WAL, checkpoint.
    pub fn open(devs: Vec<Box<dyn BlockDev>>, config: StoreConfig) -> Result<(Self, Recovery)> {
        let asp = Asp::open(devs)?;
        let mut s = Self::from_asp(asp, config);
        let generation = s.asp.superblock().generation;
        let records = s.wal.scan(s.asp.io())?;
        let replayed = records.len() as u64;
        for found in records {
            let r = WalRecord::decode(&found.payload)?;
            if r.txn != found.txn {
                return Err(StoreError::Corrupt {
                    page: 0,
                    what: format!("WAL record {} carries transaction {}", found.txn, r.txn),
                });
            }
            s.replay(&r)?;
            s.next_txn = r.txn + 1;
        }
        if replayed > 0 {
            info!("store: replayed {replayed} transaction(s) from the WAL");
        }
        s.checkpoint()?;
        Ok((
            s,
            Recovery {
                replayed,
                generation,
            },
        ))
    }

    fn from_asp(asp: Asp, config: StoreConfig) -> Self {
        let sb = asp.superblock();
        let wal = Wal {
            start: sb.wal_start,
            pages: sb.wal_pages,
            head: sb.wal_head,
            tail: sb.wal_tail,
        };
        Self {
            cache: PageCache::new(config.cache_pages),
            wal,
            config,
            next_txn: sb.last_txn + 1,
            objdir_root: sb.objdir_root,
            next_serial: sb.next_serial,
            imaged: HashSet::new(),
            txns_since_checkpoint: 0,
            last_checkpoint_at: 0,
            broken: None,
            stats: StoreStats::default(),
            locks: LockTable::default(),
            asp,
        }
    }

    /// Redo one committed transaction.
    fn replay(&mut self, r: &WalRecord) -> Result<()> {
        let alloc = self.asp.allocator_mut();
        for e in &r.allocs {
            alloc.replay_alloc(*e)?;
        }
        for e in &r.frees {
            alloc.replay_free(*e)?;
        }
        for (owner, root) in &r.roots {
            self.set_root(*owner, *root)?;
        }
        for (pno, image) in &r.images {
            let mut page = Page::from_bytes(image);
            page.verify(*pno)?;
            self.asp.io().write_raw(*pno, &mut page)?;
        }
        for p in &r.patches {
            let mut page = self.asp.io().read_page(p.page)?;
            let t = page.trailer();
            let at = usize::from(p.offset);
            page.body_mut()[at..at + p.bytes.len()].copy_from_slice(&p.bytes);
            page.seal(
                PageType::from_u16(t.page_type).unwrap_or(PageType::ObjectData),
                t.owner,
                r.txn,
            );
            self.asp.io().write_raw(p.page, &mut page)?;
        }
        self.next_serial = r.next_serial;
        Ok(())
    }

    fn set_root(&mut self, owner: ObjectId, root: Option<PageNo>) -> Result<()> {
        if !owner.is_none() {
            return Err(StoreError::Corrupt {
                page: root.unwrap_or(0),
                what: format!(
                    "root published for {owner}; only the object directory has a WAL root"
                ),
            });
        }
        self.objdir_root = root;
        Ok(())
    }

    fn check(&self) -> Result<()> {
        match self.broken {
            Some(op) => Err(StoreError::Broken { op }),
            None => Ok(()),
        }
    }

    /// Start a transaction.
    pub fn begin(&mut self) -> Result<Txn<'_>> {
        self.check()?;
        let id = self.next_txn;
        Ok(Txn::new(self, id))
    }

    /// Commit: log the record, then apply the in-place pages.
    pub(crate) fn commit(&mut self, record: &WalRecord, sealed: Vec<(PageNo, Page)>) -> Result<()> {
        self.check()?;
        let payload = record.encode()?;
        let pages = Wal::pages_for(payload.len());
        if pages > self.wal.pages {
            return Err(StoreError::TxnTooLarge {
                bytes: payload.len(),
                ring_bytes: usize::try_from(self.wal.pages).unwrap_or(usize::MAX)
                    * crate::wal::PAYLOAD,
            });
        }
        if pages > self.wal.free() {
            // The ring is full: make room. The transaction's own frees
            // are pending and would be released too early if the commit
            // failed afterwards, so any failure from here on is fatal.
            self.checkpoint()?;
        }
        if let Err(e) = self.wal.append(self.asp.io(), record.txn, &payload) {
            self.broken = Some("WAL write");
            return Err(e);
        }
        self.stats.wal_pages += pages;
        self.stats.images += record.images.len() as u64;
        self.stats.patches += record.patches.len() as u64;
        for (pno, mut page) in sealed {
            if let Err(e) = self.asp.io().write_raw(pno, &mut page) {
                self.broken = Some("in-place write");
                return Err(e);
            }
            self.cache.insert(pno, page);
            self.imaged.insert(pno);
        }
        for (owner, root) in &record.roots {
            self.set_root(*owner, *root)?;
        }
        self.next_serial = record.next_serial;
        self.next_txn = record.txn + 1;
        self.txns_since_checkpoint += 1;
        self.stats.commits += 1;
        if self.txns_since_checkpoint >= self.config.checkpoint_txns
            || self.wal.free() < self.wal.pages / 2
        {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub(crate) fn note_abort(&mut self) {
        self.stats.aborts += 1;
    }

    /// Write the allocator's tree, the superblock, and advance the WAL
    /// tail past every committed transaction.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.check()?;
        let sb = self.asp.superblock_mut();
        sb.objdir_root = self.objdir_root;
        sb.next_serial = self.next_serial;
        sb.last_txn = self.next_txn - 1;
        sb.wal_head = self.wal.head;
        sb.wal_tail = self.wal.head;
        if let Err(e) = self.asp.checkpoint() {
            self.broken = Some("checkpoint");
            warn!("store: checkpoint failed: {e}");
            return Err(e);
        }
        self.wal.tail = self.wal.head;
        self.imaged.clear();
        self.txns_since_checkpoint = 0;
        self.stats.checkpoints += 1;
        Ok(())
    }

    /// Time-based checkpoint: `now` in seconds. Returns whether one ran.
    pub fn tick(&mut self, now: u64) -> Result<bool> {
        if self.txns_since_checkpoint > 0
            && now.saturating_sub(self.last_checkpoint_at) >= self.config.checkpoint_secs
        {
            self.checkpoint()?;
            self.last_checkpoint_at = now;
            return Ok(true);
        }
        if self.txns_since_checkpoint == 0 {
            self.last_checkpoint_at = now;
        }
        Ok(false)
    }

    /// Checkpoint and close.
    pub fn close(mut self) -> Result<()> {
        self.checkpoint()
    }

    /// A page, from the cache or the units.
    pub fn read_page(&mut self, pno: PageNo) -> Result<Page> {
        if let Some(p) = self.cache.get(pno) {
            return Ok(p);
        }
        let p = self.asp.io().read_page(pno)?;
        self.cache.insert(pno, p.clone());
        Ok(p)
    }

    pub(crate) fn write_page(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
        generation: u64,
    ) -> Result<()> {
        self.asp
            .io()
            .write_page(pno, page, kind, owner, generation)?;
        self.cache.insert(pno, page.clone());
        Ok(())
    }

    pub(crate) fn cache_remove(&mut self, pno: PageNo) {
        self.cache.remove(pno);
    }

    pub(crate) fn is_imaged(&self, pno: PageNo) -> bool {
        self.imaged.contains(&pno)
    }

    /// The object directory root.
    #[must_use]
    pub fn objdir_root(&self) -> Option<PageNo> {
        self.objdir_root
    }

    /// The next object serial.
    #[must_use]
    pub fn next_serial(&self) -> u64 {
        self.next_serial
    }

    /// Record QSYS in the superblock (published at the next checkpoint).
    pub fn asp_superblock_set_qsys(&mut self, qsys: ObjectId) {
        self.asp.superblock_mut().qsys = qsys;
    }

    /// Record the `*SYSVAL` object in the superblock.
    pub fn asp_superblock_set_sysval(&mut self, id: ObjectId) {
        self.asp.superblock_mut().sysval = id;
    }

    /// The ASP number objects here belong to.
    #[must_use]
    pub fn asp_number(&self) -> u16 {
        self.asp.superblock().asp_number
    }

    /// The object lock table.
    #[must_use]
    pub fn locks(&self) -> &LockTable {
        &self.locks
    }

    /// The object lock table, to take and release locks.
    pub fn locks_mut(&mut self) -> &mut LockTable {
        &mut self.locks
    }

    /// The ASP.
    #[must_use]
    pub fn asp(&self) -> &Asp {
        &self.asp
    }

    pub(crate) fn asp_mut(&mut self) -> &mut Asp {
        &mut self.asp
    }

    /// The WAL cursors.
    #[must_use]
    pub fn wal(&self) -> &Wal {
        &self.wal
    }

    /// Counters.
    #[must_use]
    pub fn stats(&self) -> StoreStats {
        let mut s = self.stats;
        s.cache = self.cache.stats();
        s
    }

    /// Transactions committed since the last checkpoint.
    #[must_use]
    pub fn txns_since_checkpoint(&self) -> u64 {
        self.txns_since_checkpoint
    }

    /// The tunables.
    #[must_use]
    pub fn config(&self) -> &StoreConfig {
        &self.config
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record locks: one holder per record of a physical member, shared by
//! every open data path of the system. A holder is a job and the open
//! data path (or commit definition) within it: a second path of the
//! same job is refused the record too, at once.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use store::obj::{JobHandle, ObjectId};

/// Who holds a record lock: a job, and the path or definition in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Holder {
    /// The job.
    pub job: JobHandle,
    /// The open data path or commit definition ([`next_owner`]).
    pub owner: u64,
}

impl Holder {
    /// A holder.
    #[must_use]
    pub fn new(job: JobHandle, owner: u64) -> Self {
        Self { job, owner }
    }
}

/// Why a lock was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Another job holds the record.
    Job(JobHandle),
    /// Another path of the same job holds it.
    SameJob,
}

/// A new owner number, for an open data path or a commit definition.
pub fn next_owner() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The record lock table.
#[derive(Debug, Default)]
pub struct RecordLocks {
    held: HashMap<(ObjectId, u64), Holder>,
}

/// The table every open data path shares.
pub type SharedLocks = Arc<Mutex<RecordLocks>>;

/// A fresh shared table.
#[must_use]
pub fn new_shared() -> SharedLocks {
    Arc::new(Mutex::new(RecordLocks::default()))
}

impl RecordLocks {
    /// Lock record `rrn` of `member` for `who`, or say who has it.
    pub fn lock(&mut self, member: ObjectId, rrn: u64, who: Holder) -> Result<(), Refused> {
        match self.held.get(&(member, rrn)) {
            Some(h) if h.job != who.job => Err(Refused::Job(h.job)),
            Some(h) if *h != who => Err(Refused::SameJob),
            _ => {
                self.held.insert((member, rrn), who);
                Ok(())
            }
        }
    }

    /// Release a lock `who` holds; false when it did not.
    pub fn unlock(&mut self, member: ObjectId, rrn: u64, who: Holder) -> bool {
        match self.held.get(&(member, rrn)) {
            Some(h) if *h == who => {
                self.held.remove(&(member, rrn));
                true
            }
            _ => false,
        }
    }

    /// Hand a lock `from` holds to `to` (a commit definition taking a
    /// path's lock); false when `from` did not hold it.
    pub fn hand_over(&mut self, member: ObjectId, rrn: u64, from: Holder, to: Holder) -> bool {
        match self.held.get_mut(&(member, rrn)) {
            Some(h) if *h == from => {
                *h = to;
                true
            }
            _ => false,
        }
    }

    /// Which job holds a record.
    #[must_use]
    pub fn holder(&self, member: ObjectId, rrn: u64) -> Option<JobHandle> {
        self.held.get(&(member, rrn)).map(|h| h.job)
    }

    /// Release everything a job holds.
    pub fn release_job(&mut self, job: JobHandle) {
        self.held.retain(|_, h| h.job != job);
    }

    /// Locks held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held.len()
    }

    /// Whether nothing is locked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

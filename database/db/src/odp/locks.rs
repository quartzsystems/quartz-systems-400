// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record locks: one holder per record of a physical member, shared by
//! every open data path of the system.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use store::obj::{JobHandle, ObjectId};

/// The record lock table.
#[derive(Debug, Default)]
pub struct RecordLocks {
    held: HashMap<(ObjectId, u64), JobHandle>,
}

/// The table every open data path shares.
pub type SharedLocks = Arc<Mutex<RecordLocks>>;

/// A fresh shared table.
#[must_use]
pub fn new_shared() -> SharedLocks {
    Arc::new(Mutex::new(RecordLocks::default()))
}

impl RecordLocks {
    /// Lock record `rrn` of `member` for `job`; the holder when another
    /// job has it.
    pub fn lock(&mut self, member: ObjectId, rrn: u64, job: JobHandle) -> Result<(), JobHandle> {
        match self.held.get(&(member, rrn)) {
            Some(holder) if *holder != job => Err(*holder),
            _ => {
                self.held.insert((member, rrn), job);
                Ok(())
            }
        }
    }

    /// Release a lock `job` holds; false when it did not.
    pub fn unlock(&mut self, member: ObjectId, rrn: u64, job: JobHandle) -> bool {
        match self.held.get(&(member, rrn)) {
            Some(holder) if *holder == job => {
                self.held.remove(&(member, rrn));
                true
            }
            _ => false,
        }
    }

    /// Who holds a record.
    #[must_use]
    pub fn holder(&self, member: ObjectId, rrn: u64) -> Option<JobHandle> {
        self.held.get(&(member, rrn)).copied()
    }

    /// Release everything a job holds.
    pub fn release_job(&mut self, job: JobHandle) {
        self.held.retain(|_, j| *j != job);
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

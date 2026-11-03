// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record locks, shared by every open data path of the system. A record
//! has at most one update lock and any number of read locks. A holder is
//! a job and the open data path (or commit definition) within it.
//!
//! - An update lock is refused while another job holds the record
//!   either way, and while another path of the same job holds its
//!   update lock (at once, as before).
//! - A read lock (a record read under `LCKLVL(*ALL)`) is refused only
//!   while another job holds the update lock. Read locks never conflict
//!   with each other, nor with the locks of their own job: a job that
//!   read a record may lock it for update when no other job read it.

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

/// The read locks on one record; most records have one reader.
#[derive(Debug, Default)]
enum Readers {
    #[default]
    None,
    One(Holder),
    Many(Vec<Holder>),
}

impl Readers {
    fn iter(&self) -> impl Iterator<Item = &Holder> {
        let (one, many): (Option<&Holder>, &[Holder]) = match self {
            Self::None => (None, &[]),
            Self::One(h) => (Some(h), &[]),
            Self::Many(v) => (None, v),
        };
        one.into_iter().chain(many.iter())
    }

    fn contains(&self, who: Holder) -> bool {
        self.iter().any(|h| *h == who)
    }

    fn add(&mut self, who: Holder) {
        *self = match std::mem::take(self) {
            Self::None => Self::One(who),
            Self::One(h) => Self::Many(vec![h, who]),
            Self::Many(mut v) => {
                v.push(who);
                Self::Many(v)
            }
        };
    }

    fn retain(&mut self, keep: impl Fn(&Holder) -> bool) {
        *self = match std::mem::take(self) {
            Self::One(h) if keep(&h) => Self::One(h),
            Self::Many(mut v) => {
                v.retain(|h| keep(h));
                match v.len() {
                    0 => Self::None,
                    1 => Self::One(v[0]),
                    _ => Self::Many(v),
                }
            }
            _ => Self::None,
        };
    }

    fn is_empty(&self) -> bool {
        matches!(self, Self::None)
    }

    fn len(&self) -> usize {
        self.iter().count()
    }
}

/// The locks on one record.
#[derive(Debug, Default)]
struct Entry {
    update: Option<Holder>,
    readers: Readers,
}

impl Entry {
    fn is_empty(&self) -> bool {
        self.update.is_none() && self.readers.is_empty()
    }
}

/// The record lock table.
#[derive(Debug, Default)]
pub struct RecordLocks {
    held: HashMap<(ObjectId, u64), Entry>,
}

/// The table every open data path shares.
pub type SharedLocks = Arc<Mutex<RecordLocks>>;

/// A fresh shared table.
#[must_use]
pub fn new_shared() -> SharedLocks {
    Arc::new(Mutex::new(RecordLocks::default()))
}

impl RecordLocks {
    /// Lock record `rrn` of `member` for update by `who`, or say who has
    /// it. A read lock of `who`'s own job does not stand in the way.
    pub fn lock(&mut self, member: ObjectId, rrn: u64, who: Holder) -> Result<(), Refused> {
        let e = self.held.entry((member, rrn)).or_default();
        match e.update {
            Some(h) if h.job != who.job => return Err(Refused::Job(h.job)),
            Some(h) if h != who => return Err(Refused::SameJob),
            _ => {}
        }
        if let Some(r) = e.readers.iter().find(|h| h.job != who.job) {
            return Err(Refused::Job(r.job));
        }
        e.update = Some(who);
        Ok(())
    }

    /// Lock record `rrn` of `member` for reading by `who`: true when the
    /// lock is new, false when `who` held it already.
    pub fn lock_read(&mut self, member: ObjectId, rrn: u64, who: Holder) -> Result<bool, Refused> {
        let e = self.held.entry((member, rrn)).or_default();
        if let Some(h) = e.update
            && h.job != who.job
        {
            return Err(Refused::Job(h.job));
        }
        if e.readers.contains(who) {
            return Ok(false);
        }
        e.readers.add(who);
        Ok(true)
    }

    /// Release an update lock `who` holds; false when it did not.
    pub fn unlock(&mut self, member: ObjectId, rrn: u64, who: Holder) -> bool {
        let Some(e) = self.held.get_mut(&(member, rrn)) else {
            return false;
        };
        if e.update != Some(who) {
            return false;
        }
        e.update = None;
        if e.is_empty() {
            self.held.remove(&(member, rrn));
        }
        true
    }

    /// Release a read lock `who` holds; false when it did not.
    pub fn unlock_read(&mut self, member: ObjectId, rrn: u64, who: Holder) -> bool {
        let Some(e) = self.held.get_mut(&(member, rrn)) else {
            return false;
        };
        if !e.readers.contains(who) {
            return false;
        }
        e.readers.retain(|h| *h != who);
        if e.is_empty() {
            self.held.remove(&(member, rrn));
        }
        true
    }

    /// Hand an update lock `from` holds to `to` (a commit definition
    /// taking a path's lock); false when `from` did not hold it.
    pub fn hand_over(&mut self, member: ObjectId, rrn: u64, from: Holder, to: Holder) -> bool {
        match self.held.get_mut(&(member, rrn)) {
            Some(e) if e.update == Some(from) => {
                e.update = Some(to);
                true
            }
            _ => false,
        }
    }

    /// Which job holds a record's update lock.
    #[must_use]
    pub fn holder(&self, member: ObjectId, rrn: u64) -> Option<JobHandle> {
        self.held.get(&(member, rrn)).and_then(|e| e.update).map(|h| h.job)
    }

    /// The jobs holding a record's read locks.
    #[must_use]
    pub fn readers(&self, member: ObjectId, rrn: u64) -> Vec<JobHandle> {
        self.held
            .get(&(member, rrn))
            .map(|e| e.readers.iter().map(|h| h.job).collect())
            .unwrap_or_default()
    }

    /// Release everything a job holds.
    pub fn release_job(&mut self, job: JobHandle) {
        self.held.retain(|_, e| {
            if e.update.is_some_and(|h| h.job == job) {
                e.update = None;
            }
            e.readers.retain(|h| h.job != job);
            !e.is_empty()
        });
    }

    /// Locks held, update and read.
    #[must_use]
    pub fn len(&self) -> usize {
        self.held
            .values()
            .map(|e| usize::from(e.update.is_some()) + e.readers.len())
            .sum()
    }

    /// Whether nothing is locked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: ObjectId = ObjectId {
        asp: 1,
        type_code: 1,
        serial: 7,
        generation: 0,
    };

    fn h(job: u64, owner: u64) -> Holder {
        Holder::new(JobHandle(job), owner)
    }

    #[test]
    fn readers_share_and_hold_off_other_jobs_updates() {
        let mut t = RecordLocks::default();
        assert_eq!(t.lock_read(M, 1, h(1, 10)), Ok(true));
        assert_eq!(t.lock_read(M, 1, h(1, 10)), Ok(false));
        assert_eq!(t.lock_read(M, 1, h(2, 20)), Ok(true));
        assert_eq!(t.readers(M, 1).len(), 2);
        assert_eq!(t.lock(M, 1, h(3, 30)), Err(Refused::Job(JobHandle(1))));
        // The reader's own update waits for the other reader.
        assert_eq!(t.lock(M, 1, h(1, 10)), Err(Refused::Job(JobHandle(2))));
        assert!(t.unlock_read(M, 1, h(2, 20)));
        // Alone, it takes the update lock; others may no longer read.
        assert_eq!(t.lock(M, 1, h(1, 10)), Ok(()));
        assert_eq!(t.lock_read(M, 1, h(2, 20)), Err(Refused::Job(JobHandle(1))));
        assert_eq!(t.len(), 2);
        t.release_job(JobHandle(1));
        assert!(t.is_empty());
    }

    #[test]
    fn an_update_lock_holds_off_other_jobs_readers() {
        let mut t = RecordLocks::default();
        t.lock(M, 2, h(1, 10)).unwrap();
        assert_eq!(t.lock_read(M, 2, h(2, 20)), Err(Refused::Job(JobHandle(1))));
        // Its own job reads, from any path.
        assert_eq!(t.lock_read(M, 2, h(1, 11)), Ok(true));
        assert_eq!(t.lock(M, 2, h(1, 11)), Err(Refused::SameJob));
        assert!(t.unlock(M, 2, h(1, 10)));
        assert!(t.unlock_read(M, 2, h(1, 11)));
        assert!(t.is_empty());
        // A refused update leaves nothing behind.
        t.lock_read(M, 3, h(1, 10)).unwrap();
        assert!(t.lock(M, 4, h(2, 20)).is_ok());
        assert!(t.unlock(M, 4, h(2, 20)));
        assert_eq!(t.len(), 1);
    }
}

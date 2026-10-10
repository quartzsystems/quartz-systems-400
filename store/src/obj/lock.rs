// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The in-process object lock table: the seed of WRKOBJLCK.
//! `MIGRATE(phase5)`: job handles come from real jobs.

use std::collections::HashMap;

use crate::obj::ObjectId;

/// Who holds a lock. An opaque ID the caller supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobHandle(pub u64);

/// The five lock states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LockState {
    /// `*SHRRD`.
    SharedRead,
    /// `*SHRNUP`.
    SharedNoUpdate,
    /// `*SHRUPD`.
    SharedUpdate,
    /// `*EXCLRD`.
    ExclusiveRead,
    /// `*EXCL`.
    Exclusive,
}

impl LockState {
    /// The `*NAME`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SharedRead => "*SHRRD",
            Self::SharedNoUpdate => "*SHRNUP",
            Self::SharedUpdate => "*SHRUPD",
            Self::ExclusiveRead => "*EXCLRD",
            Self::Exclusive => "*EXCL",
        }
    }

    /// Whether two locks held by different jobs may coexist: the
    /// standard matrix. `*SHRRD` goes with anything but `*EXCL`;
    /// `*SHRNUP` and `*SHRUPD` each go with themselves; `*EXCLRD` and
    /// `*EXCL` go with nothing else.
    #[must_use]
    pub fn compatible(self, other: Self) -> bool {
        use LockState::{ExclusiveRead, SharedNoUpdate, SharedRead, SharedUpdate};
        matches!(
            (self, other),
            (
                SharedRead,
                SharedRead | SharedNoUpdate | SharedUpdate | ExclusiveRead
            ) | (SharedNoUpdate | SharedUpdate | ExclusiveRead, SharedRead)
                | (SharedNoUpdate, SharedNoUpdate)
                | (SharedUpdate, SharedUpdate)
        )
    }
}

/// A lock held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    /// Holder.
    pub job: JobHandle,
    /// State.
    pub state: LockState,
}

/// Locks by object.
#[derive(Debug, Default)]
pub struct LockTable {
    held: HashMap<ObjectId, Vec<Held>>,
}

impl LockTable {
    /// Take `state` on `id` for `job`. Fails with the conflicting holder
    /// when another job holds an incompatible lock.
    pub fn lock(&mut self, job: JobHandle, id: ObjectId, state: LockState) -> Result<(), Held> {
        let holders = self.held.entry(id).or_default();
        if let Some(h) = holders
            .iter()
            .find(|h| h.job != job && !h.state.compatible(state))
        {
            return Err(*h);
        }
        holders.push(Held { job, state });
        Ok(())
    }

    /// Release one lock of `state` on `id` held by `job`. Returns
    /// whether there was one.
    pub fn unlock(&mut self, job: JobHandle, id: ObjectId, state: LockState) -> bool {
        let Some(holders) = self.held.get_mut(&id) else {
            return false;
        };
        let Some(i) = holders
            .iter()
            .position(|h| h.job == job && h.state == state)
        else {
            return false;
        };
        holders.swap_remove(i);
        if holders.is_empty() {
            self.held.remove(&id);
        }
        true
    }

    /// Release every lock `job` holds.
    pub fn release_job(&mut self, job: JobHandle) {
        self.held.retain(|_, holders| {
            holders.retain(|h| h.job != job);
            !holders.is_empty()
        });
    }

    /// Release every lock on `id` (the object is gone).
    pub fn release_object(&mut self, id: ObjectId) {
        self.held.remove(&id);
    }

    /// Locks on `id`.
    #[must_use]
    pub fn holders(&self, id: ObjectId) -> Vec<Held> {
        self.held.get(&id).cloned().unwrap_or_default()
    }

    /// Objects with any lock.
    #[must_use]
    pub fn locked_objects(&self) -> Vec<ObjectId> {
        let mut v: Vec<ObjectId> = self.held.keys().copied().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix() {
        use LockState::{Exclusive, ExclusiveRead, SharedNoUpdate, SharedRead, SharedUpdate};
        assert!(SharedRead.compatible(ExclusiveRead));
        assert!(!SharedRead.compatible(Exclusive));
        assert!(SharedNoUpdate.compatible(SharedNoUpdate));
        assert!(!SharedNoUpdate.compatible(SharedUpdate));
        assert!(SharedUpdate.compatible(SharedUpdate));
        assert!(!SharedUpdate.compatible(ExclusiveRead));
        assert!(!ExclusiveRead.compatible(ExclusiveRead));
        assert!(!Exclusive.compatible(SharedRead));
    }

    #[test]
    fn table() {
        let id = ObjectId {
            asp: 1,
            type_code: 2,
            serial: 3,
            generation: 0,
        };
        let (a, b) = (JobHandle(1), JobHandle(2));
        let mut t = LockTable::default();
        t.lock(a, id, LockState::SharedUpdate).unwrap();
        t.lock(b, id, LockState::SharedUpdate).unwrap();
        let e = t.lock(b, id, LockState::Exclusive).unwrap_err();
        assert_eq!(e.job, a);
        // Same job never conflicts with itself.
        t.lock(a, id, LockState::ExclusiveRead).unwrap_err();
        t.release_job(b);
        t.lock(a, id, LockState::Exclusive).unwrap();
        assert_eq!(t.holders(id).len(), 2);
        assert!(t.unlock(a, id, LockState::Exclusive));
        assert!(!t.unlock(a, id, LockState::Exclusive));
        assert_eq!(t.locked_objects(), vec![id]);
        t.release_object(id);
        assert!(t.locked_objects().is_empty());
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte-range lock requests that wait, without anyone waiting in the
//! service: [`LockTable::enqueue`] grants a request at once or queues
//! it and answers a token; whenever a range is released the queue of
//! that object is tried oldest first ([`LockTable::grant`]), and each
//! grant is kept for [`LockTable::drain_granted`], which the caller
//! tells the requester through its poll. A request is not granted past
//! an older one it conflicts with, so the queue is fair. A request that
//! would close a cycle of waits (among queued requests and `F_SETLKW`
//! retries) is `EDEADLK`. Sub-owners ([`super::range::RangeLock::sub`])
//! release together ([`LockTable::release_sub`]).

use std::collections::HashSet;

use super::range::{self, RangeKind, RangeLock};
use super::{Holder, LockTable, ObjKey, Owner};
use crate::errno::{Errno, Result};

/// A queued request.
#[derive(Debug, Clone)]
pub struct Waiter {
    /// Its token.
    pub token: u64,
    /// The object.
    pub key: ObjKey,
    /// The object's path, for the listing once granted.
    pub path: String,
    /// The lock asked for.
    pub want: RangeLock,
}

/// How many holders a wait-for search visits at most.
const SEARCH_LIMIT: usize = 4096;

impl LockTable {
    /// The holders that block `want` on `key` now.
    fn blockers(&self, key: ObjKey, want: &RangeLock) -> Vec<Holder> {
        self.objs.get(&key).map_or_else(Vec::new, |o| {
            o.ranges
                .iter()
                .filter(|l| range::blocks(l, want))
                .map(|l| (l.owner, l.sub))
                .collect()
        })
    }

    /// Whether `from` waits, directly or through others, on `target`.
    pub(super) fn waits_on(&self, from: Holder, target: Holder) -> bool {
        let mut seen = HashSet::new();
        let mut todo = vec![from];
        while let Some(h) = todo.pop() {
            if h == target {
                return true;
            }
            if !seen.insert(h) || seen.len() > SEARCH_LIMIT {
                continue;
            }
            if let Some(&next) = self.waits.get(&h) {
                todo.push(next);
            }
            for w in self
                .queue
                .iter()
                .filter(|w| (w.want.owner, w.want.sub) == h)
            {
                todo.extend(self.blockers(w.key, &w.want));
            }
        }
        false
    }

    /// Set `want` (a read or write lock) on `key` now, or queue it:
    /// `None` when granted, else the request's token. `EDEADLK` when
    /// waiting would never end.
    ///
    /// # Errors
    ///
    /// `EINVAL` for an unlock, `EDEADLK` as above.
    pub fn enqueue(&mut self, key: ObjKey, path: &str, want: RangeLock) -> Result<Option<u64>> {
        if want.kind == RangeKind::Unlock {
            return Err(Errno::EINVAL);
        }
        let me = (want.owner, want.sub);
        let blockers = self.blockers(key, &want);
        let older = self
            .queue
            .iter()
            .any(|w| w.key == key && conflicts(&w.want, &want));
        if blockers.is_empty() && !older {
            self.set(key, path, want, false)?;
            return Ok(None);
        }
        if blockers.iter().any(|b| self.waits_on(*b, me)) {
            return Err(Errno::EDEADLK);
        }
        self.next_token += 1;
        let token = self.next_token;
        self.queue.push(Waiter {
            token,
            key,
            path: path.to_string(),
            want,
        });
        Ok(Some(token))
    }

    /// Take back the queued request `token` of `owner`: whether it was
    /// still waiting (once granted the lock is held and must be unlocked).
    pub fn cancel(&mut self, owner: Owner, token: u64) -> bool {
        let Some(i) = self
            .queue
            .iter()
            .position(|w| w.token == token && w.want.owner == owner)
        else {
            return false;
        };
        let w = self.queue.remove(i);
        // Younger requests it held back may go now.
        self.grant(w.key);
        true
    }

    /// Drop every lock and queued request of `owner`'s sub-owner `sub`
    /// (not 0: the job's own locks end with its descriptors): how many
    /// locks went.
    pub fn release_sub(&mut self, owner: Owner, sub: u64) -> usize {
        let mut n = 0;
        let mut freed = Vec::new();
        self.objs.retain(|k, o| {
            let before = o.ranges.len();
            o.ranges.retain(|l| l.owner != owner || l.sub != sub);
            if o.ranges.len() != before {
                n += before - o.ranges.len();
                freed.push(*k);
            }
            !(o.opens.is_empty() && o.ranges.is_empty())
        });
        let me = (owner, sub);
        self.waits.retain(|k, v| *k != me && *v != me);
        self.queue
            .retain(|w| w.want.owner != owner || w.want.sub != sub);
        let mut keys: Vec<ObjKey> = self.queue.iter().map(|w| w.key).collect();
        keys.extend(freed);
        keys.sort();
        keys.dedup();
        for k in keys {
            self.grant(k);
        }
        n
    }

    /// Grant the queued requests on `key` that nothing blocks, oldest
    /// first, none past an older one it conflicts with.
    pub(super) fn grant(&mut self, key: ObjKey) {
        let mut i = 0;
        let mut held_back: Vec<RangeLock> = Vec::new();
        while i < self.queue.len() {
            if self.queue[i].key != key {
                i += 1;
                continue;
            }
            let want = self.queue[i].want;
            let free = self.blockers(key, &want).is_empty()
                && !held_back.iter().any(|h| conflicts(h, &want));
            if !free {
                held_back.push(want);
                i += 1;
                continue;
            }
            let w = self.queue.remove(i);
            let o = self.objs.entry(key).or_default();
            if o.path.is_empty() {
                o.path.clone_from(&w.path);
            }
            range::apply(&mut o.ranges, w.want);
            self.waits.remove(&(w.want.owner, w.want.sub));
            self.granted.push((w.want.owner, w.token));
        }
    }

    /// The queued requests granted since the last call: owner and token.
    pub fn drain_granted(&mut self) -> Vec<(Owner, u64)> {
        std::mem::take(&mut self.granted)
    }

    /// The requests still queued, oldest first.
    #[must_use]
    pub fn queued(&self) -> &[Waiter] {
        &self.queue
    }
}

/// Whether two requests of different holders could not both hold.
fn conflicts(a: &RangeLock, b: &RangeLock) -> bool {
    range::blocks(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: ObjKey = ObjKey { fs: 1, ino: 7 };
    const L: ObjKey = ObjKey { fs: 1, ino: 8 };

    fn w(owner: u64, sub: u64, start: u64, len: u64) -> RangeLock {
        RangeLock::new(Owner(owner), RangeKind::Write, start, len).with_sub(sub)
    }

    #[test]
    fn sub_owners_of_one_job_conflict() {
        let mut t = LockTable::default();
        t.set(K, "/a", w(1, 10, 0, 10), false).unwrap();
        assert_eq!(t.set(K, "/a", w(1, 11, 5, 1), false), Err(Errno::EAGAIN));
        // The job itself (sub 0) is another holder too.
        assert_eq!(t.set(K, "/a", w(1, 0, 5, 1), false), Err(Errno::EAGAIN));
        // A sub-owner's locks outlive the job's descriptors' close.
        t.close(K, 99, Owner(1), true);
        assert_eq!(t.list(K).len(), 1);
        assert_eq!(t.release_sub(Owner(1), 10), 1);
        assert!(t.list(K).is_empty());
    }

    #[test]
    fn queued_requests_are_granted_in_order() {
        let mut t = LockTable::default();
        assert_eq!(t.enqueue(K, "/a", w(1, 1, 0, 10)), Ok(None));
        let a = t.enqueue(K, "/a", w(1, 2, 0, 5)).unwrap().unwrap();
        let b = t.enqueue(K, "/a", w(2, 0, 4, 2)).unwrap().unwrap();
        // A request clear of the holder still waits behind an older one.
        let c = t.enqueue(K, "/a", w(3, 0, 3, 1)).unwrap().unwrap();
        assert_eq!(t.queued().len(), 3);
        t.release_sub(Owner(1), 1);
        assert_eq!(t.drain_granted(), vec![(Owner(1), a)]);
        // Unlocking the first grant frees both younger ones.
        t.set(
            K,
            "/a",
            RangeLock::new(Owner(1), RangeKind::Unlock, 0, 0).with_sub(2),
            false,
        )
        .unwrap();
        assert_eq!(t.drain_granted(), vec![(Owner(2), b), (Owner(3), c)]);
        assert!(t.queued().is_empty());
    }

    #[test]
    fn cancel_and_owner_end() {
        let mut t = LockTable::default();
        t.enqueue(K, "/a", w(1, 0, 0, 0)).unwrap();
        let a = t.enqueue(K, "/a", w(2, 5, 0, 1)).unwrap().unwrap();
        let b = t.enqueue(K, "/a", w(3, 0, 0, 1)).unwrap().unwrap();
        assert!(t.cancel(Owner(2), a));
        assert!(!t.cancel(Owner(2), a));
        assert!(!t.cancel(Owner(9), b), "not the requester's");
        t.release_owner(Owner(1));
        assert_eq!(t.drain_granted(), vec![(Owner(3), b)]);
    }

    #[test]
    fn a_cycle_of_waits_is_a_deadlock() {
        let mut t = LockTable::default();
        t.enqueue(K, "/a", w(1, 1, 0, 1)).unwrap();
        t.enqueue(L, "/b", w(1, 2, 0, 1)).unwrap();
        // (1,2) waits for (1,1) on K; (1,1) asking for L would wait on (1,2).
        assert!(t.enqueue(K, "/a", w(1, 2, 0, 1)).unwrap().is_some());
        assert_eq!(t.enqueue(L, "/b", w(1, 1, 0, 1)), Err(Errno::EDEADLK));
        // A third holder waiting is no cycle.
        assert!(t.enqueue(L, "/b", w(2, 0, 0, 1)).unwrap().is_some());
    }
}

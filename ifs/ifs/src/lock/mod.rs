// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Locks on stream files: the share modes an `open()` asks for
//! ([`share`]), byte-range locks ([`range`]), and the table that holds
//! both per object, lists them for `WRKOBJLCK` and drops an owner's
//! when its job's connection closes. Check-out is persistent and lives
//! in the object's metadata; `ops` enforces it at open.

pub mod range;
pub mod share;

use std::collections::HashMap;

use crate::errno::{Errno, Result};
use range::{RangeKind, RangeLock};
use share::{Access, Share};

/// Who holds a lock: an opaque id the caller gives (the storage service
/// uses one per job connection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Owner(pub u64);

/// An object the locks are on: its file system and file number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjKey {
    /// File system (mount) id.
    pub fs: u32,
    /// File number.
    pub ino: u64,
}

/// One open of an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenRef {
    /// The open file description.
    pub id: u64,
    /// Its owner.
    pub owner: Owner,
    /// What it reads and writes.
    pub access: Access,
    /// What it lets others do.
    pub share: Share,
}

#[derive(Debug, Default)]
struct ObjLocks {
    path: String,
    opens: Vec<OpenRef>,
    ranges: Vec<RangeLock>,
}

/// One lock for `WRKOBJLCK`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockInfo {
    /// The object's path when it was locked.
    pub path: String,
    /// The holder.
    pub owner: Owner,
    /// What is held.
    pub what: Held,
}

/// What a lock holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// An open with its access and share mode.
    Open(Access, Share),
    /// A byte range.
    Range(RangeLock),
}

/// Every object's locks.
#[derive(Debug, Default)]
pub struct LockTable {
    objs: HashMap<ObjKey, ObjLocks>,
    /// Owners waiting on another owner (`F_SETLKW` retries).
    waits: HashMap<Owner, Owner>,
}

impl LockTable {
    /// Register an open, or `EBUSY` when a share mode refuses it.
    pub fn open(&mut self, key: ObjKey, path: &str, r: OpenRef) -> Result<()> {
        if let Some(o) = self.objs.get(&key)
            && o.opens
                .iter()
                .any(|x| share::conflicts(x.access, x.share, r.access, r.share))
        {
            return Err(Errno::EBUSY);
        }
        let o = self.objs.entry(key).or_default();
        o.path = path.to_string();
        o.opens.push(r);
        Ok(())
    }

    /// Drop an open; with `last_of_owner` the owner's byte-range locks
    /// on the object go too (POSIX: any close drops them).
    pub fn close(&mut self, key: ObjKey, id: u64, owner: Owner, drop_ranges: bool) {
        if let Some(o) = self.objs.get_mut(&key) {
            o.opens.retain(|x| x.id != id);
            if drop_ranges {
                o.ranges.retain(|l| l.owner != owner);
            }
            if o.opens.is_empty() && o.ranges.is_empty() {
                self.objs.remove(&key);
            }
        }
    }

    /// Whether any open of the object exists.
    #[must_use]
    pub fn is_open(&self, key: ObjKey) -> bool {
        self.objs.get(&key).is_some_and(|o| !o.opens.is_empty())
    }

    /// Opens on objects of file system `fs`.
    #[must_use]
    pub fn opens_on(&self, fs: u32) -> usize {
        self.objs
            .iter()
            .filter(|(k, _)| k.fs == fs)
            .map(|(_, o)| o.opens.len())
            .sum()
    }

    /// `F_GETLK`: the first lock that would block `want`, if any.
    #[must_use]
    pub fn get(&self, key: ObjKey, want: &RangeLock) -> Option<RangeLock> {
        self.objs
            .get(&key)?
            .ranges
            .iter()
            .find(|l| range::blocks(l, want))
            .copied()
    }

    /// `F_SETLK` (`blocking` false) or one try of `F_SETLKW` (`blocking` true):
    /// set or clear a range. A conflict is `EAGAIN` for `F_SETLK`; for
    /// `F_SETLKW` it is `EAGAIN` too (the caller retries, the service
    /// never waits) unless waiting would deadlock, `EDEADLK`.
    pub fn set(&mut self, key: ObjKey, path: &str, want: RangeLock, blocking: bool) -> Result<()> {
        let o = self.objs.entry(key).or_default();
        if o.path.is_empty() {
            o.path = path.to_string();
        }
        if want.kind != RangeKind::Unlock
            && let Some(holder) = o
                .ranges
                .iter()
                .find(|l| range::blocks(l, &want))
                .map(|l| l.owner)
        {
            if blocking {
                // Does the holder wait, directly or not, on us?
                let mut at = holder;
                let mut hops = 0;
                while let Some(&next) = self.waits.get(&at) {
                    if next == want.owner {
                        return Err(Errno::EDEADLK);
                    }
                    at = next;
                    hops += 1;
                    if hops > 1024 {
                        break;
                    }
                }
                self.waits.insert(want.owner, holder);
            }
            return Err(Errno::EAGAIN);
        }
        self.waits.remove(&want.owner);
        range::apply(&mut o.ranges, want);
        if o.opens.is_empty() && o.ranges.is_empty() {
            self.objs.remove(&key);
        }
        Ok(())
    }

    /// Stop waiting (an owner gave up an `F_SETLKW`).
    pub fn cancel_wait(&mut self, owner: Owner) {
        self.waits.remove(&owner);
    }

    /// Drop everything an owner holds; returns the open ids it had.
    pub fn release_owner(&mut self, owner: Owner) -> Vec<u64> {
        let mut ids = Vec::new();
        self.objs.retain(|_, o| {
            ids.extend(o.opens.iter().filter(|x| x.owner == owner).map(|x| x.id));
            o.opens.retain(|x| x.owner != owner);
            o.ranges.retain(|l| l.owner != owner);
            !(o.opens.is_empty() && o.ranges.is_empty())
        });
        self.waits.remove(&owner);
        self.waits.retain(|_, v| *v != owner);
        ids
    }

    /// The locks on one object.
    #[must_use]
    pub fn list(&self, key: ObjKey) -> Vec<LockInfo> {
        let Some(o) = self.objs.get(&key) else {
            return Vec::new();
        };
        let opens = o.opens.iter().map(|x| LockInfo {
            path: o.path.clone(),
            owner: x.owner,
            what: Held::Open(x.access, x.share),
        });
        let ranges = o.ranges.iter().map(|l| LockInfo {
            path: o.path.clone(),
            owner: l.owner,
            what: Held::Range(*l),
        });
        opens.chain(ranges).collect()
    }

    /// Every lock an owner holds.
    #[must_use]
    pub fn held_by(&self, owner: Owner) -> Vec<(ObjKey, LockInfo)> {
        let mut out = Vec::new();
        for (k, o) in &self.objs {
            for x in o.opens.iter().filter(|x| x.owner == owner) {
                out.push((
                    *k,
                    LockInfo {
                        path: o.path.clone(),
                        owner,
                        what: Held::Open(x.access, x.share),
                    },
                ));
            }
            for l in o.ranges.iter().filter(|l| l.owner == owner) {
                out.push((
                    *k,
                    LockInfo {
                        path: o.path.clone(),
                        owner,
                        what: Held::Range(*l),
                    },
                ));
            }
        }
        out.sort_by_key(|(k, _)| *k);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: ObjKey = ObjKey { fs: 1, ino: 9 };

    fn r(owner: u64, kind: RangeKind, start: u64, len: u64) -> RangeLock {
        RangeLock::new(Owner(owner), kind, start, len)
    }

    #[test]
    fn share_modes_at_open() {
        let mut t = LockTable::default();
        let rd = Access {
            read: true,
            write: false,
        };
        let wr = Access {
            read: false,
            write: true,
        };
        let open = |id, owner, access, share| OpenRef {
            id,
            owner: Owner(owner),
            access,
            share,
        };
        t.open(K, "/a", open(1, 1, rd, Share::ReadOnly)).unwrap();
        assert_eq!(
            t.open(K, "/a", open(2, 2, wr, Share::ReadWrite)),
            Err(Errno::EBUSY)
        );
        t.open(K, "/a", open(3, 2, rd, Share::ReadWrite)).unwrap();
        assert_eq!(t.list(K).len(), 2);
        t.close(K, 1, Owner(1), true);
        t.open(K, "/a", open(4, 3, wr, Share::ReadWrite)).unwrap();
        assert_eq!(t.release_owner(Owner(2)), vec![3]);
        assert_eq!(t.release_owner(Owner(3)), vec![4]);
        assert!(!t.is_open(K));
    }

    #[test]
    fn ranges_and_deadlock() {
        let mut t = LockTable::default();
        t.set(K, "/a", r(1, RangeKind::Write, 0, 10), false)
            .unwrap();
        t.set(
            ObjKey { fs: 1, ino: 10 },
            "/b",
            r(2, RangeKind::Write, 0, 10),
            false,
        )
        .unwrap();
        assert_eq!(
            t.set(K, "/a", r(2, RangeKind::Read, 5, 1), false),
            Err(Errno::EAGAIN)
        );
        assert_eq!(
            t.get(K, &r(2, RangeKind::Read, 5, 1)).unwrap().owner,
            Owner(1)
        );
        assert_eq!(t.get(K, &r(1, RangeKind::Write, 5, 1)), None);
        // 2 waits on 1; 1 then waits on 2: deadlock.
        assert_eq!(
            t.set(K, "/a", r(2, RangeKind::Write, 0, 1), true),
            Err(Errno::EAGAIN)
        );
        assert_eq!(
            t.set(
                ObjKey { fs: 1, ino: 10 },
                "/b",
                r(1, RangeKind::Write, 0, 1),
                true
            ),
            Err(Errno::EDEADLK)
        );
        t.release_owner(Owner(1));
        t.set(K, "/a", r(2, RangeKind::Write, 0, 1), true).unwrap();
        assert_eq!(t.held_by(Owner(2)).len(), 2);
    }
}

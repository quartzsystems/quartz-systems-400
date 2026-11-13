// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The descriptors a daemon thread keeps open, so `READ` and `WRITE`
//! do not open the file on every call: keyed by (file ID, caller,
//! read or write), at most a number of them, each closed after a time
//! unused. The caller's authority was checked when it was opened; the
//! protocol checks it again on each call through `access()`. Entries
//! of an object go when it is removed, renamed over or found stale, or
//! when an open with a share reservation needs them gone.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ifs::client::Caller;
use ifs::ops::audit::FileId;

/// What a cached descriptor was opened for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// `O_RDONLY`.
    Read,
    /// `O_WRONLY`.
    Write,
}

type Key = (FileId, Caller, Mode);

/// The cache.
#[derive(Debug)]
pub struct FdCache {
    map: HashMap<Key, (i32, Instant)>,
    cap: usize,
    idle: Duration,
}

impl FdCache {
    /// At most `cap` descriptors, each kept `idle` unused.
    #[must_use]
    pub fn new(cap: usize, idle: Duration) -> Self {
        Self {
            map: HashMap::new(),
            cap: cap.max(1),
            idle,
        }
    }

    /// The descriptor for the key, marked used at `now`.
    pub fn get(&mut self, fid: FileId, who: &Caller, mode: Mode, now: Instant) -> Option<i32> {
        let e = self.map.get_mut(&(fid, who.clone(), mode))?;
        e.1 = now;
        Some(e.0)
    }

    /// Keeps `fd`: the descriptors to close to make room.
    pub fn put(
        &mut self,
        fid: FileId,
        who: &Caller,
        mode: Mode,
        fd: i32,
        now: Instant,
    ) -> Vec<i32> {
        let mut out = Vec::new();
        while self.map.len() >= self.cap {
            let Some(k) = self
                .map
                .iter()
                .min_by_key(|(_, (_, t))| *t)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some((fd, _)) = self.map.remove(&k) {
                out.push(fd);
            }
        }
        if let Some((old, _)) = self.map.insert((fid, who.clone(), mode), (fd, now)) {
            out.push(old);
        }
        out
    }

    /// Forgets every descriptor of `fid`: those to close.
    pub fn drop_object(&mut self, fid: FileId) -> Vec<i32> {
        let keys: Vec<Key> = self.map.keys().filter(|k| k.0 == fid).cloned().collect();
        keys.into_iter()
            .filter_map(|k| self.map.remove(&k).map(|e| e.0))
            .collect()
    }

    /// Forgets one descriptor (it failed): whether it was kept.
    pub fn forget(&mut self, fd: i32) -> bool {
        let before = self.map.len();
        self.map.retain(|_, e| e.0 != fd);
        self.map.len() != before
    }

    /// Whether `fd` is kept here.
    #[must_use]
    pub fn holds(&self, fd: i32) -> bool {
        self.map.values().any(|e| e.0 == fd)
    }

    /// Those unused since before `now - idle`: to close.
    pub fn expire(&mut self, now: Instant) -> Vec<i32> {
        let idle = self.idle;
        let old: Vec<Key> = self
            .map
            .iter()
            .filter(|(_, (_, t))| now.saturating_duration_since(*t) >= idle)
            .map(|(k, _)| k.clone())
            .collect();
        old.into_iter()
            .filter_map(|k| self.map.remove(&k).map(|e| e.0))
            .collect()
    }

    /// Every descriptor: to close.
    pub fn drain(&mut self) -> Vec<i32> {
        self.map.drain().map(|(_, e)| e.0).collect()
    }

    /// How many are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fid(ino: u64) -> FileId {
        FileId {
            fs: 1,
            ino,
            generation: 1,
        }
    }

    #[test]
    fn bounded_idle_and_dropped_by_object() {
        let t = Instant::now();
        let a = Caller::Uid { uid: 1 };
        let mut c = FdCache::new(2, Duration::from_secs(5));
        assert!(c.put(fid(1), &a, Mode::Read, 10, t).is_empty());
        assert!(
            c.put(fid(1), &a, Mode::Write, 11, t + Duration::from_secs(1))
                .is_empty()
        );
        assert_eq!(
            c.get(fid(1), &a, Mode::Read, t + Duration::from_secs(2)),
            Some(10)
        );
        // Full: the least recently used goes.
        assert_eq!(
            c.put(fid(2), &a, Mode::Read, 12, t + Duration::from_secs(3)),
            vec![11]
        );
        assert_eq!(c.get(fid(1), &Caller::Uid { uid: 2 }, Mode::Read, t), None);
        assert_eq!(c.drop_object(fid(1)), vec![10]);
        assert_eq!(c.expire(t + Duration::from_secs(9)), vec![12]);
        assert!(c.is_empty());
    }
}

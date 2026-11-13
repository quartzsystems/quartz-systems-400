// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The duplicate request cache (docs/NFS.md section 3): the reply to a
//! call that is not safe to run twice is kept, keyed on (client
//! address, port, XID, program, version, procedure), for a time and up
//! to a number of entries. A retransmission gets the kept reply; one
//! that arrives while the first is still running gets nothing (the
//! client retransmits again). The caller says which procedures are
//! kept.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// How long a reply is kept by default.
pub const TTL: Duration = Duration::from_secs(60);

/// How many replies are kept by default.
pub const ENTRIES: usize = 4096;

/// NFS version 3's procedures that change something and are not safe
/// to run twice: SETATTR, CREATE, MKDIR, SYMLINK, MKNOD, REMOVE, RMDIR,
/// RENAME, LINK.
pub const NFS3_NONIDEMPOTENT: &[u32] = &[2, 8, 9, 10, 11, 12, 13, 14, 15];

/// What identifies a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// The client's address and port.
    pub peer: SocketAddr,
    /// The transaction ID.
    pub xid: u32,
    /// The program.
    pub prog: u32,
    /// The version.
    pub vers: u32,
    /// The procedure.
    pub procedure: u32,
}

/// What the cache knows of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Not cached: run it (and finish it).
    New,
    /// Running now: drop the retransmission.
    InProgress,
    /// Done: send this reply again.
    Done(Arc<Vec<u8>>),
    /// Not a procedure the cache keeps.
    NotCached,
}

/// Which (program, version, procedure) are kept.
pub type Policy = Box<dyn Fn(u32, u32, u32) -> bool + Send + Sync>;

/// A call: its sequence number and, once done, its reply.
type Held = (u64, Option<Arc<Vec<u8>>>);

#[derive(Debug, Default)]
struct State {
    map: HashMap<Key, Held>,
    order: VecDeque<(Key, Instant, u64)>,
    seq: u64,
}

/// The cache.
pub struct DuplicateCache {
    state: Mutex<State>,
    ttl: Duration,
    max: usize,
    policy: Policy,
}

impl std::fmt::Debug for DuplicateCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuplicateCache")
            .field("ttl", &self.ttl)
            .field("max", &self.max)
            .finish_non_exhaustive()
    }
}

impl DuplicateCache {
    /// A cache keeping replies for `ttl`, at most `max` of them, of the
    /// procedures `policy` names.
    pub fn new(
        ttl: Duration,
        max: usize,
        policy: impl Fn(u32, u32, u32) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            state: Mutex::new(State::default()),
            ttl,
            max: max.max(1),
            policy: Box::new(policy),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the cache keeps this procedure.
    pub fn keeps(&self, prog: u32, vers: u32, procedure: u32) -> bool {
        (self.policy)(prog, vers, procedure)
    }

    /// Looks a call up at `now`; a new one is marked running.
    pub fn begin(&self, key: Key, now: Instant) -> Lookup {
        if !self.keeps(key.prog, key.vers, key.procedure) {
            return Lookup::NotCached;
        }
        let mut s = self.state();
        self.expire(&mut s, now);
        if let Some((_, reply)) = s.map.get(&key) {
            return reply.clone().map_or(Lookup::InProgress, Lookup::Done);
        }
        while s.map.len() >= self.max {
            match s.order.pop_front() {
                Some((k, _, n)) => {
                    if s.map.get(&k).is_some_and(|(m, _)| *m == n) {
                        s.map.remove(&k);
                    }
                }
                None => break,
            }
        }
        s.seq += 1;
        let n = s.seq;
        s.map.insert(key, (n, None));
        s.order.push_back((key, now, n));
        Lookup::New
    }

    /// Keeps the reply of a call [`begin`](Self::begin) answered `New`;
    /// `None` (no reply was sent) forgets it.
    pub fn finish(&self, key: Key, reply: Option<&[u8]>) {
        let mut s = self.state();
        match reply {
            Some(r) => {
                if let Some(e) = s.map.get_mut(&key) {
                    e.1 = Some(Arc::new(r.to_vec()));
                }
            }
            None => {
                s.map.remove(&key);
            }
        }
    }

    /// How many calls are held.
    pub fn len(&self) -> usize {
        self.state().map.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn expire(&self, s: &mut State, now: Instant) {
        while let Some((k, at, n)) = s.order.front().copied() {
            if now.saturating_duration_since(at) < self.ttl {
                break;
            }
            s.order.pop_front();
            if s.map.get(&k).is_some_and(|(m, _)| *m == n) {
                s.map.remove(&k);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(xid: u32, procedure: u32) -> Key {
        Key {
            peer: "10.0.0.1:800".parse().unwrap(),
            xid,
            prog: 100_003,
            vers: 3,
            procedure,
        }
    }

    fn cache(max: usize) -> DuplicateCache {
        DuplicateCache::new(TTL, max, |p, v, n| {
            p == 100_003 && v == 3 && NFS3_NONIDEMPOTENT.contains(&n)
        })
    }

    #[test]
    fn a_retransmission_gets_the_kept_reply() {
        let c = cache(8);
        let t = Instant::now();
        assert_eq!(c.begin(key(1, 12), t), Lookup::New);
        assert_eq!(c.begin(key(1, 12), t), Lookup::InProgress);
        c.finish(key(1, 12), Some(b"reply"));
        assert_eq!(
            c.begin(key(1, 12), t),
            Lookup::Done(Arc::new(b"reply".to_vec()))
        );
        // Another XID, port or procedure is another call.
        assert_eq!(c.begin(key(2, 12), t), Lookup::New);
        assert_eq!(c.begin(key(1, 1), t), Lookup::NotCached);
    }

    #[test]
    fn entries_expire_and_are_bounded() {
        let c = cache(2);
        let t = Instant::now();
        for x in 0..5 {
            assert_eq!(c.begin(key(x, 8), t), Lookup::New);
            c.finish(key(x, 8), Some(b"r"));
        }
        assert_eq!(c.len(), 2);
        assert_eq!(c.begin(key(0, 8), t), Lookup::New);
        let later = t + TTL + Duration::from_secs(1);
        assert_eq!(c.begin(key(9, 8), later), Lookup::New);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn no_reply_is_forgotten() {
        let c = cache(4);
        let t = Instant::now();
        assert_eq!(c.begin(key(1, 8), t), Lookup::New);
        c.finish(key(1, 8), None);
        assert!(c.is_empty());
    }
}

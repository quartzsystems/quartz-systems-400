// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's security session: its principal and the decisions already
//! made. An entry is keyed by object and remembers the object's
//! authority generation and the system's security epoch; a change to
//! the object bumps the first, a change to any profile or list bumps the
//! second, and either makes the entry stale. Every check runs in the
//! storage service, one process, so the epoch is a counter in memory and
//! needs no broadcast. Adoption is never cached: only what the user and
//! its groups have.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use store::obj::ObjectId;

use crate::auth::principal::Principal;
use crate::auth::search::Found;

static EPOCH: AtomicU64 = AtomicU64::new(1);

/// A profile, group membership or authorization list changed: every
/// cached decision is stale.
pub fn bump_epoch() {
    EPOCH.fetch_add(1, Ordering::Relaxed);
}

/// The current epoch.
#[must_use]
pub fn epoch() -> u64 {
    EPOCH.load(Ordering::Relaxed)
}

/// Most cached decisions per job.
const LIMIT: usize = 4096;

/// Hits and misses, for `PHASE6.md`'s numbers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Answered from the cache.
    pub hits: u64,
    /// Searched.
    pub misses: u64,
}

/// A job's principal and cache.
#[derive(Debug, Default)]
pub struct Session {
    /// Who.
    pub principal: Principal,
    /// The epoch the principal was loaded in.
    pub loaded: u64,
    cache: HashMap<ObjectId, (u64, u64, Found)>,
    /// Counters.
    pub stats: Stats,
}

impl Session {
    /// A session for `principal`.
    #[must_use]
    pub fn new(principal: Principal) -> Self {
        Self {
            principal,
            loaded: epoch(),
            cache: HashMap::new(),
            stats: Stats::default(),
        }
    }

    /// A cached answer for `id` at generation `gen`.
    pub fn get(&mut self, id: ObjectId, generation: u64) -> Option<Found> {
        let now = epoch();
        match self.cache.get(&id) {
            Some((g, e, f)) if *g == generation && *e == now => {
                self.stats.hits += 1;
                Some(*f)
            }
            _ => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Remember an answer.
    pub fn put(&mut self, id: ObjectId, generation: u64, f: Found) {
        if self.cache.len() >= LIMIT {
            self.cache.clear();
        }
        self.cache.insert(id, (generation, epoch(), f));
    }

    /// Whether the principal predates a profile change.
    #[must_use]
    pub fn stale(&self) -> bool {
        self.loaded != epoch()
    }

    /// Forget every answer.
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

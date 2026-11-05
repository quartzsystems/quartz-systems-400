// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The plan cache: the statements prepared on the system, by text and
//! naming, each parsed once while it stays. A statement prepared again
//! takes its parsed form from here. The cache is bounded by its size
//! (`QSYS2.CHANGE_PLAN_CACHE_SIZE`, 512 MiB to start) and by entries;
//! the entry used least recently leaves first. `QSYS2.DUMP_PLAN_CACHE`
//! writes the entries to a database monitor file, `CLEAR_PLAN_CACHE`
//! empties it.
//!
//! What is kept is the statement, not its access plan: the planner
//! chooses paths and join methods again each time it runs, against the
//! files and statistics as they are then (IBM's plans are revalidated
//! the same way, and rebuilt when they no longer fit). The library list
//! and current schema a run resolves names with are the run's, so they
//! are not part of the key.
//!
//! It lives in the storage service's memory, shared by every job.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::ast::{Naming, Statement};

/// Entries the cache holds at most, whatever their size.
const MAX_ENTRIES: usize = 10_000;

/// The size to start with, in MiB (IBM's default).
pub const DEFAULT_MB: u64 = 512;

/// One statement kept.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The statement's text.
    pub text: String,
    /// The naming it was parsed with.
    pub naming: Naming,
    /// The parsed statement.
    pub statement: Statement,
    /// Its number, in the order entries were made.
    pub id: u64,
    /// Times it was prepared, the first included.
    pub uses: u64,
    /// When it was first and last prepared (Unix seconds).
    pub first: u64,
    /// See `first`.
    pub last: u64,
    /// The use clock's value when last used.
    tick: u64,
}

impl Entry {
    /// Bytes it is reckoned to take.
    fn bytes(&self) -> u64 {
        u64::try_from(self.text.len()).unwrap_or(u64::MAX).saturating_mul(16).saturating_add(512)
    }
}

/// The cache.
struct Cache {
    entries: HashMap<(String, bool), Entry>,
    bytes: u64,
    limit_mb: u64,
    tick: u64,
    next_id: u64,
}

impl Default for Cache {
    fn default() -> Self {
        Self { entries: HashMap::new(), bytes: 0, limit_mb: DEFAULT_MB, tick: 0, next_id: 0 }
    }
}

impl Cache {
    fn limit(&self) -> u64 {
        self.limit_mb.saturating_mul(1 << 20)
    }

    /// Room for `need` more bytes: the least recently used leave.
    fn make_room(&mut self, need: u64) {
        while !self.entries.is_empty() && (self.bytes.saturating_add(need) > self.limit() || self.entries.len() >= MAX_ENTRIES) {
            let oldest = self.entries.iter().min_by_key(|(_, e)| e.tick).map(|(k, _)| k.clone()).expect("not empty");
            if let Some(e) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(e.bytes());
            }
        }
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn key(text: &str, naming: Naming) -> (String, bool) {
    (text.to_string(), naming == Naming::Sys)
}

/// The statement prepared from `text` before, when it is still kept.
#[must_use]
pub fn lookup(text: &str, naming: Naming, now: u64) -> Option<Statement> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.tick += 1;
        let tick = c.tick;
        c.entries.get_mut(&key(text, naming)).map(|e| {
            e.uses += 1;
            e.last = now;
            e.tick = tick;
            e.statement.clone()
        })
    })
}

/// Keep `statement`, prepared from `text`.
pub fn keep(text: &str, naming: Naming, statement: &Statement, now: u64) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.tick += 1;
        c.next_id += 1;
        let e = Entry { text: text.to_string(), naming, statement: statement.clone(), id: c.next_id, uses: 1, first: now, last: now, tick: c.tick };
        let need = e.bytes();
        if need > c.limit() {
            return;
        }
        c.make_room(need);
        c.bytes += need;
        if let Some(old) = c.entries.insert(key(text, naming), e) {
            c.bytes = c.bytes.saturating_sub(old.bytes());
        }
    });
}

/// Every entry, oldest first.
#[must_use]
pub fn entries() -> Vec<Entry> {
    CACHE.with(|c| {
        let mut v: Vec<Entry> = c.borrow().entries.values().cloned().collect();
        v.sort_by_key(|e| e.id);
        v
    })
}

/// Empty the cache.
pub fn clear() {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.entries.clear();
        c.bytes = 0;
    });
}

/// A new size in MiB; entries past it leave.
pub fn resize(mb: u64) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.limit_mb = mb;
        c.make_room(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_by_text_and_naming() {
        clear();
        resize(DEFAULT_MB);
        let st = crate::parse::parse("VALUES 1", Naming::Sql).unwrap();
        keep("VALUES 1", Naming::Sql, &st, 1);
        keep("VALUES 2", Naming::Sql, &st, 2);
        assert!(lookup("VALUES 1", Naming::Sql, 3).is_some());
        assert!(lookup("VALUES 1", Naming::Sys, 3).is_none());
        assert_eq!(entries()[0].uses, 2);
        // No room: every entry leaves.
        resize(0);
        assert!(entries().is_empty());
        resize(DEFAULT_MB);
        keep("VALUES 3", Naming::Sql, &st, 4);
        clear();
        assert!(entries().is_empty());
    }
}

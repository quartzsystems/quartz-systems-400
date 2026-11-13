// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profiles by name, UID and GID, built once and kept until a
//! profile changes ([`IdCache`]), so mapping a UID costs a lookup, not a
//! read of every profile. UID 0 is `QSECOFR`'s alone: elsewhere 0 is a
//! profile with no UID yet. When two profiles claim one number (only
//! possible on a system from before numbers were checked) the first by
//! name keeps it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// One profile as the environment needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The name.
    pub name: String,
    /// `UID`.
    pub uid: u32,
    /// `GID` (0: none).
    pub gid: u32,
    /// Its group profiles.
    pub groups: Vec<String>,
}

/// The profiles, indexed.
#[derive(Debug, Default)]
pub struct IdIndex {
    rows: Vec<Row>,
    by_name: HashMap<String, usize>,
    by_uid: HashMap<u32, usize>,
    by_gid: HashMap<u32, usize>,
    groups: std::collections::HashSet<String>,
}

impl IdIndex {
    /// Index `rows`.
    #[must_use]
    pub fn new(mut rows: Vec<Row>) -> Self {
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        let mut x = Self::default();
        for (i, r) in rows.iter().enumerate() {
            x.by_name.insert(r.name.clone(), i);
            if r.uid != 0 || r.name == "QSECOFR" {
                x.by_uid.entry(r.uid).or_insert(i);
            }
            if r.gid != 0 {
                x.by_gid.entry(r.gid).or_insert(i);
                x.groups.insert(r.name.clone());
            }
            x.groups.extend(r.groups.iter().cloned());
        }
        x.rows = rows;
        x
    }

    /// The profile `name`.
    #[must_use]
    pub fn by_name(&self, name: &str) -> Option<&Row> {
        self.by_name.get(name).map(|&i| &self.rows[i])
    }

    /// The profile with this UID.
    #[must_use]
    pub fn by_uid(&self, uid: u32) -> Option<&Row> {
        self.by_uid.get(&uid).map(|&i| &self.rows[i])
    }

    /// The group profile with this GID (none for 0).
    #[must_use]
    pub fn by_gid(&self, gid: u32) -> Option<&Row> {
        self.by_gid.get(&gid).map(|&i| &self.rows[i])
    }

    /// Whether `name` is a group profile: it has a GID or a member.
    #[must_use]
    pub fn is_group(&self, name: &str) -> bool {
        self.groups.contains(name)
    }
}

/// What an index's validity is judged by.
pub type Stamp = (u64, u64);

/// An index kept across requests, valid while its stamp is the current
/// one (the caller's: the service's security epoch and profile writes).
#[derive(Debug, Default)]
pub struct IdCache {
    slot: Mutex<Option<(Stamp, Arc<IdIndex>)>>,
    builds: std::sync::atomic::AtomicU64,
}

impl IdCache {
    /// The index at `stamp`, built by `load` when the one kept is older.
    pub fn get(&self, stamp: (u64, u64), load: impl FnOnce() -> Vec<Row>) -> Arc<IdIndex> {
        let mut g = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((s, x)) = &*g
            && *s == stamp
        {
            return Arc::clone(x);
        }
        let x = Arc::new(IdIndex::new(load()));
        self.builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *g = Some((stamp, Arc::clone(&x)));
        x
    }

    /// How many times an index was built.
    #[must_use]
    pub fn builds(&self) -> u64 {
        self.builds.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, uid: u32, gid: u32) -> Row {
        Row {
            name: name.into(),
            uid,
            gid,
            groups: Vec::new(),
        }
    }

    #[test]
    fn numbers_map_once_and_zero_is_the_security_officers() {
        let x = IdIndex::new(vec![
            row("QSYS", 101, 0),
            row("QUSER", 0, 0),
            row("QSECOFR", 0, 0),
            row("BOB", 1000, 0),
            row("AAA", 1000, 0),
            row("DEV", 2000, 500),
        ]);
        assert_eq!(x.by_uid(0).unwrap().name, "QSECOFR");
        assert_eq!(x.by_uid(1000).unwrap().name, "AAA", "first by name");
        assert_eq!(x.by_gid(500).unwrap().name, "DEV");
        assert!(x.by_gid(0).is_none());
        assert!(x.is_group("DEV") && !x.is_group("BOB"));
        let c = IdCache::default();
        let a = c.get((1, 1), || vec![row("A", 5, 0)]);
        let b = c.get((1, 1), || unreachable!());
        assert!(Arc::ptr_eq(&a, &b));
        c.get((1, 2), || vec![row("A", 6, 0)]);
        assert_eq!(c.builds(), 2);
    }
}

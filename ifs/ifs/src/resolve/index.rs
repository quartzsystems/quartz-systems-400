// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The per-directory casefold index of case-insensitive file systems:
//! folded name to stored name, built from one `readdir` and cached per
//! backend directory. Every change made through this crate updates or
//! drops the affected entries; a miss is checked against the backend
//! by exact name, so a name created behind the crate's back is found.

use std::collections::HashMap;

use crate::backend::{self, Backend};
use crate::errno::{Errno, Result};
use crate::path::fold::fold;

/// Directories kept at most; past it the cache starts over.
pub const CAPACITY: usize = 4096;

/// The cache.
#[derive(Debug, Default)]
pub struct FoldIndex {
    dirs: HashMap<String, (i64, HashMap<String, Vec<String>>)>,
    /// Indexes built (for tests and statistics).
    pub builds: u64,
}

/// The stored name for `name`: itself when stored, else the first that
/// folds alike.
fn pick(map: &HashMap<String, Vec<String>>, name: &str) -> Option<String> {
    let v = map.get(&fold(name))?;
    Some(v.iter().find(|s| *s == name).unwrap_or(&v[0]).clone())
}

impl FoldIndex {
    fn build<B: Backend>(&mut self, b: &B, dir: &str) -> Result<&HashMap<String, Vec<String>>> {
        if !self.dirs.contains_key(dir) {
            if self.dirs.len() >= CAPACITY {
                self.dirs.clear();
            }
            let mut map = HashMap::new();
            let mtime = b.stat(dir)?.mtime;
            let mut ents = b.readdir(dir)?;
            // Names that fold alike (made outside the crate): the exact
            // name wins, else the first in byte order.
            ents.sort_by(|a, b| a.name.cmp(&b.name));
            for e in ents {
                map.entry(fold(&e.name))
                    .or_insert_with(Vec::new)
                    .push(e.name);
            }
            self.builds += 1;
            self.dirs.insert(dir.to_string(), (mtime, map));
        }
        Ok(&self.dirs[dir].1)
    }

    /// The stored name matching `name` in backend directory `dir`.
    pub fn lookup<B: Backend>(
        &mut self,
        b: &B,
        dir: &str,
        name: &str,
        case_sensitive: bool,
    ) -> Result<String> {
        if case_sensitive {
            b.stat(&backend::join(dir, name))?;
            return Ok(name.to_string());
        }
        if let Some(stored) = pick(self.build(b, dir)?, name) {
            return Ok(stored);
        }
        // Changed outside the crate (the directory changed since the
        // index was built, or the exact name exists)? Then rebuild once.
        let stale = self
            .dirs
            .get(dir)
            .is_some_and(|(t, _)| b.stat(dir).is_ok_and(|s| s.mtime != *t));
        if stale || b.stat(&backend::join(dir, name)).is_ok() {
            self.dirs.remove(dir);
            if let Some(stored) = pick(self.build(b, dir)?, name) {
                return Ok(stored);
            }
        }
        Err(Errno::ENOENT)
    }

    /// A name was added to `dir`.
    pub fn added<B: Backend>(&mut self, b: &B, dir: &str, name: &str) {
        if let Some((t, m)) = self.dirs.get_mut(dir) {
            *t = b.stat(dir).map_or(0, |s| s.mtime);
            let v = m.entry(fold(name)).or_default();
            if !v.iter().any(|s| s == name) {
                v.push(name.to_string());
                v.sort();
            }
        }
    }

    /// A name was removed from `dir`.
    pub fn removed<B: Backend>(&mut self, b: &B, dir: &str, name: &str) {
        if let Some((t, m)) = self.dirs.get_mut(dir) {
            *t = b.stat(dir).map_or(0, |s| s.mtime);
            let f = fold(name);
            if let Some(v) = m.get_mut(&f) {
                v.retain(|s| s != name);
                if v.is_empty() {
                    m.remove(&f);
                }
            }
        }
    }

    /// `dir` and everything under it changed place or went away.
    pub fn drop_tree(&mut self, dir: &str) {
        let prefix = format!("{dir}/");
        self.dirs.retain(|k, _| k != dir && !k.starts_with(&prefix));
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.dirs.clear();
    }

    /// Directories cached.
    #[must_use]
    pub fn len(&self) -> usize {
        self.dirs.len()
    }

    /// Whether nothing is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mem::MemBackend;

    #[test]
    fn folded_lookups() {
        let b = MemBackend::new();
        b.mkdir("d", 0o755).unwrap();
        b.create("d/ReadMe.TXT", 0o644).unwrap();
        let mut ix = FoldIndex::default();
        assert_eq!(
            ix.lookup(&b, "d", "readme.txt", false).unwrap(),
            "ReadMe.TXT"
        );
        assert_eq!(
            ix.lookup(&b, "d", "README.TXT", false).unwrap(),
            "ReadMe.TXT"
        );
        assert_eq!(ix.builds, 1);
        assert_eq!(ix.lookup(&b, "d", "readme.txt", true), Err(Errno::ENOENT));
        assert_eq!(ix.lookup(&b, "d", "other", false), Err(Errno::ENOENT));
        // Made behind the index's back: found by the exact-name check.
        b.create("d/late", 0o644).unwrap();
        assert_eq!(ix.lookup(&b, "d", "late", false).unwrap(), "late");
        // Removed through the crate: the entry goes.
        b.unlink("d/ReadMe.TXT").unwrap();
        ix.removed(&b, "d", "ReadMe.TXT");
        assert_eq!(ix.lookup(&b, "d", "readme.txt", false), Err(Errno::ENOENT));
        ix.added(&b, "d", "x");
        ix.drop_tree("d");
        assert!(ix.is_empty());
    }
}

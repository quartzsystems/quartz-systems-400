// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The current export table (in memory, as IBM's is kernel state): one
//! entry a directory, a stable export ID a path through the IPL, a
//! generation that moves with every change.

use std::collections::BTreeMap;

use nfs::exports::{Export, file};

/// One exported directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its export ID.
    pub id: u32,
    /// The export, host CCSIDs resolved.
    pub export: Export,
    /// The anonymous UID; `None` refuses unknown users.
    pub anon_uid: Option<u32>,
    /// That UID's profile, `*NONE` when none.
    pub anon_profile: String,
    /// The job that exported it.
    pub by: String,
    /// When.
    pub since: u64,
}

/// The table.
#[derive(Debug, Default)]
pub struct ExportTable {
    entries: Vec<Entry>,
    /// Every path ever exported this IPL and its ID, so an export again
    /// keeps its handles valid.
    ids: BTreeMap<String, u32>,
    next: u32,
    generation: u64,
}

fn key(path: &str) -> String {
    file::normal(path).to_ascii_uppercase()
}

impl ExportTable {
    /// The entries, in the order they were exported.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The generation: it moves with every change.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The entry of `path`.
    #[cfg(test)]
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Entry> {
        let k = key(path);
        self.entries.iter().find(|e| key(&e.export.path) == k)
    }

    /// The entry of the nearest exported directory holding `path`.
    #[must_use]
    pub fn holding(&self, path: &str) -> Option<&Entry> {
        let k = key(path);
        self.entries
            .iter()
            .filter(|e| {
                let p = key(&e.export.path);
                p == "/" || k == p || k.starts_with(&format!("{p}/"))
            })
            .max_by_key(|e| e.export.path.len())
    }

    /// Put `e` in, replacing the entry of its path; its ID and whether it
    /// replaced one.
    pub fn put(&mut self, mut e: Entry) -> (u32, bool) {
        let k = key(&e.export.path);
        let id = if let Some(id) = self.ids.get(&k) {
            *id
        } else {
            self.next += 1;
            self.ids.insert(k.clone(), self.next);
            self.next
        };
        e.id = id;
        self.generation += 1;
        if let Some(old) = self.entries.iter_mut().find(|x| key(&x.export.path) == k) {
            *old = e;
            (id, true)
        } else {
            self.entries.push(e);
            (id, false)
        }
    }

    /// Take the entry of `path` out; whether there was one.
    pub fn remove(&mut self, path: &str) -> bool {
        let k = key(path);
        let before = self.entries.len();
        self.entries.retain(|e| key(&e.export.path) != k);
        let gone = self.entries.len() != before;
        if gone {
            self.generation += 1;
        }
        gone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> Entry {
        Entry {
            id: 0,
            export: Export::new(path),
            anon_uid: Some(65534),
            anon_profile: "QNFSANON".into(),
            by: "job".into(),
            since: 1,
        }
    }

    #[test]
    fn ids_last_the_ipl() {
        let mut t = ExportTable::default();
        assert_eq!(t.put(entry("/home/a")), (1, false));
        assert_eq!(t.put(entry("/home/b")), (2, false));
        let g = t.generation();
        assert_eq!(t.put(entry("/HOME/A/")), (1, true));
        assert!(t.generation() > g);
        assert!(t.remove("/home/a"));
        assert!(!t.remove("/home/a"));
        assert_eq!(t.put(entry("/home/a")).0, 1);
        assert_eq!(t.entries().len(), 2);
    }

    #[test]
    fn the_nearest_export_holds_a_path() {
        let mut t = ExportTable::default();
        t.put(entry("/"));
        t.put(entry("/home"));
        t.put(entry("/home/joe"));
        assert_eq!(t.holding("/home/joe/x").unwrap().export.path, "/home/joe");
        assert_eq!(t.holding("/home/joey").unwrap().export.path, "/home");
        assert_eq!(t.holding("/etc").unwrap().export.path, "/");
        assert!(t.get("/home/").is_some());
    }
}

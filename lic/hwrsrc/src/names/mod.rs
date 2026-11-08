// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names that last: each resource's name saved with what identifies
//! its device ([`key`]), so the next IPL gives the device the same name
//! wherever it now sits, a new device the next free name of its kind,
//! and a device gone its name still, not detected, until it is removed.
//! The hardware resource service keeps the map in the LIC partition
//! ([`format`]), read before anything asks for a resource.

pub mod format;
pub mod key;

pub use format::{FormatError, MAX_BYTES, TAG_HW_NAMES, VERSION};
pub use key::{identify, same_device, strong};

use std::path::Path;

use crate::scan::nic::PermMacs;
use crate::table::{Kind, Resource, Status, Table};

/// One saved name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The resource name.
    pub name: String,
    /// Its kind.
    pub kind: Kind,
    /// What identifies the device.
    pub key: String,
    /// Where it was last seen (PCI address, device name).
    pub location: String,
    /// Type and model it last reported; blank when none.
    pub type_model: (String, String),
    /// Its packaging parent then.
    pub parent: String,
    /// Its logical parent then.
    pub logical: String,
}

/// Every saved name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameMap {
    /// Present resources first, then those not detected.
    pub entries: Vec<Entry>,
}

impl NameMap {
    /// The map of a table: every resource with a key.
    #[must_use]
    pub fn of(t: &Table) -> Self {
        let entry = |r: &Resource| Entry {
            name: r.name.clone(),
            kind: r.kind,
            key: r.key.clone(),
            location: r.location.clone(),
            type_model: r.type_model.clone().unwrap_or_default(),
            parent: r.parent.clone(),
            logical: r.logical.clone(),
        };
        let with_key = || t.resources.iter().filter(|r| !r.key.is_empty());
        let mut entries: Vec<Entry> = with_key().filter(|r| r.operational()).map(entry).collect();
        entries.extend(with_key().filter(|r| !r.operational()).map(entry));
        Self { entries }
    }

    /// The bytes saved.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        format::encode(self)
    }

    /// From the bytes saved.
    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        format::decode(bytes)
    }
}

impl Table {
    /// Walk sysfs at an IPL and name by the saved map: a device saved
    /// keeps its name, a saved one not found stays as not detected under
    /// its saved parents, a new one takes the next free name of its kind
    /// in walk order. One line per resource added or not detected.
    #[must_use]
    pub fn scan_named(root: &Path, perm: &PermMacs, map: &NameMap) -> (Self, Vec<String>) {
        let mut t = Self::walk(root, perm);
        let changes = t.name_by(map);
        t.link();
        (t, changes)
    }

    /// Rename a fresh walk by the saved map (see [`Table::scan_named`]).
    fn name_by(&mut self, map: &NameMap) -> Vec<String> {
        let mut used = vec![false; map.entries.len()];
        let mut named = vec![false; self.resources.len()];
        // A saved name seen twice keeps its first entry.
        for (j, e) in map.entries.iter().enumerate() {
            if map.entries[..j]
                .iter()
                .any(|o| o.name.eq_ignore_ascii_case(&e.name))
            {
                used[j] = true;
            }
        }
        for (i, r) in self.resources.iter_mut().enumerate() {
            if r.key.is_empty() {
                continue;
            }
            let hit = map
                .entries
                .iter()
                .enumerate()
                .position(|(j, e)| !used[j] && e.kind == r.kind && e.key == r.key);
            if let Some(j) = hit {
                used[j] = true;
                named[i] = true;
                r.name.clone_from(&map.entries[j].name);
            }
        }
        let mut changes = Vec::new();
        for (e, _) in map.entries.iter().zip(&used).filter(|(_, u)| !**u) {
            let mut r = Resource::new(
                e.name.clone(),
                e.kind,
                e.location.clone(),
                e.kind.text().to_string(),
                String::new(),
            );
            r.status = Status::NotDetected;
            r.key.clone_from(&e.key);
            r.parent.clone_from(&e.parent);
            r.logical.clone_from(&e.logical);
            if !e.type_model.0.is_empty() {
                r.type_model = Some(e.type_model.clone());
            }
            changes.push(format!("{} not detected ({})", r.name, r.location));
            self.resources.push(r);
        }
        let fresh: Vec<usize> = (0..named.len()).filter(|i| !named[*i]).collect();
        for &i in &fresh {
            self.resources[i].name.clear();
        }
        for &i in &fresh {
            let name = self.next_name(self.resources[i].kind);
            changes.push(format!("added {name} ({})", self.resources[i].location));
            self.resources[i].name = name;
        }
        changes
    }
}

#[cfg(test)]
mod tests;

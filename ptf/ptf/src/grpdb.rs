// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF groups a system knows: each group's signed manifest as it
//! arrived (`SNDPTFORD`, `INSPTF`), and the newest level the update
//! service last showed (`QSCHKUPD`), which `WRKPTFGRP` marks. The
//! integrator keeps the encoded bytes (a user space in `QSYS`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::db::PtfDb;
use crate::group::{self, Group, GroupStatus};

/// One group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownGroup {
    /// The signed manifest.
    pub manifest: String,
    /// The newest level the service offers; 0 when unknown.
    pub newest: u32,
}

impl KnownGroup {
    /// The manifest, parsed; `None` when it does not parse.
    #[must_use]
    pub fn group(&self, trusted: &[[u8; 32]]) -> Option<(Group, bool)> {
        group::parse(&self.manifest, trusted).ok()
    }
}

/// Every group, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupDb {
    /// The groups.
    pub groups: BTreeMap<String, KnownGroup>,
}

/// What `WRKPTFGRP` shows for one group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupRow {
    /// Name.
    pub name: String,
    /// Level on the system.
    pub level: u32,
    /// Status.
    pub status: GroupStatus,
    /// Text.
    pub text: String,
    /// A newer level the service offers.
    pub newer: Option<u32>,
}

const MAGIC: &str = "QS400PTFGRP 1";

impl GroupDb {
    /// The stored form: a line per group (`name newest bytes`), then
    /// that many bytes of manifest.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = format!("{MAGIC}\n");
        for (name, g) in &self.groups {
            let _ = writeln!(out, "{name} {} {}", g.newest, g.manifest.len());
            out.push_str(&g.manifest);
        }
        out
    }

    /// Parse the stored form; what does not parse ends the list.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let mut db = Self::default();
        let Some(mut rest) = text.strip_prefix(MAGIC).and_then(|r| r.strip_prefix('\n')) else {
            return db;
        };
        while let Some((line, after)) = rest.split_once('\n') {
            let f: Vec<&str> = line.split(' ').collect();
            let [name, newest, len] = f.as_slice() else { break };
            let (Ok(newest), Ok(len)) = (newest.parse(), len.parse::<usize>()) else {
                break;
            };
            let Some(manifest) = after.get(..len) else { break };
            db.groups.insert(
                (*name).to_string(),
                KnownGroup {
                    manifest: manifest.into(),
                    newest,
                },
            );
            rest = &after[len..];
        }
        db
    }

    /// Keep a manifest that arrived, replacing an older level.
    pub fn put(&mut self, name: &str, manifest: &str) {
        let e = self.groups.entry(name.to_string()).or_insert(KnownGroup {
            manifest: String::new(),
            newest: 0,
        });
        e.manifest = manifest.into();
    }

    /// Note the newest level the service offers.
    pub fn offer(&mut self, name: &str, level: u32) {
        if let Some(g) = self.groups.get_mut(name) {
            g.newest = level;
        }
    }

    /// The rows `WRKPTFGRP` shows, by name.
    #[must_use]
    pub fn rows(&self, db: &PtfDb, trusted: &[[u8; 32]]) -> Vec<GroupRow> {
        self.groups
            .iter()
            .map(|(name, k)| match k.group(trusted) {
                Some((g, ok)) => GroupRow {
                    name: name.clone(),
                    level: g.level,
                    status: group::status(&g, ok, db),
                    text: g.text.clone(),
                    newer: (k.newest > g.level).then_some(k.newest),
                },
                None => GroupRow {
                    name: name.clone(),
                    level: 0,
                    status: GroupStatus::Unknown,
                    text: String::new(),
                    newer: None,
                },
            })
            .collect()
    }
}

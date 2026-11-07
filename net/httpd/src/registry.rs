// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instance registry beside the instances (`/www/instances.json`):
//! each instance's description, autostart flag, hot backup partner, the
//! last start and end, and whether its configuration changed since it
//! started.

use serde::{Deserialize, Serialize};

use crate::files::Files;
use crate::{Error, Result};

/// Where the registry lives.
pub const PATH: &str = "/www/instances.json";

/// One instance's registry entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The instance (and job) name, upper case.
    pub name: String,
    /// Its text.
    pub description: String,
    /// The server root (`/www/webstore`).
    pub root: String,
    /// Started by `STRTCP` / `STRTCPSVR *HTTP` without `HTTPSVR`.
    pub autostart: bool,
    /// The hot backup partner instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hot_backup: Option<String>,
    /// The hot backup partner takes over when this instance fails
    /// (the partner is remembered while switched off).
    #[serde(default)]
    pub hot_backup_active: bool,
    /// Seconds since 1970 of the last start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_started: Option<u64>,
    /// Seconds since 1970 of the last end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ended: Option<u64>,
    /// How it last ended (`ENDTCPSVR *HTTP, controlled`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ended_how: String,
    /// The configuration changed while it ran.
    #[serde(default)]
    pub restart_required: bool,
    /// Seconds since 1970 of creation.
    #[serde(default)]
    pub created: u64,
    /// The `DELAY` of the `ENDTCPSVR` ending the running job, which the
    /// job reads when told to end; cleared when it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_delay: Option<u32>,
}

/// Every instance's entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    /// Format version.
    pub version: u32,
    /// The entries, by name.
    pub instances: Vec<Entry>,
}

impl Registry {
    /// Read the registry; an absent file is an empty registry.
    pub fn load(files: &dyn Files) -> Result<Self> {
        match files.read(PATH) {
            Ok(b) => serde_json::from_slice(&b).map_err(|e| Error::Damaged(format!("{PATH}: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                version: 1,
                instances: Vec::new(),
            }),
            Err(e) => Err(Error::Io(PATH.into(), e.to_string())),
        }
    }

    /// Write the registry whole.
    pub fn save(&self, files: &dyn Files) -> Result<()> {
        let mut text =
            serde_json::to_string_pretty(self).map_err(|e| Error::Damaged(e.to_string()))?;
        text.push('\n');
        files
            .mkdir_all("/www")
            .map_err(|e| Error::Io("/www".into(), e.to_string()))?;
        files
            .write_atomic(PATH, text.as_bytes())
            .map_err(|e| Error::Io(PATH.into(), e.to_string()))
    }

    /// Entry `name` (case-blind).
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.instances
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Entry `name`, to change.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Entry> {
        self.instances
            .iter_mut()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Replace an entry in place, or add it at the end (the list keeps
    /// creation order).
    pub fn put(&mut self, e: Entry) {
        match self.get_mut(&e.name) {
            Some(x) => *x = e,
            None => self.instances.push(e),
        }
    }

    /// Remove entry `name`; whether it was there.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.instances.len();
        self.instances
            .retain(|x| !x.name.eq_ignore_ascii_case(name));
        before != self.instances.len()
    }

    /// Change entry `name` and save; `NotFound` when it is not there.
    pub fn update(files: &dyn Files, name: &str, f: impl FnOnce(&mut Entry)) -> Result<()> {
        let mut r = Self::load(files)?;
        let e = r
            .get_mut(name)
            .ok_or_else(|| Error::NotFound(name.to_string()))?;
        f(e);
        r.save(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    #[test]
    fn load_put_save() {
        let f = MemFiles::new();
        let mut r = Registry::load(&f).unwrap();
        assert!(r.instances.is_empty());
        r.put(Entry {
            name: "WEBSTORE".into(),
            root: "/www/webstore".into(),
            ..Entry::default()
        });
        r.put(Entry {
            name: "ADMIN".into(),
            autostart: true,
            ..Entry::default()
        });
        r.save(&f).unwrap();
        let back = Registry::load(&f).unwrap();
        assert_eq!(
            back.instances
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            ["WEBSTORE", "ADMIN"]
        );
        Registry::update(&f, "webstore", |e| e.restart_required = true).unwrap();
        assert!(
            Registry::load(&f)
                .unwrap()
                .get("WEBSTORE")
                .unwrap()
                .restart_required
        );
        assert!(matches!(
            Registry::update(&f, "NONE", |_| {}),
            Err(Error::NotFound(_))
        ));
        f.put(PATH, b"{bad");
        assert!(matches!(Registry::load(&f), Err(Error::Damaged(_))));
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The mount list `DUMP` answers: (client host, directory) pairs `MNT`
//! adds and `UMNT`/`UMNTALL` remove. The oracle keeps no `rmtab`-like
//! file in `/etc`, so QS/400 keeps the list in memory ([`MemMountList`])
//! and it starts empty with each start of `QNFSMNTD`, as a client that
//! still has the export mounted keeps working without it.

use std::sync::Mutex;

/// The mount list.
pub trait MountList: Send + Sync {
    /// `host` mounted `dir`.
    fn add(&self, host: &str, dir: &str);
    /// `host` unmounted `dir`.
    fn remove(&self, host: &str, dir: &str);
    /// `host` unmounted everything.
    fn remove_host(&self, host: &str);
    /// Every (host, directory), in the order mounted.
    fn list(&self) -> Vec<(String, String)>;
}

/// The list in memory.
#[derive(Debug, Default)]
pub struct MemMountList(Mutex<Vec<(String, String)>>);

impl MemMountList {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut Vec<(String, String)>) -> T) -> T {
        f(&mut self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }
}

impl MountList for MemMountList {
    fn add(&self, host: &str, dir: &str) {
        self.with(|l| {
            if !l.iter().any(|(h, d)| h == host && d == dir) {
                l.push((host.into(), dir.into()));
            }
        });
    }
    fn remove(&self, host: &str, dir: &str) {
        self.with(|l| l.retain(|(h, d)| !(h == host && d == dir)));
    }
    fn remove_host(&self, host: &str) {
        self.with(|l| l.retain(|(h, _)| h != host));
    }
    fn list(&self) -> Vec<(String, String)> {
        self.with(|l| l.clone())
    }
}

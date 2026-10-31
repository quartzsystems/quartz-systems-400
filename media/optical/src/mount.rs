// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The loaded volumes: what `/QOPT` shows, one per volume identifier.
//! Loading an image catalog or a physical disc puts its volume here;
//! unloading flushes and takes it away. Shared between threads; the
//! clock stamps flushed volumes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::error::{Error, Result};
use crate::volume::Volume;

/// A clock: Unix seconds.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The set.
#[derive(Clone)]
pub struct VolumeSet {
    inner: Arc<Mutex<BTreeMap<String, Volume>>>,
    clock: Clock,
}

impl std::fmt::Debug for VolumeSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VolumeSet")
            .field("volumes", &self.names())
            .finish()
    }
}

impl Default for VolumeSet {
    fn default() -> Self {
        Self::new(Arc::new(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        }))
    }
}

impl VolumeSet {
    /// An empty set stamping with `clock`.
    #[must_use]
    pub fn new(clock: Clock) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BTreeMap::new())),
            clock,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, BTreeMap<String, Volume>>> {
        self.inner
            .lock()
            .map_err(|_| Error::Invalid("volume set lock poisoned".into()))
    }

    /// The clock's time.
    #[must_use]
    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Add a volume; its identifier must be free.
    pub fn insert(&self, v: Volume) -> Result<()> {
        let id = v.id();
        let mut g = self.lock()?;
        if g.contains_key(&id) {
            return Err(Error::Exists(id));
        }
        g.insert(id, v);
        Ok(())
    }

    /// Flush and take a volume away.
    pub fn remove(&self, volid: &str) -> Result<Option<Volume>> {
        let now = self.now();
        let mut g = self.lock()?;
        let key = g.keys().find(|k| k.eq_ignore_ascii_case(volid)).cloned();
        let Some(key) = key else {
            return Ok(None);
        };
        if let Some(v) = g.get_mut(&key) {
            v.flush(now)?;
        }
        Ok(g.remove(&key))
    }

    /// The identifiers, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Run `f` on one volume (identifier without regard to case).
    pub fn with<T>(&self, volid: &str, f: impl FnOnce(&mut Volume) -> Result<T>) -> Result<T> {
        let mut g = self.lock()?;
        let key = g
            .keys()
            .find(|k| k.eq_ignore_ascii_case(volid))
            .cloned()
            .ok_or_else(|| Error::NotFound(volid.to_string()))?;
        f(g.get_mut(&key).expect("found"))
    }

    /// Write a volume's pending changes.
    pub fn sync(&self, volid: &str) -> Result<bool> {
        let now = self.now();
        self.with(volid, |v| v.flush(now))
    }

    /// Write every volume's pending changes.
    pub fn sync_all(&self) -> Result<()> {
        let now = self.now();
        for v in self.lock()?.values_mut() {
            v.flush(now)?;
        }
        Ok(())
    }
}

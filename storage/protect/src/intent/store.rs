// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a write-intent map is kept: the caller's.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Keeps one map's bytes ([`IntentMap::encode`](super::IntentMap::encode)'s)
/// wherever the caller keeps the set's metadata.
pub trait IntentStore: Send {
    /// Every byte kept; empty when nothing is.
    fn read_all(&mut self) -> io::Result<Vec<u8>>;
    /// Put `bytes` at `offset` and make them durable before returning.
    /// A write past the end extends what is kept.
    fn write(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()>;
}

#[derive(Debug, Default)]
struct Mem {
    bytes: Vec<u8>,
    writes: u64,
    fail: bool,
}

/// A map kept in memory, for tests. Clones are handles on the same
/// bytes, so a test keeps one to look at what the set wrote.
#[derive(Debug, Clone, Default)]
pub struct MemIntentStore {
    inner: Arc<Mutex<Mem>>,
}

impl MemIntentStore {
    /// Nothing kept.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Mem> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The bytes kept.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.lock().bytes.clone()
    }

    /// Writes made so far.
    #[must_use]
    pub fn writes(&self) -> u64 {
        self.lock().writes
    }

    /// Every write fails while `on`.
    pub fn fail(&self, on: bool) {
        self.lock().fail = on;
    }
}

impl IntentStore for MemIntentStore {
    fn read_all(&mut self) -> io::Result<Vec<u8>> {
        Ok(self.bytes())
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        let mut m = self.lock();
        if m.fail {
            return Err(io::Error::other("injected"));
        }
        m.writes += 1;
        let at = usize::try_from(offset).map_err(io::Error::other)?;
        if m.bytes.len() < at + bytes.len() {
            m.bytes.resize(at + bytes.len(), 0);
        }
        m.bytes[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}

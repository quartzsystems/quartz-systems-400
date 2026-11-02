// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The volume for another thread: volume pages read and written through
//! the ASP's shared I/O handle, never through the store. The store's
//! thread hands it a new volume after each growth it has checkpointed.

use std::sync::{Arc, PoisonError, RwLock};

use crate::asp::ifs::Volume;
use crate::asp::io::SharedIo;
use crate::page::PAGE_SIZE;
use crate::{Result, StoreError};

/// The volume and the units under it.
#[derive(Clone)]
pub struct IfsIo {
    io: SharedIo,
    volume: Arc<RwLock<Volume>>,
}

fn beyond(v: u64) -> StoreError {
    StoreError::Corrupt {
        page: v,
        what: "beyond the IFS volume".into(),
    }
}

impl IfsIo {
    /// Over `io` with `volume`.
    #[must_use]
    pub fn new(io: SharedIo, volume: Volume) -> Self {
        Self {
            io,
            volume: Arc::new(RwLock::new(volume)),
        }
    }

    /// The volume grew: its new extents.
    pub fn set_volume(&self, volume: Volume) {
        *self.volume.write().unwrap_or_else(PoisonError::into_inner) = volume;
    }

    /// Pages of the volume.
    #[must_use]
    pub fn pages(&self) -> u64 {
        self.volume.read().unwrap_or_else(PoisonError::into_inner).pages()
    }

    fn runs(&self, v: u64, count: u64) -> Result<Vec<(u64, u64)>> {
        self.volume
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .runs(v, count)
            .ok_or_else(|| beyond(v))
    }

    /// Read whole volume pages from `v` into `buf`.
    pub fn read(&self, v: u64, buf: &mut [u8]) -> Result<()> {
        let mut at = 0;
        for (pno, n) in self.runs(v, (buf.len() / PAGE_SIZE) as u64)? {
            let got = self.io.read_run(pno, n)?;
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            buf[at..at + len].copy_from_slice(&got[..len]);
            at += len;
        }
        Ok(())
    }

    /// Write whole volume pages from `v`. `bytes` is block aligned when
    /// the units are opened for direct I/O.
    pub fn write(&self, v: u64, bytes: &[u8]) -> Result<()> {
        let mut at = 0;
        for (pno, n) in self.runs(v, (bytes.len() / PAGE_SIZE) as u64)? {
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            self.io.write_bytes(pno, &bytes[at..at + len])?;
            at += len;
        }
        Ok(())
    }

    /// Make every write so far durable.
    pub fn flush(&self) -> Result<()> {
        self.io.flush()
    }
}

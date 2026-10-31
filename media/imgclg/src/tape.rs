// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Virtual tape volumes as a catalog sees them. The volume file format
//! belongs to the tape crate; a `*TAP` catalog calls it through this
//! trait to make new volumes and to read an existing one's name.

use crate::fs::CatalogFs;

/// What a `*TAP` catalog needs of virtual tape volumes.
pub trait TapeImages: Send + Sync {
    /// Make a new empty volume file at `path` named `volume`, `size_mb`
    /// MiB at most.
    fn create(
        &self,
        fs: &dyn CatalogFs,
        path: &str,
        volume: &str,
        size_mb: u64,
    ) -> Result<(), String>;
    /// The volume name recorded in an existing volume file.
    fn volume_id(&self, fs: &dyn CatalogFs, path: &str) -> Result<String, String>;
}

/// No tape support: every call fails.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTape;

impl TapeImages for NoTape {
    fn create(&self, _: &dyn CatalogFs, _: &str, _: &str, _: u64) -> Result<(), String> {
        Err("virtual tape volumes are not available".into())
    }
    fn volume_id(&self, _: &dyn CatalogFs, _: &str) -> Result<String, String> {
        Err("virtual tape volumes are not available".into())
    }
}

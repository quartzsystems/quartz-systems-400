// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QOPT`: optical volumes, read-only. Phase 11 implements
//! [`OpticalView`] over image catalogs; until then [`NoOptical`] has no
//! volumes.

use crate::backend::Kind;
use crate::errno::{Errno, Result};

/// One entry of an optical directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptEntry {
    /// Name.
    pub name: String,
    /// Directory or stream file.
    pub kind: Kind,
    /// Size in bytes.
    pub size: u64,
}

/// Optical volumes.
pub trait OpticalView: Send + Sync {
    /// Volume names.
    fn volumes(&self) -> Vec<String>;
    /// Entries of a directory in a volume (`path` relative to it).
    fn readdir(&self, vol: &str, path: &[String]) -> Result<Vec<OptEntry>>;
    /// One entry.
    fn stat(&self, vol: &str, path: &[String]) -> Result<OptEntry>;
    /// A file's bytes from `off`.
    fn read(&self, vol: &str, path: &[String], off: u64, buf: &mut [u8]) -> Result<usize>;
}

/// No volumes.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOptical;

impl OpticalView for NoOptical {
    fn volumes(&self) -> Vec<String> {
        Vec::new()
    }
    fn readdir(&self, _vol: &str, _path: &[String]) -> Result<Vec<OptEntry>> {
        Err(Errno::ENOENT)
    }
    fn stat(&self, _vol: &str, _path: &[String]) -> Result<OptEntry> {
        Err(Errno::ENOENT)
    }
    fn read(&self, _vol: &str, _path: &[String], _off: u64, _buf: &mut [u8]) -> Result<usize> {
        Err(Errno::ENOENT)
    }
}

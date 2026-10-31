// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QOPT`: optical volumes. [`OpticalView`] is what the resolver and
//! the operations ask; [`Volumes`] answers from the loaded volumes of
//! the `optical` crate, read-only for ISO 9660 images and read-write
//! for writable virtual volumes; [`NoOptical`] has no volumes.

use optical::{EntryKind, VolumeSet};

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

/// Optical volumes. The writing calls fail with `EROFS` unless a view
/// says the volume is writable.
pub trait OpticalView: Send + Sync {
    /// Volume names.
    fn volumes(&self) -> Vec<String>;
    /// Entries of a directory in a volume (`path` relative to it).
    fn readdir(&self, vol: &str, path: &[String]) -> Result<Vec<OptEntry>>;
    /// One entry.
    fn stat(&self, vol: &str, path: &[String]) -> Result<OptEntry>;
    /// A file's bytes from `off`.
    fn read(&self, vol: &str, path: &[String], off: u64, buf: &mut [u8]) -> Result<usize>;

    /// Whether the volume takes writes.
    fn writable(&self, _vol: &str) -> bool {
        false
    }
    /// Make an empty file.
    fn create(&self, _vol: &str, _path: &[String]) -> Result<()> {
        Err(Errno::EROFS)
    }
    /// Replace a file's bytes.
    fn put(&self, _vol: &str, _path: &[String], _data: &[u8]) -> Result<()> {
        Err(Errno::EROFS)
    }
    /// Make a directory.
    fn mkdir(&self, _vol: &str, _path: &[String]) -> Result<()> {
        Err(Errno::EROFS)
    }
    /// Remove a file or an empty directory.
    fn remove(&self, _vol: &str, _path: &[String]) -> Result<()> {
        Err(Errno::EROFS)
    }
    /// Write the volume's changes to its medium.
    fn sync(&self, _vol: &str) -> Result<()> {
        Ok(())
    }
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

/// The loaded volumes of the `optical` crate.
#[derive(Debug, Clone)]
pub struct Volumes {
    set: VolumeSet,
}

impl Volumes {
    /// A view of `set`.
    #[must_use]
    pub fn new(set: VolumeSet) -> Self {
        Self { set }
    }
}

/// The `errno` for an optical error.
#[must_use]
pub fn errno_of(e: &optical::Error) -> Errno {
    use optical::Error as E;
    match e {
        E::NotFound(_) => Errno::ENOENT,
        E::Exists(_) => Errno::EEXIST,
        E::NotDir(_) => Errno::ENOTDIR,
        E::IsDir(_) => Errno::EISDIR,
        E::NotEmpty(_) => Errno::ENOTEMPTY,
        E::BadName(_) => Errno::ENAMETOOLONG,
        E::VolumeFull { .. } => Errno::ENOSPC,
        E::WriteProtected | E::ReadOnly => Errno::EROFS,
        E::Io(_) | E::Malformed(_) => Errno::EIO,
        E::BadVolumeId(_) | E::ActiveFiles | E::Invalid(_) => Errno::EINVAL,
    }
}

fn entry(e: optical::VolEntry) -> OptEntry {
    OptEntry {
        name: e.name,
        kind: if e.kind == EntryKind::Dir {
            Kind::Dir
        } else {
            Kind::File
        },
        size: e.size,
    }
}

fn p(path: &[String]) -> String {
    format!("/{}", path.join("/"))
}

impl OpticalView for Volumes {
    fn volumes(&self) -> Vec<String> {
        self.set.names()
    }
    fn readdir(&self, vol: &str, path: &[String]) -> Result<Vec<OptEntry>> {
        self.set
            .with(vol, |v| v.list(&p(path)))
            .map(|v| v.into_iter().map(entry).collect())
            .map_err(|e| errno_of(&e))
    }
    fn stat(&self, vol: &str, path: &[String]) -> Result<OptEntry> {
        self.set
            .with(vol, |v| v.stat(&p(path)))
            .map(entry)
            .map_err(|e| errno_of(&e))
    }
    fn read(&self, vol: &str, path: &[String], off: u64, buf: &mut [u8]) -> Result<usize> {
        self.set
            .with(vol, |v| v.read(&p(path), off, buf))
            .map_err(|e| errno_of(&e))
    }
    fn writable(&self, vol: &str) -> bool {
        self.set.with(vol, |v| Ok(v.is_writable())).unwrap_or(false)
    }
    fn create(&self, vol: &str, path: &[String]) -> Result<()> {
        self.set
            .with(vol, |v| v.create_file(&p(path)))
            .map_err(|e| errno_of(&e))
    }
    fn put(&self, vol: &str, path: &[String], data: &[u8]) -> Result<()> {
        self.set
            .with(vol, |v| v.put_file(&p(path), data.to_vec(), true))
            .map_err(|e| errno_of(&e))
    }
    fn mkdir(&self, vol: &str, path: &[String]) -> Result<()> {
        self.set
            .with(vol, |v| v.mkdir(&p(path)))
            .map_err(|e| errno_of(&e))
    }
    fn remove(&self, vol: &str, path: &[String]) -> Result<()> {
        self.set
            .with(vol, |v| v.remove(&p(path)))
            .map_err(|e| errno_of(&e))
    }
    fn sync(&self, vol: &str) -> Result<()> {
        self.set.sync(vol).map(|_| ()).map_err(|e| errno_of(&e))
    }
}

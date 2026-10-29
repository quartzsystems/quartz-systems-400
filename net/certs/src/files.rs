// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file layer under the stores. The IFS will implement
//! [`StoreFiles`]; [`DirFiles`] maps IFS paths under a host directory and
//! [`MemFiles`] keeps everything in memory.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Whole-file access by absolute IFS path (`/QIBM/...`).
pub trait StoreFiles: Send + Sync {
    /// The file's bytes, or `None` when it does not exist.
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>>;
    /// Replace the file atomically, creating parent directories.
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Remove the file. Removing a missing file is not an error.
    fn remove(&self, path: &str) -> io::Result<()>;

    /// Whether the file exists.
    fn exists(&self, path: &str) -> io::Result<bool> {
        Ok(self.read(path)?.is_some())
    }
}

/// IFS paths mapped under a host directory: `/QIBM/x` is `root/QIBM/x`.
#[derive(Debug, Clone)]
pub struct DirFiles {
    root: PathBuf,
}

impl DirFiles {
    /// Files under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn host(&self, path: &str) -> io::Result<PathBuf> {
        let rel = path.trim_start_matches('/');
        if rel.split('/').any(|c| c == "..") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path leaves the root",
            ));
        }
        Ok(self.root.join(rel))
    }
}

impl StoreFiles for DirFiles {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.host(path)?) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let target = self.host(path)?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = tmp_name(&target);
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &target)
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        match std::fs::remove_file(self.host(path)?) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

fn tmp_name(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".qstmp");
    target.with_file_name(name)
}

/// Everything in memory.
#[derive(Debug, Default)]
pub struct MemFiles {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemFiles {
    /// An empty file system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Paths present, sorted.
    pub fn paths(&self) -> Vec<String> {
        self.files
            .lock()
            .expect("files lock")
            .keys()
            .cloned()
            .collect()
    }
}

impl StoreFiles for MemFiles {
    fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        Ok(self.files.lock().expect("files lock").get(path).cloned())
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.files
            .lock()
            .expect("files lock")
            .insert(path.to_string(), data.to_vec());
        Ok(())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        self.files.lock().expect("files lock").remove(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_files_round_trip_and_refuse_dotdot() {
        let dir = std::env::temp_dir().join(format!("qs400-certs-files-{}", std::process::id()));
        let f = DirFiles::new(&dir);
        assert_eq!(f.read("/QIBM/a/b.KDB").unwrap(), None);
        f.write("/QIBM/a/b.KDB", b"one").unwrap();
        f.write("/QIBM/a/b.KDB", b"two").unwrap();
        assert_eq!(f.read("/QIBM/a/b.KDB").unwrap().unwrap(), b"two");
        assert!(f.exists("/QIBM/a/b.KDB").unwrap());
        f.remove("/QIBM/a/b.KDB").unwrap();
        f.remove("/QIBM/a/b.KDB").unwrap();
        assert!(f.read("/../x").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mem_files() {
        let f = MemFiles::new();
        f.write("/a", b"x").unwrap();
        assert_eq!(f.paths(), vec!["/a".to_string()]);
        f.remove("/a").unwrap();
        assert!(!f.exists("/a").unwrap());
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file system catalogs live in: a small trait over IFS-style
//! absolute paths, with [`StdFs`] (a host directory standing for the
//! root) and [`MemFs`] (memory). The IFS gets its own implementation
//! later.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use optical::{FileImage, ImageStore, MemImage, ReadAt};

/// What catalogs need of a file system. Paths are absolute and
/// `/`-separated.
pub trait CatalogFs: Send + Sync {
    /// Whether anything is at `path`.
    fn exists(&self, path: &str) -> bool;
    /// Whether `path` is a directory.
    fn is_dir(&self, path: &str) -> bool;
    /// Make a directory and its parents.
    fn create_dir_all(&self, path: &str) -> io::Result<()>;
    /// Remove an empty directory.
    fn remove_dir(&self, path: &str) -> io::Result<()>;
    /// Names in a directory, sorted.
    fn list(&self, dir: &str) -> io::Result<Vec<String>>;
    /// A whole file.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    /// Replace a whole file (made if missing).
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Remove a file.
    fn remove_file(&self, path: &str) -> io::Result<()>;
    /// A file's length.
    fn size(&self, path: &str) -> io::Result<u64>;
    /// An existing file as an image to read and rewrite.
    fn image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>>;
    /// A new empty file as an image; an existing one is an error.
    fn create_image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>>;

    /// Copy a file.
    fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        let data = self.read(from)?;
        self.write(to, &data)
    }
}

/// `dir` and `name` joined.
#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    format!(
        "{}/{}",
        dir.trim_end_matches('/'),
        name.trim_start_matches('/')
    )
}

/// The last component of a path.
#[must_use]
pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// The directory of a path.
#[must_use]
pub fn parent(path: &str) -> &str {
    let p = path.trim_end_matches('/');
    match p.rfind('/') {
        Some(0) => "/",
        Some(i) => &p[..i],
        None => "",
    }
}

/// A host directory standing for the root.
#[derive(Debug, Clone)]
pub struct StdFs {
    root: PathBuf,
}

impl StdFs {
    /// Paths resolve under `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn host(&self, path: &str) -> PathBuf {
        let mut p = self.root.clone();
        for c in path
            .split('/')
            .filter(|c| !c.is_empty() && *c != "." && *c != "..")
        {
            p.push(c);
        }
        p
    }
}

impl CatalogFs for StdFs {
    fn exists(&self, path: &str) -> bool {
        self.host(path).exists()
    }
    fn is_dir(&self, path: &str) -> bool {
        self.host(path).is_dir()
    }
    fn create_dir_all(&self, path: &str) -> io::Result<()> {
        fs::create_dir_all(self.host(path))
    }
    fn remove_dir(&self, path: &str) -> io::Result<()> {
        fs::remove_dir(self.host(path))
    }
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        let mut v: Vec<String> = fs::read_dir(self.host(dir))?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        Ok(v)
    }
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.host(path))
    }
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let p = self.host(path);
        let mut tmp = p.clone().into_os_string();
        tmp.push(".new");
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &p)
    }
    fn remove_file(&self, path: &str) -> io::Result<()> {
        fs::remove_file(self.host(path))
    }
    fn size(&self, path: &str) -> io::Result<u64> {
        Ok(fs::metadata(self.host(path))?.len())
    }
    fn image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>> {
        Ok(Arc::new(FileImage::open(&self.host(path))?))
    }
    fn create_image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>> {
        Ok(Arc::new(FileImage::create(&self.host(path), &[])?))
    }
}

#[derive(Default)]
struct MemState {
    dirs: BTreeSet<String>,
    files: BTreeMap<String, Arc<MemImage>>,
}

/// A file system in memory.
#[derive(Default, Clone)]
pub struct MemFs {
    st: Arc<Mutex<MemState>>,
}

impl std::fmt::Debug for MemFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemFs")
    }
}

fn norm(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    format!("/{}", parts.join("/"))
}

fn not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("{path} not found"))
}

impl MemFs {
    /// An empty file system.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut MemState) -> io::Result<T>) -> io::Result<T> {
        let mut g = self
            .st
            .lock()
            .map_err(|_| io::Error::other("file system lock poisoned"))?;
        f(&mut g)
    }

    fn parent_ok(st: &MemState, path: &str) -> bool {
        let p = parent(path);
        p == "/" || p.is_empty() || st.dirs.contains(p)
    }
}

impl CatalogFs for MemFs {
    fn exists(&self, path: &str) -> bool {
        let p = norm(path);
        self.with(|s| Ok(p == "/" || s.dirs.contains(&p) || s.files.contains_key(&p)))
            .unwrap_or(false)
    }
    fn is_dir(&self, path: &str) -> bool {
        let p = norm(path);
        self.with(|s| Ok(p == "/" || s.dirs.contains(&p)))
            .unwrap_or(false)
    }
    fn create_dir_all(&self, path: &str) -> io::Result<()> {
        let p = norm(path);
        self.with(|s| {
            let mut at = String::new();
            for c in p.split('/').filter(|c| !c.is_empty()) {
                at.push('/');
                at.push_str(c);
                if s.files.contains_key(&at) {
                    return Err(io::Error::new(io::ErrorKind::AlreadyExists, at.clone()));
                }
                s.dirs.insert(at.clone());
            }
            Ok(())
        })
    }
    fn remove_dir(&self, path: &str) -> io::Result<()> {
        let p = norm(path);
        self.with(|s| {
            let prefix = format!("{p}/");
            if s.dirs.iter().any(|d| d.starts_with(&prefix))
                || s.files.keys().any(|f| f.starts_with(&prefix))
            {
                return Err(io::Error::new(io::ErrorKind::DirectoryNotEmpty, p.clone()));
            }
            if !s.dirs.remove(&p) {
                return Err(not_found(&p));
            }
            Ok(())
        })
    }
    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        let p = norm(dir);
        self.with(|s| {
            if p != "/" && !s.dirs.contains(&p) {
                return Err(not_found(&p));
            }
            let prefix = if p == "/" {
                "/".to_string()
            } else {
                format!("{p}/")
            };
            let names: BTreeSet<String> = s
                .dirs
                .iter()
                .chain(s.files.keys())
                .filter_map(|x| x.strip_prefix(&prefix))
                .filter(|rest| !rest.is_empty() && !rest.contains('/'))
                .map(ToString::to_string)
                .collect();
            Ok(names.into_iter().collect())
        })
    }
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let p = norm(path);
        self.with(|s| {
            s.files
                .get(&p)
                .map(|f| f.bytes())
                .ok_or_else(|| not_found(&p))
        })
    }
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let p = norm(path);
        self.with(|s| {
            if !Self::parent_ok(s, &p) || s.dirs.contains(&p) {
                return Err(not_found(parent(&p)));
            }
            s.files.insert(p, Arc::new(MemImage::new(data.to_vec())));
            Ok(())
        })
    }
    fn remove_file(&self, path: &str) -> io::Result<()> {
        let p = norm(path);
        self.with(|s| s.files.remove(&p).map(|_| ()).ok_or_else(|| not_found(&p)))
    }
    fn size(&self, path: &str) -> io::Result<u64> {
        let p = norm(path);
        self.with(|s| s.files.get(&p).ok_or_else(|| not_found(&p))?.size())
    }
    fn image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>> {
        let p = norm(path);
        self.with(|s| {
            let f: Arc<dyn ImageStore> = s.files.get(&p).ok_or_else(|| not_found(&p))?.clone();
            Ok(f)
        })
    }
    fn create_image(&self, path: &str) -> io::Result<Arc<dyn ImageStore>> {
        let p = norm(path);
        self.with(|s| {
            if s.files.contains_key(&p) || s.dirs.contains(&p) {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, p.clone()));
            }
            if !Self::parent_ok(s, &p) {
                return Err(not_found(parent(&p)));
            }
            let f = Arc::new(MemImage::default());
            s.files.insert(p, f.clone());
            let f: Arc<dyn ImageStore> = f;
            Ok(f)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(join("/a/", "b"), "/a/b");
        assert_eq!(file_name("/a/b.iso"), "b.iso");
        assert_eq!(parent("/a/b.iso"), "/a");
        assert_eq!(parent("/b"), "/");
        let m = MemFs::new();
        m.create_dir_all("/x/y").unwrap();
        m.write("/x/y/f", b"1").unwrap();
        assert_eq!(m.list("/x").unwrap(), ["y"]);
        assert_eq!(m.list("/x/y").unwrap(), ["f"]);
        assert!(m.remove_dir("/x/y").is_err());
        assert!(m.write("/nope/f", b"").is_err());
        assert_eq!(m.size("/x/y/f").unwrap(), 1);
    }
}

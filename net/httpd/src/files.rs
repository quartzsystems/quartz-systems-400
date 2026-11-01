// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The files instances live in, by IFS path. The running system answers
//! through the IFS; [`MemFiles`] is for tests and fixtures, [`DirFiles`]
//! maps IFS paths under a host directory.

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Mutex;

/// A directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name within the directory.
    pub name: String,
    /// A directory.
    pub dir: bool,
    /// Bytes (0 for a directory).
    pub size: u64,
    /// Seconds since 1970 of the last change.
    pub modified: u64,
}

/// File access by IFS path (`/www/webstore/conf/httpd.conf`).
pub trait Files: Send + Sync {
    /// A whole file.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    /// Replace a file whole: written to a temporary name beside it, then
    /// renamed, so a reader sees the old file or the new one.
    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Add to the end of a file, creating it.
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Rename, replacing `to`.
    fn rename(&self, from: &str, to: &str) -> io::Result<()>;
    /// Copy a file, replacing `to`.
    fn copy(&self, from: &str, to: &str) -> io::Result<()> {
        let d = self.read(from)?;
        self.write_atomic(to, &d)
    }
    /// Create a directory and its parents.
    fn mkdir_all(&self, path: &str) -> io::Result<()>;
    /// Remove a file.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// Remove a directory and everything under it.
    fn remove_tree(&self, path: &str) -> io::Result<()>;
    /// A directory's entries, by name.
    fn list(&self, dir: &str) -> io::Result<Vec<Entry>>;
    /// The entry at `path`.
    fn stat(&self, path: &str) -> io::Result<Entry>;
    /// Whether `path` exists.
    fn exists(&self, path: &str) -> bool {
        self.stat(path).is_ok()
    }
    /// A file to stream.
    fn open(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(io::Cursor::new(self.read(path)?)))
    }
}

fn not_found(p: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("{p} not found"))
}

fn parent(p: &str) -> &str {
    match p.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((d, _)) => d,
    }
}

fn norm(p: &str) -> String {
    let t = p.trim_end_matches('/');
    if t.is_empty() { "/".into() } else { t.into() }
}

#[derive(Clone, Debug)]
enum MemNode {
    Dir,
    File(Vec<u8>, u64),
}

/// Files in memory.
#[derive(Debug, Default)]
pub struct MemFiles {
    nodes: Mutex<BTreeMap<String, MemNode>>,
    /// The time stamped on writes.
    pub now: u64,
}

impl MemFiles {
    /// An empty tree.
    pub fn new() -> Self {
        let m = Self::default();
        m.nodes.lock().expect("files").insert("/".into(), MemNode::Dir);
        m
    }

    /// Put a file, creating its directories.
    pub fn put(&self, path: &str, data: &[u8]) {
        let _ = self.mkdir_all(parent(path));
        self.nodes.lock().expect("files").insert(norm(path), MemNode::File(data.to_vec(), self.now));
    }

    /// Every path held, in order.
    pub fn paths(&self) -> Vec<String> {
        self.nodes.lock().expect("files").keys().cloned().collect()
    }
}

impl Files for MemFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        match self.nodes.lock().expect("files").get(&norm(path)) {
            Some(MemNode::File(d, _)) => Ok(d.clone()),
            Some(MemNode::Dir) => Err(io::Error::other(format!("{path} is a directory"))),
            None => Err(not_found(path)),
        }
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        if !matches!(self.nodes.lock().expect("files").get(parent(path)), Some(MemNode::Dir)) {
            return Err(not_found(parent(path)));
        }
        self.nodes.lock().expect("files").insert(norm(path), MemNode::File(data.to_vec(), self.now));
        Ok(())
    }

    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let mut old = self.read(path).unwrap_or_default();
        old.extend_from_slice(data);
        self.write_atomic(path, &old)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        let mut m = self.nodes.lock().expect("files");
        let from = norm(from);
        let to = norm(to);
        let moved: Vec<(String, MemNode)> = m
            .range(from.clone()..)
            .take_while(|(k, _)| **k == from || k.starts_with(&format!("{from}/")))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if moved.is_empty() {
            return Err(not_found(&from));
        }
        for (k, v) in moved {
            m.remove(&k);
            m.insert(format!("{to}{}", &k[from.len()..]), v);
        }
        Ok(())
    }

    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        let mut m = self.nodes.lock().expect("files");
        let mut cur = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            cur.push('/');
            cur.push_str(part);
            match m.get(&cur) {
                Some(MemNode::File(..)) => return Err(io::Error::other(format!("{cur} is a file"))),
                Some(MemNode::Dir) => {}
                None => {
                    m.insert(cur.clone(), MemNode::Dir);
                }
            }
        }
        Ok(())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let mut m = self.nodes.lock().expect("files");
        match m.get(&norm(path)) {
            Some(MemNode::File(..)) => {
                m.remove(&norm(path));
                Ok(())
            }
            Some(MemNode::Dir) => Err(io::Error::other(format!("{path} is a directory"))),
            None => Err(not_found(path)),
        }
    }

    fn remove_tree(&self, path: &str) -> io::Result<()> {
        let p = norm(path);
        let mut m = self.nodes.lock().expect("files");
        if !m.contains_key(&p) {
            return Err(not_found(path));
        }
        let pre = format!("{p}/");
        m.retain(|k, _| *k != p && !k.starts_with(&pre));
        Ok(())
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let d = norm(dir);
        let m = self.nodes.lock().expect("files");
        if !matches!(m.get(&d), Some(MemNode::Dir)) {
            return Err(not_found(dir));
        }
        let pre = if d == "/" { "/".to_string() } else { format!("{d}/") };
        Ok(m.iter()
            .filter(|(k, _)| k.starts_with(&pre) && k.len() > pre.len() && !k[pre.len()..].contains('/'))
            .map(|(k, v)| entry(&k[pre.len()..], v))
            .collect())
    }

    fn stat(&self, path: &str) -> io::Result<Entry> {
        let p = norm(path);
        let m = self.nodes.lock().expect("files");
        let name = p.rsplit('/').next().unwrap_or("");
        m.get(&p).map(|v| entry(name, v)).ok_or_else(|| not_found(path))
    }
}

fn entry(name: &str, n: &MemNode) -> Entry {
    match n {
        MemNode::Dir => Entry {
            name: name.into(),
            dir: true,
            size: 0,
            modified: 0,
        },
        MemNode::File(d, t) => Entry {
            name: name.into(),
            dir: false,
            size: d.len() as u64,
            modified: *t,
        },
    }
}

/// IFS paths under a host directory (the demo server, tests).
#[derive(Clone, Debug)]
pub struct DirFiles {
    root: PathBuf,
}

impl DirFiles {
    /// Paths under `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn host(&self, p: &str) -> io::Result<PathBuf> {
        let mut out = self.root.clone();
        for part in p.split('/').filter(|s| !s.is_empty()) {
            if part == ".." || part == "." {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{p} is not a plain path")));
            }
            out.push(part);
        }
        Ok(out)
    }
}

fn mtime(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

impl Files for DirFiles {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.host(path)?)
    }

    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let to = self.host(path)?;
        let tmp = to.with_extension(format!(
            "{}.tmp",
            to.extension().and_then(|e| e.to_str()).unwrap_or("")
        ));
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &to)
    }

    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        use std::io::Write;
        std::fs::OpenOptions::new().create(true).append(true).open(self.host(path)?)?.write_all(data)
    }

    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(self.host(from)?, self.host(to)?)
    }

    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        std::fs::create_dir_all(self.host(path)?)
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        std::fs::remove_file(self.host(path)?)
    }

    fn remove_tree(&self, path: &str) -> io::Result<()> {
        std::fs::remove_dir_all(self.host(path)?)
    }

    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(self.host(dir)?)? {
            let e = e?;
            let m = e.metadata()?;
            out.push(Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                dir: m.is_dir(),
                size: if m.is_dir() { 0 } else { m.len() },
                modified: mtime(&m),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn stat(&self, path: &str) -> io::Result<Entry> {
        let h = self.host(path)?;
        let m = std::fs::metadata(&h)?;
        Ok(Entry {
            name: path.trim_end_matches('/').rsplit('/').next().unwrap_or("").into(),
            dir: m.is_dir(),
            size: if m.is_dir() { 0 } else { m.len() },
            modified: mtime(&m),
        })
    }

    fn open(&self, path: &str) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(std::fs::File::open(self.host(path)?)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(f: &dyn Files) {
        f.mkdir_all("/www/a/conf").unwrap();
        f.write_atomic("/www/a/conf/httpd.conf", b"one").unwrap();
        f.append("/www/a/conf/httpd.conf", b"+two").unwrap();
        assert_eq!(f.read("/www/a/conf/httpd.conf").unwrap(), b"one+two");
        f.copy("/www/a/conf/httpd.conf", "/www/a/conf/httpd.conf.bak").unwrap();
        let names: Vec<String> = f.list("/www/a/conf").unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["httpd.conf", "httpd.conf.bak"]);
        assert!(f.stat("/www/a").unwrap().dir);
        assert!(f.write_atomic("/www/missing/x", b"").is_err());
        f.rename("/www/a", "/www/b").unwrap();
        assert!(f.exists("/www/b/conf/httpd.conf") && !f.exists("/www/a"));
        let mut s = String::new();
        f.open("/www/b/conf/httpd.conf.bak").unwrap().read_to_string(&mut s).unwrap();
        assert_eq!(s, "one+two");
        f.remove("/www/b/conf/httpd.conf.bak").unwrap();
        f.remove_tree("/www/b").unwrap();
        assert!(!f.exists("/www/b/conf"));
        assert!(f.list("/www").unwrap().is_empty());
    }

    #[test]
    fn memory_and_directory_trees_behave_alike() {
        exercise(&MemFiles::new());
        let dir = std::env::temp_dir().join(format!("httpd-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = DirFiles::new(&dir);
        exercise(&d);
        assert!(d.read("/../x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

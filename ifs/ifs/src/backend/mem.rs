// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An in-memory backend: inodes, hard links, symbolic links, extended
//! attributes, a capacity, and faults to inject. Every test uses it.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{BFile, BStat, Backend, DirEntry, Kind, Nanos, StatFs};
use crate::errno::{Errno, Result};

const ROOT: u64 = 1;
const BLOCK: u64 = 4096;

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    data: Vec<u8>,
    children: BTreeMap<String, u64>,
    target: String,
    nlink: u32,
    open: u32,
    atime: Nanos,
    mtime: Nanos,
    ctime: Nanos,
    mode: u32,
    uid: u32,
    gid: u32,
    xattrs: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Default)]
struct Inner {
    nodes: HashMap<u64, Node>,
    next: u64,
    capacity: Option<u64>,
    fail_xattr_set: bool,
    clock: Option<Nanos>,
}

/// The in-memory tree. Clones share it.
#[derive(Debug, Clone)]
pub struct MemBackend {
    inner: Arc<Mutex<Inner>>,
}

impl Default for MemBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn now(i: &Inner) -> Nanos {
    i.clock.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
    })
}

impl Inner {
    fn node(&self, ino: u64) -> Result<&Node> {
        self.nodes.get(&ino).ok_or(Errno::ESTALE)
    }

    fn node_mut(&mut self, ino: u64) -> Result<&mut Node> {
        self.nodes.get_mut(&ino).ok_or(Errno::ESTALE)
    }

    fn walk(&self, p: &str) -> Result<u64> {
        let mut at = ROOT;
        for c in p.split('/').filter(|c| !c.is_empty()) {
            let n = self.node(at)?;
            match n.kind {
                Kind::Dir => {}
                Kind::Symlink => return Err(Errno::ELOOP),
                _ => return Err(Errno::ENOTDIR),
            }
            at = *n.children.get(c).ok_or(Errno::ENOENT)?;
        }
        Ok(at)
    }

    /// The parent directory's inode and the last name.
    fn parent<'p>(&self, p: &'p str) -> Result<(u64, &'p str)> {
        let name = super::last(p);
        if name.is_empty() {
            return Err(Errno::EEXIST);
        }
        let dir = self.walk(super::parent(p))?;
        if self.node(dir)?.kind != Kind::Dir {
            return Err(Errno::ENOTDIR);
        }
        Ok((dir, name))
    }

    fn used(&self) -> u64 {
        self.nodes
            .values()
            .map(|n| (n.data.len() as u64).div_ceil(BLOCK) * BLOCK)
            .sum()
    }

    fn add(&mut self, dir: u64, name: &str, mut node: Node) -> Result<u64> {
        if self.node(dir)?.children.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        let t = now(self);
        node.atime = t;
        node.mtime = t;
        node.ctime = t;
        self.next += 1;
        let ino = self.next;
        self.nodes.insert(ino, node);
        let d = self.node_mut(dir)?;
        d.children.insert(name.to_string(), ino);
        d.mtime = t;
        d.ctime = t;
        Ok(ino)
    }

    fn drop_link(&mut self, ino: u64) {
        let t = now(self);
        if let Some(n) = self.nodes.get_mut(&ino) {
            n.nlink = n.nlink.saturating_sub(1);
            n.ctime = t;
            if n.nlink == 0 && n.open == 0 {
                self.nodes.remove(&ino);
            }
        }
    }

    fn stat_of(&self, ino: u64) -> Result<BStat> {
        let n = self.node(ino)?;
        let size = match n.kind {
            Kind::File => n.data.len() as u64,
            Kind::Symlink => n.target.len() as u64,
            Kind::Dir => 4096,
            _ => 0,
        };
        Ok(BStat {
            kind: n.kind,
            ino,
            dev: 1,
            nlink: n.nlink,
            size,
            alloc: size.div_ceil(BLOCK) * BLOCK,
            blksize: 4096,
            atime: n.atime,
            mtime: n.mtime,
            ctime: n.ctime,
            mode: n.mode,
            uid: n.uid,
            gid: n.gid,
        })
    }
}

fn blank(kind: Kind, mode: u32) -> Node {
    Node {
        kind,
        data: Vec::new(),
        children: BTreeMap::new(),
        target: String::new(),
        nlink: if kind == Kind::Dir { 2 } else { 1 },
        open: 0,
        atime: 0,
        mtime: 0,
        ctime: 0,
        mode: mode & 0o7777,
        uid: 0,
        gid: 0,
        xattrs: BTreeMap::new(),
    }
}

impl MemBackend {
    /// An empty tree.
    #[must_use]
    pub fn new() -> Self {
        let mut inner = Inner {
            next: ROOT,
            ..Inner::default()
        };
        inner.nodes.insert(ROOT, blank(Kind::Dir, 0o755));
        Self {
            inner: Arc::new(Mutex::new(inner)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Limit stored data to `bytes` (writes past it fail `ENOSPC`).
    pub fn set_capacity(&self, bytes: Option<u64>) {
        self.lock().capacity = bytes;
    }

    /// Make every `xattr_set` fail, as a crash between creating a node
    /// and writing its metadata would leave it.
    pub fn fail_xattr_set(&self, fail: bool) {
        self.lock().fail_xattr_set = fail;
    }

    /// Use a fixed clock (`None`: the system's).
    pub fn set_clock(&self, t: Option<Nanos>) {
        self.lock().clock = t;
    }

    /// Nodes in the tree, for leak checks.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.lock().nodes.len()
    }
}

/// An open file of a [`MemBackend`].
#[derive(Debug)]
pub struct MemFile {
    inner: Arc<Mutex<Inner>>,
    ino: u64,
}

impl MemFile {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for MemFile {
    fn drop(&mut self) {
        let mut i = self.lock();
        if let Some(n) = i.nodes.get_mut(&self.ino) {
            n.open = n.open.saturating_sub(1);
            if n.open == 0 && n.nlink == 0 {
                i.nodes.remove(&self.ino);
            }
        }
    }
}

impl BFile for MemFile {
    fn read_at(&self, buf: &mut [u8], off: u64) -> Result<usize> {
        let mut i = self.lock();
        let t = now(&i);
        let n = i.node_mut(self.ino)?;
        n.atime = t;
        let Ok(off) = usize::try_from(off) else {
            return Ok(0);
        };
        if off >= n.data.len() {
            return Ok(0);
        }
        let len = buf.len().min(n.data.len() - off);
        buf[..len].copy_from_slice(&n.data[off..off + len]);
        Ok(len)
    }

    fn write_at(&self, buf: &[u8], off: u64) -> Result<usize> {
        let mut i = self.lock();
        let off = usize::try_from(off).map_err(|_| Errno::EFBIG)?;
        let end = off + buf.len();
        if let Some(cap) = i.capacity {
            let cur = i.node(self.ino)?.data.len();
            let grow = end.saturating_sub(cur) as u64;
            if i.used() + grow > cap {
                return Err(Errno::ENOSPC);
            }
        }
        let t = now(&i);
        let n = i.node_mut(self.ino)?;
        if n.data.len() < end {
            n.data.resize(end, 0);
        }
        n.data[off..end].copy_from_slice(buf);
        n.mtime = t;
        n.ctime = t;
        Ok(buf.len())
    }

    fn stat(&self) -> Result<BStat> {
        self.lock().stat_of(self.ino)
    }

    fn set_len(&self, len: u64) -> Result<()> {
        let mut i = self.lock();
        let len = usize::try_from(len).map_err(|_| Errno::EFBIG)?;
        let t = now(&i);
        let n = i.node_mut(self.ino)?;
        n.data.resize(len, 0);
        n.mtime = t;
        n.ctime = t;
        Ok(())
    }

    fn sync(&self, _data_only: bool) -> Result<()> {
        Ok(())
    }
}

impl Backend for MemBackend {
    type File = MemFile;

    fn stat(&self, p: &str) -> Result<BStat> {
        let i = self.lock();
        let ino = i.walk(p)?;
        i.stat_of(ino)
    }

    fn readdir(&self, p: &str) -> Result<Vec<DirEntry>> {
        let i = self.lock();
        let n = i.node(i.walk(p)?)?;
        if n.kind != Kind::Dir {
            return Err(Errno::ENOTDIR);
        }
        n.children
            .iter()
            .map(|(name, &ino)| {
                Ok(DirEntry {
                    name: name.clone(),
                    ino,
                    kind: i.node(ino)?.kind,
                })
            })
            .collect()
    }

    fn mkdir(&self, p: &str, mode: u32) -> Result<()> {
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        i.add(dir, name, blank(Kind::Dir, mode))?;
        i.node_mut(dir)?.nlink += 1;
        Ok(())
    }

    fn rmdir(&self, p: &str) -> Result<()> {
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        let ino = i.walk(p)?;
        let node = i.node(ino)?;
        if node.kind != Kind::Dir {
            return Err(Errno::ENOTDIR);
        }
        if !node.children.is_empty() {
            return Err(Errno::ENOTEMPTY);
        }
        let t = now(&i);
        let d = i.node_mut(dir)?;
        d.children.remove(name);
        d.nlink -= 1;
        d.mtime = t;
        d.ctime = t;
        i.nodes.remove(&ino);
        Ok(())
    }

    fn create(&self, p: &str, mode: u32) -> Result<MemFile> {
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        let ino = i.add(dir, name, blank(Kind::File, mode))?;
        i.node_mut(ino)?.open += 1;
        Ok(MemFile {
            inner: self.inner.clone(),
            ino,
        })
    }

    fn open(&self, p: &str, _write: bool) -> Result<MemFile> {
        let mut i = self.lock();
        let ino = i.walk(p)?;
        let n = i.node_mut(ino)?;
        match n.kind {
            Kind::File => {}
            Kind::Dir => return Err(Errno::EISDIR),
            Kind::Symlink => return Err(Errno::ELOOP),
            _ => return Err(Errno::ENOTSUP),
        }
        n.open += 1;
        Ok(MemFile {
            inner: self.inner.clone(),
            ino,
        })
    }

    fn mknod(&self, p: &str, kind: Kind, mode: u32) -> Result<()> {
        if matches!(kind, Kind::File | Kind::Dir | Kind::Symlink) {
            return Err(Errno::EINVAL);
        }
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        i.add(dir, name, blank(kind, mode))?;
        Ok(())
    }

    fn unlink(&self, p: &str) -> Result<()> {
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        let ino = i.walk(p)?;
        if i.node(ino)?.kind == Kind::Dir {
            return Err(Errno::EISDIR);
        }
        let t = now(&i);
        let d = i.node_mut(dir)?;
        d.children.remove(name);
        d.mtime = t;
        d.ctime = t;
        i.drop_link(ino);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str, replace: bool) -> Result<()> {
        let mut i = self.lock();
        let (fdir, fname) = i.parent(from)?;
        let (tdir, tname) = i.parent(to)?;
        let ino = i.walk(from)?;
        let moving_dir = i.node(ino)?.kind == Kind::Dir;
        if moving_dir && (to == from || to.starts_with(&format!("{from}/"))) {
            return Err(Errno::EINVAL);
        }
        if let Ok(old) = i.walk(to) {
            if !replace {
                return Err(Errno::EEXIST);
            }
            if old == ino {
                return Ok(());
            }
            let o = i.node(old)?;
            match (moving_dir, o.kind == Kind::Dir) {
                (true, false) => return Err(Errno::ENOTDIR),
                (false, true) => return Err(Errno::EISDIR),
                (true, true) if !o.children.is_empty() => return Err(Errno::ENOTEMPTY),
                _ => {}
            }
            if o.kind == Kind::Dir {
                i.nodes.remove(&old);
                i.node_mut(tdir)?.nlink -= 1;
            } else {
                i.drop_link(old);
            }
        }
        let t = now(&i);
        i.node_mut(fdir)?.children.remove(fname);
        i.node_mut(tdir)?.children.insert(tname.to_string(), ino);
        if moving_dir && fdir != tdir {
            i.node_mut(fdir)?.nlink -= 1;
            i.node_mut(tdir)?.nlink += 1;
        }
        for d in [fdir, tdir] {
            let n = i.node_mut(d)?;
            n.mtime = t;
            n.ctime = t;
        }
        i.node_mut(ino)?.ctime = t;
        Ok(())
    }

    fn link(&self, existing: &str, new: &str) -> Result<()> {
        let mut i = self.lock();
        let ino = i.walk(existing)?;
        if i.node(ino)?.kind == Kind::Dir {
            return Err(Errno::EPERM);
        }
        let (dir, name) = i.parent(new)?;
        if i.node(dir)?.children.contains_key(name) {
            return Err(Errno::EEXIST);
        }
        let t = now(&i);
        let d = i.node_mut(dir)?;
        d.children.insert(name.to_string(), ino);
        d.mtime = t;
        let n = i.node_mut(ino)?;
        n.nlink += 1;
        n.ctime = t;
        Ok(())
    }

    fn symlink(&self, target: &str, p: &str) -> Result<()> {
        let mut i = self.lock();
        let (dir, name) = i.parent(p)?;
        let mut n = blank(Kind::Symlink, 0o777);
        n.target = target.to_string();
        i.add(dir, name, n)?;
        Ok(())
    }

    fn readlink(&self, p: &str) -> Result<String> {
        let i = self.lock();
        let n = i.node(i.walk(p)?)?;
        if n.kind != Kind::Symlink {
            return Err(Errno::EINVAL);
        }
        Ok(n.target.clone())
    }

    fn set_times(&self, p: &str, atime: Option<Nanos>, mtime: Option<Nanos>) -> Result<()> {
        let mut i = self.lock();
        let ino = i.walk(p)?;
        let t = now(&i);
        let n = i.node_mut(ino)?;
        if let Some(a) = atime {
            n.atime = a;
        }
        if let Some(m) = mtime {
            n.mtime = m;
        }
        n.ctime = t;
        Ok(())
    }

    fn set_mode(&self, p: &str, mode: u32) -> Result<()> {
        let mut i = self.lock();
        let ino = i.walk(p)?;
        i.node_mut(ino)?.mode = mode & 0o7777;
        Ok(())
    }

    fn set_owner(&self, p: &str, uid: u32, gid: u32) -> Result<()> {
        let mut i = self.lock();
        let ino = i.walk(p)?;
        let n = i.node_mut(ino)?;
        n.uid = uid;
        n.gid = gid;
        Ok(())
    }

    fn xattr_get(&self, p: &str, name: &str) -> Result<Option<Vec<u8>>> {
        let i = self.lock();
        Ok(i.node(i.walk(p)?)?.xattrs.get(name).cloned())
    }

    fn xattr_set(&self, p: &str, name: &str, value: &[u8]) -> Result<()> {
        let mut i = self.lock();
        if i.fail_xattr_set {
            return Err(Errno::EIO);
        }
        let ino = i.walk(p)?;
        i.node_mut(ino)?
            .xattrs
            .insert(name.to_string(), value.to_vec());
        Ok(())
    }

    fn xattr_list(&self, p: &str) -> Result<Vec<String>> {
        let i = self.lock();
        Ok(i.node(i.walk(p)?)?.xattrs.keys().cloned().collect())
    }

    fn xattr_remove(&self, p: &str, name: &str) -> Result<()> {
        let mut i = self.lock();
        let ino = i.walk(p)?;
        i.node_mut(ino)?.xattrs.remove(name);
        Ok(())
    }

    fn statfs(&self) -> Result<StatFs> {
        let i = self.lock();
        let total = i.capacity.unwrap_or(1 << 34);
        let free = total.saturating_sub(i.used()) / BLOCK;
        Ok(StatFs {
            block_size: BLOCK,
            blocks: total / BLOCK,
            bfree: free,
            bavail: free,
            files: 1 << 20,
            ffree: (1u64 << 20).saturating_sub(i.nodes.len() as u64),
            name_max: 255,
            fsid: 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_operations() {
        let b = MemBackend::new();
        b.mkdir("root", 0o755).unwrap();
        let f = b.create("root/a", 0o644).unwrap();
        f.write_at(b"hello", 0).unwrap();
        assert_eq!(b.stat("root/a").unwrap().size, 5);
        b.link("root/a", "root/b").unwrap();
        assert_eq!(b.stat("root/b").unwrap().nlink, 2);
        b.unlink("root/a").unwrap();
        let mut buf = [0u8; 5];
        assert_eq!(f.read_at(&mut buf, 0).unwrap(), 5);
        b.unlink("root/b").unwrap();
        // Still open: the inode lives until the file is dropped.
        assert_eq!(f.stat().unwrap().nlink, 0);
        let before = b.node_count();
        drop(f);
        assert_eq!(b.node_count(), before - 1);
        b.symlink("../x", "root/l").unwrap();
        assert_eq!(b.readlink("root/l").unwrap(), "../x");
        assert_eq!(b.stat("root/l/x"), Err(Errno::ELOOP));
        assert_eq!(b.rmdir("root"), Err(Errno::ENOTEMPTY));
        b.mkdir("root/d", 0o755).unwrap();
        assert_eq!(b.rename("root", "root/d/x", true), Err(Errno::EINVAL));
        b.rename("root/d", "root/e", false).unwrap();
        assert_eq!(b.stat("root").unwrap().nlink, 3);
        b.xattr_set("root/e", "k", b"v").unwrap();
        assert_eq!(b.xattr_get("root/e", "k").unwrap(), Some(b"v".to_vec()));
        assert_eq!(b.xattr_list("root/e").unwrap(), vec!["k".to_string()]);
    }

    #[test]
    fn capacity() {
        let b = MemBackend::new();
        b.set_capacity(Some(8192));
        let f = b.create("f", 0o644).unwrap();
        f.write_at(&[0; 8000], 0).unwrap();
        assert_eq!(f.write_at(&[0; 9000], 0), Err(Errno::ENOSPC));
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage under root, `/QOpenSys` and user-defined file systems: a
//! real directory tree. Paths here are relative to the backend's top
//! (`""` is the top, `root/home/a.txt` a file), name exact components
//! (no folding), and never follow a symbolic link: the resolver has
//! already resolved them. [`mem::MemBackend`] keeps the tree in memory
//! for tests; [`linux::LinuxBackend`] is a directory on xfs.

pub mod mem;

#[cfg(target_os = "linux")]
pub mod linux;

use crate::errno::Result;

/// Nanoseconds since the epoch.
pub type Nanos = i64;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A stream file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link.
    Symlink,
    /// A FIFO.
    Fifo,
    /// A local socket.
    Socket,
    /// A character special file.
    CharDev,
}

/// A node's attributes as the backend keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BStat {
    /// What it is.
    pub kind: Kind,
    /// Inode number.
    pub ino: u64,
    /// Device of the backend.
    pub dev: u64,
    /// Hard links.
    pub nlink: u32,
    /// Data size in bytes.
    pub size: u64,
    /// Allocated bytes.
    pub alloc: u64,
    /// Preferred I/O size.
    pub blksize: u32,
    /// Last access.
    pub atime: Nanos,
    /// Last data change.
    pub mtime: Nanos,
    /// Last status change.
    pub ctime: Nanos,
    /// Permission bits (a mirror only; QS/400 never trusts them).
    pub mode: u32,
    /// Owner UID (mirror).
    pub uid: u32,
    /// Group GID (mirror).
    pub gid: u32,
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    /// The stored name.
    pub name: String,
    /// Inode number.
    pub ino: u64,
    /// What it is.
    pub kind: Kind,
}

/// File system capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatFs {
    /// Block size.
    pub block_size: u64,
    /// Total blocks.
    pub blocks: u64,
    /// Free blocks.
    pub bfree: u64,
    /// Free blocks an unprivileged user may use.
    pub bavail: u64,
    /// Total inodes.
    pub files: u64,
    /// Free inodes.
    pub ffree: u64,
    /// Longest name.
    pub name_max: u64,
    /// File system identifier.
    pub fsid: u64,
}

/// An open stream file. Positions are the caller's: every call names
/// its offset.
pub trait BFile: Send + Sync {
    /// Read at `off`; 0 at end of file.
    fn read_at(&self, buf: &mut [u8], off: u64) -> Result<usize>;
    /// Write at `off`.
    fn write_at(&self, buf: &[u8], off: u64) -> Result<usize>;
    /// Current attributes.
    fn stat(&self) -> Result<BStat>;
    /// Set the size.
    fn set_len(&self, len: u64) -> Result<()>;
    /// Force data (and with `!data_only` the attributes) to disk.
    fn sync(&self, data_only: bool) -> Result<()>;
}

/// The directory tree.
pub trait Backend: Send + Sync + 'static {
    /// An open stream file.
    type File: BFile;

    /// Attributes of a node (not following a symbolic link).
    fn stat(&self, p: &str) -> Result<BStat>;
    /// A directory's entries, without `.` and `..`.
    fn readdir(&self, p: &str) -> Result<Vec<DirEntry>>;
    /// Make a directory.
    fn mkdir(&self, p: &str, mode: u32) -> Result<()>;
    /// Remove an empty directory.
    fn rmdir(&self, p: &str) -> Result<()>;
    /// Create a stream file that must not exist, and open it.
    fn create(&self, p: &str, mode: u32) -> Result<Self::File>;
    /// Open an existing stream file.
    fn open(&self, p: &str, write: bool) -> Result<Self::File>;
    /// Make a FIFO, socket or character special node.
    fn mknod(&self, p: &str, kind: Kind, mode: u32) -> Result<()>;
    /// Remove a link to a non-directory.
    fn unlink(&self, p: &str) -> Result<()>;
    /// Rename; `replace` false fails with `EEXIST` when `to` exists.
    fn rename(&self, from: &str, to: &str, replace: bool) -> Result<()>;
    /// A new hard link `new` to `existing`.
    fn link(&self, existing: &str, new: &str) -> Result<()>;
    /// A symbolic link at `p` holding `target`.
    fn symlink(&self, target: &str, p: &str) -> Result<()>;
    /// A symbolic link's text.
    fn readlink(&self, p: &str) -> Result<String>;
    /// Set access and modification times (`None` leaves one alone).
    fn set_times(&self, p: &str, atime: Option<Nanos>, mtime: Option<Nanos>) -> Result<()>;
    /// Mirror the permission bits.
    fn set_mode(&self, p: &str, mode: u32) -> Result<()>;
    /// Mirror the owner and group.
    fn set_owner(&self, p: &str, uid: u32, gid: u32) -> Result<()>;
    /// An extended attribute, `None` when absent.
    fn xattr_get(&self, p: &str, name: &str) -> Result<Option<Vec<u8>>>;
    /// Set an extended attribute.
    fn xattr_set(&self, p: &str, name: &str, value: &[u8]) -> Result<()>;
    /// Extended attribute names.
    fn xattr_list(&self, p: &str) -> Result<Vec<String>>;
    /// Remove an extended attribute (absent is not an error).
    fn xattr_remove(&self, p: &str, name: &str) -> Result<()>;
    /// Capacity.
    fn statfs(&self) -> Result<StatFs>;
}

/// `a/b` joined, either side possibly empty.
#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    match (dir.is_empty(), name.is_empty()) {
        (true, _) => name.to_string(),
        (_, true) => dir.to_string(),
        _ => format!("{dir}/{name}"),
    }
}

/// The parent of a backend path (`""` for a top-level name).
#[must_use]
pub fn parent(p: &str) -> &str {
    p.rfind('/').map_or("", |i| &p[..i])
}

/// The last component of a backend path.
#[must_use]
pub fn last(p: &str) -> &str {
    p.rfind('/').map_or(p, |i| &p[i + 1..])
}

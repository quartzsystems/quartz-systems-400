// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The trait a remote file system implements, and the factory that
//! mounts one. Every call blocks (on the network) and is made only
//! from a mount's worker threads, never from the thread that calls the
//! [`crate::Ifs`]. Names are bytes as the server holds them: the IFS
//! converts them with the mount's path name CCSID.

use super::types::{
    CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, MountCcsid, OpenToken,
    PathConf, RAttr, RHandle, RKind, RResult, RemoteInfo, SetAttr, Share, Stable, WriteReply,
};

/// A mounted remote file system.
#[allow(clippy::too_many_arguments)]
pub trait RemoteFs: Send + Sync {
    /// The mounted directory.
    fn root(&self) -> RResult<(RHandle, RAttr)>;
    /// An object's attributes.
    fn getattr(&self, cred: &Cred, h: &RHandle) -> RResult<RAttr>;
    /// Change attributes; with `guard`, only while the object's ctime
    /// is still that (else `EAGAIN`, nothing changed). The new ones.
    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr>;
    /// A name in a directory.
    fn lookup(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)>;
    /// Which of `mask`'s [`access`] bits the caller has.
    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32>;
    /// A symbolic link's text.
    fn readlink(&self, cred: &Cred, h: &RHandle) -> RResult<Vec<u8>>;
    /// Up to `len` bytes at `off`, and whether that reached the end.
    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)>;
    /// Write `data` at `off`.
    fn write(
        &self,
        cred: &Cred,
        h: &RHandle,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> RResult<WriteReply>;
    /// Make unstable writes in `[off, off + len)` safe (`len` 0: to the
    /// end); the write verifier.
    fn commit(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<u64>;
    /// A regular file.
    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)>;
    /// A directory.
    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)>;
    /// A symbolic link holding `target`.
    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        target: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)>;
    /// A special node; only [`RKind::Fifo`] need be supported (others
    /// `ENOTSUP`).
    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)>;
    /// Remove a name that is not a directory.
    fn remove(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()>;
    /// Remove an empty directory.
    fn rmdir(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()>;
    /// Rename, replacing what `to` names.
    fn rename(
        &self,
        cred: &Cred,
        from_dir: &RHandle,
        from: &[u8],
        to_dir: &RHandle,
        to: &[u8],
    ) -> RResult<()>;
    /// A new hard link to `h`.
    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, name: &[u8]) -> RResult<()>;
    /// A page of entries after `cookie` (0: the first), with the
    /// verifier of the page before.
    fn readdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        verifier: u64,
        max: u32,
    ) -> RResult<DirPage>;
    /// Capacity.
    fn statfs(&self, cred: &Cred, h: &RHandle) -> RResult<FsStat>;
    /// Limits.
    fn pathconf(&self, cred: &Cred, h: &RHandle) -> RResult<PathConf>;
    /// Begin an open (a stateful protocol's share reservation). A
    /// stateless one returns an empty token.
    fn open(&self, cred: &Cred, h: &RHandle, share: Share, owner: LockOwner) -> RResult<OpenToken>;
    /// End an open.
    fn close(&self, cred: &Cred, h: &RHandle, token: &OpenToken) -> RResult<()>;
    /// Take a byte-range lock without waiting: `EAGAIN` when another
    /// holds it.
    fn lock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        token: &OpenToken,
    ) -> RResult<()>;
    /// Release a range.
    fn unlock(&self, cred: &Cred, h: &RHandle, owner: LockOwner, off: u64, len: u64)
    -> RResult<()>;
    /// The lock that would stand in the way.
    fn test(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>>;
    /// The mount ends (tell the server, close connections).
    fn unmount(&self) -> RResult<()>;
}

/// The bits of [`RemoteFs::access`].
pub mod access {
    /// Read data, list a directory.
    pub const READ: u32 = 0x01;
    /// Look a name up in a directory.
    pub const LOOKUP: u32 = 0x02;
    /// Change data, or a directory's names.
    pub const MODIFY: u32 = 0x04;
    /// Add data, or names.
    pub const EXTEND: u32 = 0x08;
    /// Remove names.
    pub const DELETE: u32 = 0x10;
    /// Run a file.
    pub const EXECUTE: u32 = 0x20;
}

/// A remote file system and what it says about itself.
pub struct Mounted {
    /// The file system.
    pub fs: Box<dyn RemoteFs>,
    /// What the IFS needs to know.
    pub info: RemoteInfo,
}

/// What the storage service registers on the IFS to mount remote file
/// systems. `mount` blocks on the network too and runs on a worker
/// thread; retrying is the factory's business.
pub trait RemoteFactory: Send + Sync {
    /// Whether `kind` (`*NFS` ...) is one this factory mounts.
    fn supports(&self, kind: &str) -> bool;
    /// Mount `mfs` (`host:/path`) with the `OPTIONS` string as given,
    /// the CCSIDs already resolved, as `cred`.
    fn mount(
        &self,
        kind: &str,
        mfs: &str,
        options: &str,
        ccsid: MountCcsid,
        cred: &Cred,
    ) -> super::types::RResult<Mounted>;
}

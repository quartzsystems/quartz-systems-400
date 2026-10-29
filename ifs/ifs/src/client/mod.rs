// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a job calls: [`IfsClient`], one method per operation, with the
//! descriptors, directory streams and errors the service hands out.
//! The runtime's APIs and QSH depend on this trait alone; the storage
//! service answers it over the job's link (`stg::remote::RemoteIfs`),
//! [`LocalIfs`] answers it in the process over an [`Ifs`] for tests,
//! and [`Direct`] is the adapter both the service and `LocalIfs` use.
//! Data moves in chunks of at most [`CHUNK`] bytes per call. The wire
//! codecs of the types are in [`wire`].

pub mod wire;

use std::sync::Arc;

use sec::auth::principal::Principal;

use crate::auth::AuthEnv;
use crate::backend::{Backend, Nanos};
use crate::errno::Result;
use crate::lock::range::{RangeKind, RangeLock};
use crate::lock::{LockInfo, ObjKey, Owner};
use crate::meta::reclaim::Report;
use crate::meta::{CrtObjAud, Journal};
use crate::ops::audit::{Audit, FileId};
use crate::ops::{
    Attr, AttrId, AuthList, ChgAut, CopyOpts, Ctx, DirEnt, Ifs, OpenArgs, RenameMode, Stat,
    StatVfs, UdfsInfo, Visit, WalkOpts,
};

/// Most bytes one read or write moves.
pub const CHUNK: usize = 1 << 20;

/// One object a walk visited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Its path.
    pub path: String,
    /// Its attributes, or why they could not be read.
    pub stat: Result<Stat>,
}

/// The IFS as a job sees it. Descriptors are the service's numbers;
/// `read` and `pread` answer at most `max` bytes (and never more than
/// [`CHUNK`]), an empty answer being end of file; `readdir` answers up
/// to `max` entries, an empty answer being the end of the stream.
#[allow(clippy::missing_errors_doc, clippy::too_many_arguments)]
pub trait IfsClient {
    /// `open()`.
    fn open(&self, path: &str, args: OpenArgs) -> Result<i32>;
    /// `openat()`; `None` is `AT_FDCWD`.
    fn openat(&self, dirfd: Option<i32>, path: &str, args: OpenArgs) -> Result<i32>;
    /// `close()`.
    fn close(&self, fd: i32) -> Result<()>;
    /// `read()`: up to `max` bytes.
    fn read(&self, fd: i32, max: usize) -> Result<Vec<u8>>;
    /// `pread()`.
    fn pread(&self, fd: i32, max: usize, off: u64) -> Result<Vec<u8>>;
    /// `write()`: how many bytes were taken.
    fn write(&self, fd: i32, data: &[u8]) -> Result<usize>;
    /// `pwrite()`.
    fn pwrite(&self, fd: i32, data: &[u8], off: u64) -> Result<usize>;
    /// `lseek()`: the new position.
    fn lseek(&self, fd: i32, off: i64, whence: i32) -> Result<u64>;
    /// `fsync()` (`fdatasync()` with `data_only`).
    fn fsync(&self, fd: i32, data_only: bool) -> Result<()>;
    /// `ftruncate()`.
    fn ftruncate(&self, fd: i32, len: u64) -> Result<()>;
    /// `fstat()`.
    fn fstat(&self, fd: i32) -> Result<Stat>;
    /// `stat()` with `follow`, else `lstat()`.
    fn stat(&self, path: &str, follow: bool) -> Result<Stat>;
    /// `access()`.
    fn access(&self, path: &str, amode: i32) -> Result<()>;
    /// `chmod()`.
    fn chmod(&self, path: &str, mode: u32) -> Result<()>;
    /// `chown()`.
    fn chown(&self, path: &str, uid: u32, gid: u32) -> Result<()>;
    /// `link()`.
    fn link(&self, existing: &str, new: &str) -> Result<()>;
    /// `unlink()`.
    fn unlink(&self, path: &str) -> Result<()>;
    /// `rename()`, `Qp0lRenameKeep`, `Qp0lRenameUnlink`.
    fn rename(&self, old: &str, new: &str, mode: RenameMode) -> Result<()>;
    /// `symlink()`.
    fn symlink(&self, target: &str, path: &str) -> Result<()>;
    /// `readlink()`.
    fn readlink(&self, path: &str) -> Result<String>;
    /// `mkdir()`.
    fn mkdir(&self, path: &str, mode: u32) -> Result<()>;
    /// `mkfifo()`.
    fn mkfifo(&self, path: &str, mode: u32) -> Result<()>;
    /// `rmdir()`.
    fn rmdir(&self, path: &str) -> Result<()>;
    /// `opendir()`: a stream handle.
    fn opendir(&self, path: &str) -> Result<u64>;
    /// `readdir()`: up to `max` entries.
    fn readdir(&self, handle: u64, max: usize) -> Result<Vec<DirEnt>>;
    /// `rewinddir()`.
    fn rewinddir(&self, handle: u64) -> Result<()>;
    /// `closedir()`.
    fn closedir(&self, handle: u64) -> Result<()>;
    /// Every entry of a directory but `.` and `..`.
    fn read_dir(&self, path: &str) -> Result<Vec<DirEnt>>;
    /// `chdir()`.
    fn chdir(&self, path: &str) -> Result<()>;
    /// `fchdir()`.
    fn fchdir(&self, fd: i32) -> Result<()>;
    /// `getcwd()`.
    fn getcwd(&self) -> Result<String>;
    /// `dup()`.
    fn dup(&self, fd: i32) -> Result<i32>;
    /// `dup2()`.
    fn dup2(&self, fd: i32, new: i32) -> Result<i32>;
    /// `fcntl()` other than the locks.
    fn fcntl(&self, fd: i32, cmd: i32, arg: i32) -> Result<i32>;
    /// `fcntl()` `F_GETLK`, `F_SETLK`, `F_SETLKW`: the blocking lock for
    /// `F_GETLK`; `F_SETLKW` answers `EAGAIN` while it would wait.
    fn lock_range(
        &self,
        fd: i32,
        cmd: i32,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<RangeLock>>;
    /// The path a descriptor was opened by.
    fn fd_path(&self, fd: i32) -> Result<String>;
    /// `utime()`; `None` sets both times to now.
    fn utime(&self, path: &str, times: Option<(Nanos, Nanos)>) -> Result<()>;
    /// `umask()`: the old mask.
    fn umask(&self, mask: u32) -> Result<u32>;
    /// `pathconf()`: `None` where there is no limit.
    fn pathconf(&self, path: &str, name: i32) -> Result<Option<i64>>;
    /// `statvfs()`.
    fn statvfs(&self, path: &str) -> Result<StatVfs>;
    /// `Qp0lGetAttr` (every attribute with `ids` empty).
    fn get_attrs(&self, path: &str, ids: &[AttrId], follow: bool) -> Result<Vec<Attr>>;
    /// `Qp0lSetAttr` for one attribute.
    fn set_attr(&self, path: &str, attr: &Attr, follow: bool) -> Result<()>;
    /// `DSPAUT`.
    fn dspaut(&self, path: &str) -> Result<AuthList>;
    /// `CHGAUT`.
    fn chgaut(&self, path: &str, c: &ChgAut) -> Result<()>;
    /// `RVKAUT`-style: remove a user's private authority.
    fn rvkaut(&self, path: &str, user: &str) -> Result<()>;
    /// `CHGOWN`.
    fn chgown(&self, path: &str, new_owner: &str, revoke_old: bool) -> Result<()>;
    /// `CHGPGP`; `None` is `*NONE`.
    fn chgpgp(&self, path: &str, new_group: Option<&str>, revoke_old: bool) -> Result<()>;
    /// `CHKOUT`.
    fn check_out(&self, path: &str) -> Result<()>;
    /// `CHKIN`.
    fn check_in(&self, path: &str) -> Result<()>;
    /// `STRJRN` on an object (stored only).
    fn start_journal(&self, path: &str, j: Journal) -> Result<()>;
    /// `ENDJRN`.
    fn end_journal(&self, path: &str) -> Result<()>;
    /// `CPY`.
    fn copy(&self, from: &str, to: &str, opts: CopyOpts) -> Result<()>;
    /// `MOV`.
    fn mov(&self, from: &str, to: &str, replace: bool) -> Result<()>;
    /// Remove a directory and everything in it.
    fn remove_tree(&self, path: &str) -> Result<()>;
    /// `Qp0lProcessSubtree` as a listing: at most `max` objects,
    /// parents before children, in name order.
    fn walk(&self, path: &str, opts: WalkOpts, max: usize) -> Result<Vec<WalkEntry>>;
    /// `Qp0lGetPathFromFileID`.
    fn path_from_file_id(&self, id: FileId) -> Result<String>;
    /// `CRTUDFS`.
    fn crtudfs(
        &self,
        udfs: &str,
        case_sensitive: bool,
        crtobjaud: CrtObjAud,
        text: &str,
    ) -> Result<()>;
    /// `DLTUDFS`.
    fn dltudfs(&self, udfs: &str) -> Result<()>;
    /// `MOUNT`: the file system id.
    fn mount(&self, udfs: &str, over: &str, read_only: bool) -> Result<u32>;
    /// `UNMOUNT`.
    fn unmount(&self, path: &str) -> Result<()>;
    /// `DSPUDFS`.
    fn dspudfs(&self, udfs: &str) -> Result<UdfsInfo>;
    /// `RCLLNK`.
    fn rcllnk(&self, check_only: bool) -> Result<Report>;
    /// The locks on an object (`WRKOBJLCK`).
    fn object_locks(&self, path: &str) -> Result<Vec<LockInfo>>;
    /// Everything the caller's job holds.
    fn owner_locks(&self) -> Result<Vec<(ObjKey, LockInfo)>>;
}

/// An [`Ifs`] with a caller's context: what the service and
/// [`LocalIfs`] answer [`IfsClient`] with.
pub struct Direct<'a, B: Backend> {
    /// The file system.
    pub ifs: &'a Ifs<B>,
    /// The caller.
    pub cx: &'a Ctx<'a>,
}

impl<B: Backend> IfsClient for Direct<'_, B> {
    fn open(&self, path: &str, args: OpenArgs) -> Result<i32> {
        self.ifs.open(self.cx, path, args)
    }
    fn openat(&self, dirfd: Option<i32>, path: &str, args: OpenArgs) -> Result<i32> {
        self.ifs.openat(self.cx, dirfd, path, args)
    }
    fn close(&self, fd: i32) -> Result<()> {
        self.ifs.close(self.cx, fd)
    }
    fn read(&self, fd: i32, max: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max.min(CHUNK)];
        let n = self.ifs.read(self.cx, fd, &mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }
    fn pread(&self, fd: i32, max: usize, off: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; max.min(CHUNK)];
        let n = self.ifs.pread(self.cx, fd, &mut buf, off)?;
        buf.truncate(n);
        Ok(buf)
    }
    fn write(&self, fd: i32, data: &[u8]) -> Result<usize> {
        self.ifs.write(self.cx, fd, &data[..data.len().min(CHUNK)])
    }
    fn pwrite(&self, fd: i32, data: &[u8], off: u64) -> Result<usize> {
        self.ifs
            .pwrite(self.cx, fd, &data[..data.len().min(CHUNK)], off)
    }
    fn lseek(&self, fd: i32, off: i64, whence: i32) -> Result<u64> {
        self.ifs.lseek(self.cx, fd, off, whence)
    }
    fn fsync(&self, fd: i32, data_only: bool) -> Result<()> {
        self.ifs.fsync(self.cx, fd, data_only)
    }
    fn ftruncate(&self, fd: i32, len: u64) -> Result<()> {
        self.ifs.ftruncate(self.cx, fd, len)
    }
    fn fstat(&self, fd: i32) -> Result<Stat> {
        self.ifs.fstat(self.cx, fd)
    }
    fn stat(&self, path: &str, follow: bool) -> Result<Stat> {
        if follow {
            self.ifs.stat(self.cx, path)
        } else {
            self.ifs.lstat(self.cx, path)
        }
    }
    fn access(&self, path: &str, amode: i32) -> Result<()> {
        self.ifs.access(self.cx, path, amode)
    }
    fn chmod(&self, path: &str, mode: u32) -> Result<()> {
        self.ifs.chmod(self.cx, path, mode)
    }
    fn chown(&self, path: &str, uid: u32, gid: u32) -> Result<()> {
        self.ifs.chown(self.cx, path, uid, gid)
    }
    fn link(&self, existing: &str, new: &str) -> Result<()> {
        self.ifs.link(self.cx, existing, new)
    }
    fn unlink(&self, path: &str) -> Result<()> {
        self.ifs.unlink(self.cx, path)
    }
    fn rename(&self, old: &str, new: &str, mode: RenameMode) -> Result<()> {
        self.ifs.rename(self.cx, old, new, mode)
    }
    fn symlink(&self, target: &str, path: &str) -> Result<()> {
        self.ifs.symlink(self.cx, target, path)
    }
    fn readlink(&self, path: &str) -> Result<String> {
        self.ifs.readlink(self.cx, path)
    }
    fn mkdir(&self, path: &str, mode: u32) -> Result<()> {
        self.ifs.mkdir(self.cx, path, mode)
    }
    fn mkfifo(&self, path: &str, mode: u32) -> Result<()> {
        self.ifs.mkfifo(self.cx, path, mode)
    }
    fn rmdir(&self, path: &str) -> Result<()> {
        self.ifs.rmdir(self.cx, path)
    }
    fn opendir(&self, path: &str) -> Result<u64> {
        self.ifs.opendir(self.cx, path)
    }
    fn readdir(&self, handle: u64, max: usize) -> Result<Vec<DirEnt>> {
        let mut v = Vec::new();
        while v.len() < max {
            match self.ifs.readdir(self.cx, handle)? {
                Some(e) => v.push(e),
                None => break,
            }
        }
        Ok(v)
    }
    fn rewinddir(&self, handle: u64) -> Result<()> {
        self.ifs.rewinddir(self.cx, handle)
    }
    fn closedir(&self, handle: u64) -> Result<()> {
        self.ifs.closedir(self.cx, handle)
    }
    fn read_dir(&self, path: &str) -> Result<Vec<DirEnt>> {
        self.ifs.read_dir(self.cx, path)
    }
    fn chdir(&self, path: &str) -> Result<()> {
        self.ifs.chdir(self.cx, path)
    }
    fn fchdir(&self, fd: i32) -> Result<()> {
        self.ifs.fchdir(self.cx, fd)
    }
    fn getcwd(&self) -> Result<String> {
        Ok(self.ifs.getcwd(self.cx))
    }
    fn dup(&self, fd: i32) -> Result<i32> {
        self.ifs.dup(self.cx, fd)
    }
    fn dup2(&self, fd: i32, new: i32) -> Result<i32> {
        self.ifs.dup2(self.cx, fd, new)
    }
    fn fcntl(&self, fd: i32, cmd: i32, arg: i32) -> Result<i32> {
        self.ifs.fcntl(self.cx, fd, cmd, arg)
    }
    fn lock_range(
        &self,
        fd: i32,
        cmd: i32,
        kind: RangeKind,
        whence: i32,
        start: i64,
        len: i64,
    ) -> Result<Option<RangeLock>> {
        self.ifs
            .lock_range(self.cx, fd, cmd, kind, whence, start, len)
    }
    fn fd_path(&self, fd: i32) -> Result<String> {
        self.ifs.fd_path(self.cx, fd)
    }
    fn utime(&self, path: &str, times: Option<(Nanos, Nanos)>) -> Result<()> {
        self.ifs.utime(self.cx, path, times)
    }
    fn umask(&self, mask: u32) -> Result<u32> {
        Ok(self.ifs.umask(self.cx, mask))
    }
    fn pathconf(&self, path: &str, name: i32) -> Result<Option<i64>> {
        self.ifs.pathconf(self.cx, path, name)
    }
    fn statvfs(&self, path: &str) -> Result<StatVfs> {
        self.ifs.statvfs(self.cx, path)
    }
    fn get_attrs(&self, path: &str, ids: &[AttrId], follow: bool) -> Result<Vec<Attr>> {
        self.ifs.get_attrs(self.cx, path, ids, follow)
    }
    fn set_attr(&self, path: &str, attr: &Attr, follow: bool) -> Result<()> {
        self.ifs.set_attr(self.cx, path, attr, follow)
    }
    fn dspaut(&self, path: &str) -> Result<AuthList> {
        self.ifs.dspaut(self.cx, path)
    }
    fn chgaut(&self, path: &str, c: &ChgAut) -> Result<()> {
        self.ifs.chgaut(self.cx, path, c)
    }
    fn rvkaut(&self, path: &str, user: &str) -> Result<()> {
        self.ifs.rvkaut(self.cx, path, user)
    }
    fn chgown(&self, path: &str, new_owner: &str, revoke_old: bool) -> Result<()> {
        self.ifs.chgown(self.cx, path, new_owner, revoke_old)
    }
    fn chgpgp(&self, path: &str, new_group: Option<&str>, revoke_old: bool) -> Result<()> {
        self.ifs.chgpgp(self.cx, path, new_group, revoke_old)
    }
    fn check_out(&self, path: &str) -> Result<()> {
        self.ifs.check_out(self.cx, path)
    }
    fn check_in(&self, path: &str) -> Result<()> {
        self.ifs.check_in(self.cx, path)
    }
    fn start_journal(&self, path: &str, j: Journal) -> Result<()> {
        self.ifs.start_journal(self.cx, path, j)
    }
    fn end_journal(&self, path: &str) -> Result<()> {
        self.ifs.end_journal(self.cx, path)
    }
    fn copy(&self, from: &str, to: &str, opts: CopyOpts) -> Result<()> {
        self.ifs.copy(self.cx, from, to, opts)
    }
    fn mov(&self, from: &str, to: &str, replace: bool) -> Result<()> {
        self.ifs.mov(self.cx, from, to, replace)
    }
    fn remove_tree(&self, path: &str) -> Result<()> {
        self.ifs.remove_tree(self.cx, path)
    }
    fn walk(&self, path: &str, opts: WalkOpts, max: usize) -> Result<Vec<WalkEntry>> {
        let mut out = Vec::new();
        self.ifs.walk(self.cx, path, opts, &mut |p, st| {
            out.push(WalkEntry {
                path: p.to_string(),
                stat: st.cloned(),
            });
            if out.len() >= max {
                Visit::Stop
            } else {
                Visit::Continue
            }
        })?;
        Ok(out)
    }
    fn path_from_file_id(&self, id: FileId) -> Result<String> {
        self.ifs.path_from_file_id(self.cx, id)
    }
    fn crtudfs(
        &self,
        udfs: &str,
        case_sensitive: bool,
        crtobjaud: CrtObjAud,
        text: &str,
    ) -> Result<()> {
        self.ifs
            .crtudfs(self.cx, udfs, case_sensitive, crtobjaud, text)
    }
    fn dltudfs(&self, udfs: &str) -> Result<()> {
        self.ifs.dltudfs(self.cx, udfs)
    }
    fn mount(&self, udfs: &str, over: &str, read_only: bool) -> Result<u32> {
        self.ifs.mount(self.cx, udfs, over, read_only)
    }
    fn unmount(&self, path: &str) -> Result<()> {
        self.ifs.unmount(self.cx, path)
    }
    fn dspudfs(&self, udfs: &str) -> Result<UdfsInfo> {
        self.ifs.dspudfs(self.cx, udfs)
    }
    fn rcllnk(&self, check_only: bool) -> Result<Report> {
        self.ifs.rcllnk(self.cx, check_only)
    }
    fn object_locks(&self, path: &str) -> Result<Vec<LockInfo>> {
        self.ifs.object_locks(self.cx, path)
    }
    fn owner_locks(&self) -> Result<Vec<(ObjKey, LockInfo)>> {
        Ok(self.ifs.owner_locks(self.cx.owner))
    }
}

/// An [`IfsClient`] in the process: one caller over a shared [`Ifs`],
/// for tests and for tools that run in the service.
pub struct LocalIfs<B: Backend> {
    /// The file system, shared by every caller.
    pub ifs: Arc<Ifs<B>>,
    /// Profiles and authorization lists.
    pub env: Arc<dyn AuthEnv>,
    /// Where audit events go.
    pub audit: Arc<dyn Audit>,
    /// Who the caller is.
    pub principal: Principal,
    /// Whose descriptors and locks.
    pub owner: Owner,
    /// The job's CCSID.
    pub job_ccsid: u16,
}

impl<B: Backend> LocalIfs<B> {
    /// Run `f` with the caller's context.
    pub fn with<T>(&self, f: impl FnOnce(&dyn IfsClient) -> T) -> T {
        let cx = Ctx {
            owner: self.owner,
            principal: &self.principal,
            env: &*self.env,
            audit: &*self.audit,
            job_ccsid: self.job_ccsid,
        };
        f(&Direct {
            ifs: &self.ifs,
            cx: &cx,
        })
    }

    /// End the caller's use: its descriptors, streams and locks go.
    pub fn release(&self) {
        self.ifs.release_owner(self.owner);
    }
}

macro_rules! forward {
    ($( fn $name:ident ( $($a:ident : $t:ty),* ) -> $r:ty; )*) => {
        $( fn $name(&self, $($a: $t),*) -> $r { self.with(|d| d.$name($($a),*)) } )*
    };
}

macro_rules! refuse {
    ($( fn $name:ident ( $($a:ident : $t:ty),* ) -> $r:ty; )*) => {
        $( fn $name(&self, $(_: $t),*) -> $r { Err(self.0) } )*
    };
}

/// Every [`IfsClient`] method's signature, handed to `$m`.
macro_rules! every_method {
    ($m:ident) => { $m! {
        fn open(path: &str, args: OpenArgs) -> Result<i32>;
        fn openat(dirfd: Option<i32>, path: &str, args: OpenArgs) -> Result<i32>;
        fn close(fd: i32) -> Result<()>;
        fn read(fd: i32, max: usize) -> Result<Vec<u8>>;
        fn pread(fd: i32, max: usize, off: u64) -> Result<Vec<u8>>;
        fn write(fd: i32, data: &[u8]) -> Result<usize>;
        fn pwrite(fd: i32, data: &[u8], off: u64) -> Result<usize>;
        fn lseek(fd: i32, off: i64, whence: i32) -> Result<u64>;
        fn fsync(fd: i32, data_only: bool) -> Result<()>;
        fn ftruncate(fd: i32, len: u64) -> Result<()>;
        fn fstat(fd: i32) -> Result<Stat>;
        fn stat(path: &str, follow: bool) -> Result<Stat>;
        fn access(path: &str, amode: i32) -> Result<()>;
        fn chmod(path: &str, mode: u32) -> Result<()>;
        fn chown(path: &str, uid: u32, gid: u32) -> Result<()>;
        fn link(existing: &str, new: &str) -> Result<()>;
        fn unlink(path: &str) -> Result<()>;
        fn rename(old: &str, new: &str, mode: RenameMode) -> Result<()>;
        fn symlink(target: &str, path: &str) -> Result<()>;
        fn readlink(path: &str) -> Result<String>;
        fn mkdir(path: &str, mode: u32) -> Result<()>;
        fn mkfifo(path: &str, mode: u32) -> Result<()>;
        fn rmdir(path: &str) -> Result<()>;
        fn opendir(path: &str) -> Result<u64>;
        fn readdir(handle: u64, max: usize) -> Result<Vec<DirEnt>>;
        fn rewinddir(handle: u64) -> Result<()>;
        fn closedir(handle: u64) -> Result<()>;
        fn read_dir(path: &str) -> Result<Vec<DirEnt>>;
        fn chdir(path: &str) -> Result<()>;
        fn fchdir(fd: i32) -> Result<()>;
        fn getcwd() -> Result<String>;
        fn dup(fd: i32) -> Result<i32>;
        fn dup2(fd: i32, new: i32) -> Result<i32>;
        fn fcntl(fd: i32, cmd: i32, arg: i32) -> Result<i32>;
        fn lock_range(fd: i32, cmd: i32, kind: RangeKind, whence: i32, start: i64, len: i64) -> Result<Option<RangeLock>>;
        fn fd_path(fd: i32) -> Result<String>;
        fn utime(path: &str, times: Option<(Nanos, Nanos)>) -> Result<()>;
        fn umask(mask: u32) -> Result<u32>;
        fn pathconf(path: &str, name: i32) -> Result<Option<i64>>;
        fn statvfs(path: &str) -> Result<StatVfs>;
        fn get_attrs(path: &str, ids: &[AttrId], follow: bool) -> Result<Vec<Attr>>;
        fn set_attr(path: &str, attr: &Attr, follow: bool) -> Result<()>;
        fn dspaut(path: &str) -> Result<AuthList>;
        fn chgaut(path: &str, c: &ChgAut) -> Result<()>;
        fn rvkaut(path: &str, user: &str) -> Result<()>;
        fn chgown(path: &str, new_owner: &str, revoke_old: bool) -> Result<()>;
        fn chgpgp(path: &str, new_group: Option<&str>, revoke_old: bool) -> Result<()>;
        fn check_out(path: &str) -> Result<()>;
        fn check_in(path: &str) -> Result<()>;
        fn start_journal(path: &str, j: Journal) -> Result<()>;
        fn end_journal(path: &str) -> Result<()>;
        fn copy(from: &str, to: &str, opts: CopyOpts) -> Result<()>;
        fn mov(from: &str, to: &str, replace: bool) -> Result<()>;
        fn remove_tree(path: &str) -> Result<()>;
        fn walk(path: &str, opts: WalkOpts, max: usize) -> Result<Vec<WalkEntry>>;
        fn path_from_file_id(id: FileId) -> Result<String>;
        fn crtudfs(udfs: &str, case_sensitive: bool, crtobjaud: CrtObjAud, text: &str) -> Result<()>;
        fn dltudfs(udfs: &str) -> Result<()>;
        fn mount(udfs: &str, over: &str, read_only: bool) -> Result<u32>;
        fn unmount(path: &str) -> Result<()>;
        fn dspudfs(udfs: &str) -> Result<UdfsInfo>;
        fn rcllnk(check_only: bool) -> Result<Report>;
        fn object_locks(path: &str) -> Result<Vec<LockInfo>>;
        fn owner_locks() -> Result<Vec<(ObjKey, LockInfo)>>;
    } };
}

impl<B: Backend> IfsClient for LocalIfs<B> {
    every_method!(forward);
}

/// An [`IfsClient`] with no file system behind it: every call answers
/// the one `errno`. The service answers with it while the IFS is not
/// mounted.
#[derive(Debug, Clone, Copy)]
pub struct Unavailable(pub crate::errno::Errno);

impl IfsClient for Unavailable {
    every_method!(refuse);
}

/// Read a whole file through a client, chunk by chunk.
pub fn read_all(c: &dyn IfsClient, fd: i32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let chunk = c.read(fd, CHUNK)?;
        if chunk.is_empty() {
            return Ok(out);
        }
        out.extend_from_slice(&chunk);
    }
}

/// Write everything through a client, chunk by chunk.
pub fn write_all(c: &dyn IfsClient, fd: i32, mut data: &[u8]) -> Result<()> {
    while !data.is_empty() {
        let n = c.write(fd, &data[..data.len().min(CHUNK)])?;
        if n == 0 {
            return Err(crate::errno::Errno::ENOSPC);
        }
        data = &data[n..];
    }
    Ok(())
}

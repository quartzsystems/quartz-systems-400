// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`IfsVfs`]: the [`Vfs`] over an IFS client (docs/NFS.md section 5).
//! A daemon thread has one, over its own link (`IfsClient` is not
//! `Send`). Each call runs as the request's caller
//! (`IfsClient::as_caller`); a caller no profile is (`ENOTENROLL`) is
//! run again as the export's anonymous user, or refused. A handle is
//! found by its file ID (`stat_by_id`, as the caller) and must lie
//! under its export's root. Reads and writes go through a small cache
//! of descriptors ([`fdcache`]); text is converted per host
//! ([`super::text`]).

pub mod fdcache;

mod attr;
mod change;
mod dir;
mod io;
mod lock;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::client::{Caller, IfsClient};
use ifs::ops::Stat;
use ifs::ops::audit::FileId;

use self::fdcache::{FdCache, Mode};
use super::cred;
use super::error::{VResult, VfsError};
use super::export::within;
use super::handle::FileHandle;
use super::instance::Instance;
use super::vfs::{
    Attr, Conflict, CreateHow, DirPage, FsStat, LockKind, LockOutcome, Made, Node, PathConf,
    ReadData, Req, SetAttr, ShareAccess, ShareDeny, Special, Stable, Time, Vfs, Wcc, Written,
};

/// How an [`IfsVfs`] keeps descriptors.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Descriptors kept at most.
    pub fds: usize,
    /// How long one is kept unused.
    pub idle: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fds: 64,
            idle: Duration::from_secs(5),
        }
    }
}

/// Why a step failed: a [`VfsError`], or the caller is no profile.
#[derive(Debug)]
pub(crate) enum Fail {
    V(VfsError),
    Enroll,
}

impl From<Errno> for Fail {
    fn from(e: Errno) -> Self {
        if e == Errno::ENOTENROLL {
            Self::Enroll
        } else {
            Self::V(VfsError::from(e))
        }
    }
}

impl From<VfsError> for Fail {
    fn from(e: VfsError) -> Self {
        Self::V(e)
    }
}

pub(crate) type Fr<T> = Result<T, Fail>;

/// A call's client: the IFS as the caller, and who that is.
pub(crate) struct As<'a> {
    pub c: &'a dyn IfsClient,
    pub who: &'a Caller,
}

/// The [`Vfs`] over an IFS client.
pub struct IfsVfs<C: IfsClient> {
    base: C,
    instance: Arc<Instance>,
    fds: RefCell<FdCache>,
    roots: RefCell<HashMap<u32, (FileId, String)>>,
    opens: RefCell<HashMap<u64, i32>>,
    next_open: std::cell::Cell<u64>,
}

impl<C: IfsClient> std::fmt::Debug for IfsVfs<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IfsVfs")
            .field("fds", &self.fds.borrow().len())
            .finish_non_exhaustive()
    }
}

impl<C: IfsClient> IfsVfs<C> {
    /// A file system over `base`, the daemon job's client, for the
    /// server start `instance`. The job's umask is set to 0: the
    /// client applied its own.
    pub fn new(base: C, instance: Arc<Instance>, cfg: Config) -> Self {
        let _ = base.umask(0);
        Self {
            base,
            instance,
            fds: RefCell::new(FdCache::new(cfg.fds, cfg.idle)),
            roots: RefCell::new(HashMap::new()),
            opens: RefCell::new(HashMap::new()),
            next_open: std::cell::Cell::new(1),
        }
    }

    /// The client underneath.
    pub fn client(&self) -> &C {
        &self.base
    }

    /// Descriptors kept now.
    pub fn cached_fds(&self) -> usize {
        self.fds.borrow().len()
    }

    /// Closes every kept descriptor.
    pub fn flush(&self) {
        let fds = self.fds.borrow_mut().drain();
        self.close_all(&fds);
    }

    fn close_all(&self, fds: &[i32]) {
        for fd in fds {
            let _ = self.base.close(*fd);
        }
    }

    /// Runs `f` as the request's caller, again as the export's
    /// anonymous user when the caller is no profile.
    pub(crate) fn run<T>(&self, r: &Req, f: impl Fn(&As<'_>) -> Fr<T>) -> VResult<T> {
        self.sweep();
        match self.once(&r.caller, &f) {
            Ok(v) => Ok(v),
            Err(Fail::V(e)) => Err(e),
            Err(Fail::Enroll) => {
                let Some(a) = cred::fallback(&r.caller, &*r.export) else {
                    return Err(VfsError::Acces);
                };
                match self.once(&a, &f) {
                    Ok(v) => Ok(v),
                    Err(Fail::V(e)) => Err(e),
                    Err(Fail::Enroll) => Err(VfsError::Acces),
                }
            }
        }
    }

    fn once<T>(&self, who: &Caller, f: &impl Fn(&As<'_>) -> Fr<T>) -> Fr<T> {
        let c = self.base.as_caller(who)?;
        f(&As { c: &*c, who })
    }

    fn sweep(&self) {
        let old = self.fds.borrow_mut().expire(Instant::now());
        self.close_all(&old);
    }

    /// The export's root path, as the job resolves it.
    fn root_path(&self, r: &Req, fresh: bool) -> Fr<String> {
        let id = r.export.id();
        let rid = r.export.root_id();
        if !fresh
            && let Some((f, p)) = self.roots.borrow().get(&id)
            && *f == rid
        {
            return Ok(p.clone());
        }
        let (p, _) = self.base.stat_by_id(rid).map_err(|e| match e {
            Errno::ESTALE | Errno::EBADFID | Errno::ENOENT => Fail::V(VfsError::Stale),
            e => Fail::from(e),
        })?;
        self.roots.borrow_mut().insert(id, (rid, p.clone()));
        Ok(p)
    }

    /// The path and attributes of the handle's object, as the caller;
    /// it must lie under the export.
    pub(crate) fn locate(&self, a: &As<'_>, r: &Req, fh: &FileHandle) -> Fr<(String, Stat)> {
        let (path, st) = match a.c.stat_by_id(fh.fid) {
            Ok(v) => v,
            Err(Errno::ESTALE | Errno::EBADFID | Errno::ENOENT) => {
                self.forget_object(fh.fid);
                return Err(Fail::V(VfsError::Stale));
            }
            Err(e) => return Err(Fail::from(e)),
        };
        let mut root = self.root_path(r, false)?;
        if within(&root, &path).is_none() {
            root = self.root_path(r, true)?;
            if within(&root, &path).is_none() {
                return Err(Fail::V(VfsError::BadHandle));
            }
        }
        Ok((path, st))
    }

    /// Whether `fh` is its export's root.
    pub(crate) fn is_root(r: &Req, fh: &FileHandle) -> bool {
        fh.fid == r.export.root_id()
    }

    /// Closes the kept descriptors of an object.
    pub(crate) fn forget_object(&self, fid: FileId) {
        let fds = self.fds.borrow_mut().drop_object(fid);
        self.close_all(&fds);
    }

    /// A kept descriptor of `fid` for `mode`, opened as the caller.
    pub(crate) fn fd(&self, a: &As<'_>, fid: FileId, mode: Mode) -> Fr<i32> {
        let now = Instant::now();
        if let Some(fd) = self.fds.borrow_mut().get(fid, a.who, mode, now) {
            return Ok(fd);
        }
        let flags = match mode {
            Mode::Read => ifs::ops::flags::O_RDONLY,
            Mode::Write => ifs::ops::flags::O_WRONLY,
        };
        let (fd, _) = a.c.open_by_id(fid, ifs::ops::OpenArgs::new(flags))?;
        let out = self.fds.borrow_mut().put(fid, a.who, mode, fd, now);
        self.close_all(&out);
        Ok(fd)
    }

    /// A descriptor failed: it is not kept.
    pub(crate) fn drop_fd(&self, fd: i32) {
        if self.fds.borrow_mut().forget(fd) {
            let _ = self.base.close(fd);
        }
    }

    pub(crate) fn verf(&self) -> [u8; 8] {
        self.instance.verf()
    }

    pub(crate) fn new_open(&self, fd: i32) -> u64 {
        let n = self.next_open.get();
        self.next_open.set(n + 1);
        self.opens.borrow_mut().insert(n, fd);
        n
    }
}

/// `dir/name`.
pub(crate) fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The parent of an absolute path.
pub(crate) fn parent(path: &str) -> &str {
    match path.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

/// The handle of an object in `r`'s export.
pub(crate) fn handle_of(r: &Req, st: &Stat) -> FileHandle {
    FileHandle {
        export: r.export.id(),
        fid: FileId {
            fs: st.vfs,
            ino: st.ino,
            generation: st.ino_gen,
        },
        root_ino: r.export.root_id().ino,
    }
}

/// The handle and attributes.
pub(crate) fn node_of(r: &Req, st: &Stat) -> Node {
    Node {
        fh: handle_of(r, st),
        attr: attr::attr_of(r, st),
    }
}

impl<C: IfsClient> Drop for IfsVfs<C> {
    fn drop(&mut self) {
        self.flush();
        let opens: Vec<i32> = self.opens.borrow_mut().drain().map(|(_, fd)| fd).collect();
        self.close_all(&opens);
    }
}

impl<C: IfsClient> Vfs for IfsVfs<C> {
    fn root(&self, r: &Req) -> VResult<Node> {
        self.run(r, |a| {
            let p = self.root_path(r, false)?;
            let st = a.c.stat(&p, true)?;
            Ok(node_of(r, &st))
        })
    }
    fn lookup_path(&self, r: &Req, path: &[u8]) -> VResult<Node> {
        self.do_lookup_path(r, path)
    }
    fn getattr(&self, r: &Req, fh: &FileHandle) -> VResult<Attr> {
        self.run(r, |a| {
            let (_, st) = self.locate(a, r, fh)?;
            Ok(attr::attr_of(r, &st))
        })
    }
    fn setattr(&self, r: &Req, fh: &FileHandle, s: &SetAttr, guard: Option<Time>) -> VResult<Wcc> {
        self.do_setattr(r, fh, s, guard)
    }
    fn lookup(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<(Node, Option<Attr>)> {
        self.do_lookup(r, dir, name)
    }
    fn access(&self, r: &Req, fh: &FileHandle, want: u32) -> VResult<(u32, Option<Attr>)> {
        self.do_access(r, fh, want)
    }
    fn readlink(&self, r: &Req, fh: &FileHandle) -> VResult<(Vec<u8>, Option<Attr>)> {
        self.do_readlink(r, fh)
    }
    fn read(&self, r: &Req, fh: &FileHandle, off: u64, count: u32) -> VResult<ReadData> {
        self.do_read(r, fh, off, count)
    }
    fn write(&self, r: &Req, fh: &FileHandle, off: u64, data: &[u8], stable: Stable) -> VResult<Written> {
        self.do_write(r, fh, off, data, stable)
    }
    fn commit(&self, r: &Req, fh: &FileHandle, off: u64, count: u32) -> VResult<([u8; 8], Wcc)> {
        self.do_commit(r, fh, off, count)
    }
    fn create(&self, r: &Req, dir: &FileHandle, name: &[u8], how: CreateHow) -> VResult<Made> {
        self.do_create(r, dir, name, how)
    }
    fn mkdir(&self, r: &Req, dir: &FileHandle, name: &[u8], s: &SetAttr) -> VResult<Made> {
        self.do_mkdir(r, dir, name, s)
    }
    fn symlink(&self, r: &Req, dir: &FileHandle, name: &[u8], target: &[u8], s: &SetAttr) -> VResult<Made> {
        self.do_symlink(r, dir, name, target, s)
    }
    fn mknod(&self, r: &Req, dir: &FileHandle, name: &[u8], what: Special, s: &SetAttr) -> VResult<Made> {
        self.do_mknod(r, dir, name, what, s)
    }
    fn remove(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<Wcc> {
        self.do_remove(r, dir, name, false)
    }
    fn rmdir(&self, r: &Req, dir: &FileHandle, name: &[u8]) -> VResult<Wcc> {
        self.do_remove(r, dir, name, true)
    }
    fn rename(&self, r: &Req, from: &FileHandle, fname: &[u8], to: &FileHandle, tname: &[u8]) -> VResult<(Wcc, Wcc)> {
        self.do_rename(r, from, fname, to, tname)
    }
    fn link(&self, r: &Req, fh: &FileHandle, dir: &FileHandle, name: &[u8]) -> VResult<(Option<Attr>, Wcc)> {
        self.do_link(r, fh, dir, name)
    }
    fn readdir(&self, r: &Req, dir: &FileHandle, cookie: u64, verf: [u8; 8], max: usize, plus: bool) -> VResult<DirPage> {
        self.do_readdir(r, dir, cookie, verf, max, plus)
    }
    fn fsstat(&self, r: &Req, fh: &FileHandle) -> VResult<(FsStat, Option<Attr>)> {
        self.do_fsstat(r, fh)
    }
    fn pathconf(&self, r: &Req, fh: &FileHandle) -> VResult<(PathConf, Option<Attr>)> {
        self.do_pathconf(r, fh)
    }
    fn open(&self, r: &Req, fh: &FileHandle, access: ShareAccess, deny: ShareDeny) -> VResult<u64> {
        self.do_open(r, fh, access, deny)
    }
    fn close(&self, open: u64) -> VResult<()> {
        let fd = self.opens.borrow_mut().remove(&open).ok_or(VfsError::Inval)?;
        self.base.close(fd).map_err(VfsError::from)
    }
    fn lock(&self, r: &Req, fh: &FileHandle, sub: u64, kind: LockKind, start: u64, len: u64, wait: bool) -> VResult<LockOutcome> {
        self.do_lock(r, fh, sub, kind, start, len, wait)
    }
    fn test_lock(&self, r: &Req, fh: &FileHandle, sub: u64, kind: LockKind, start: u64, len: u64) -> VResult<Option<Conflict>> {
        self.do_test_lock(r, fh, sub, kind, start, len)
    }
    fn unlock(&self, r: &Req, fh: &FileHandle, sub: u64, start: u64, len: u64) -> VResult<()> {
        self.do_unlock(r, fh, sub, start, len)
    }
    fn cancel_wait(&self, token: u64) -> VResult<bool> {
        self.base.lock_cancel(token).map_err(VfsError::from)
    }
    fn release_sub(&self, sub: u64) -> VResult<u32> {
        match self.base.lock_release_sub(sub) {
            Ok(n) => Ok(n),
            Err(Errno::ENOENT) => Ok(0),
            Err(e) => Err(VfsError::from(e)),
        }
    }
    fn lease(&self, r: &Req, fh: &FileHandle, tag: u64, key: [u8; 32], want: u8) -> VResult<u8> {
        self.run(r, |a| {
            let (path, _) = self.locate(a, r, fh)?;
            Ok(a.c.lease_grant(&path, fh.fid, tag, key, want)?)
        })
    }
    fn lease_report(&self, tag: u64, leases: &[([u8; 32], u8)]) -> VResult<()> {
        self.base.lease_report(tag, leases).map_err(VfsError::from)
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A remote file system in memory, for tests: a small tree with
//! handles that go stale when their object is removed or made again,
//! permission checks by mode bits, unstable writes a restart loses
//! (with a new write verifier), byte-range locks, and knobs for what a
//! network does: latency, a server that is down (calls wait, or time
//! out), failures injected. It records the threads that call it, so a
//! test can prove no call came from the thread that called the IFS.
//! [`MemServer`] is the server a test keeps; [`MemFactory`] mounts it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::ThreadId;
use std::time::Duration;

use super::fs::{Mounted, RemoteFactory, RemoteFs, access};
use super::types::{
    CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, MountCcsid, OpenToken,
    PathConf, RAttr, REntry, RHandle, RKind, RResult, RemoteError, RemoteInfo, SetAttr, SetTime,
    Share, Stable, WriteReply,
};
use crate::errno::Errno;

const ROOT: u64 = 1;

#[derive(Debug, Clone)]
struct Node {
    generation: u32,
    kind: RKind,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    data: Vec<u8>,
    /// What a restart keeps.
    stable: Vec<u8>,
    link: Vec<u8>,
    entries: BTreeMap<Vec<u8>, u64>,
    atime: i64,
    mtime: i64,
    ctime: i64,
    change: u64,
}

#[derive(Debug, Default)]
struct State {
    nodes: HashMap<u64, Node>,
    next: u64,
    gens: u32,
    change: u64,
    verifier: u64,
    locks: Vec<(u64, LockOwner, LockKind, u64, u64)>,
    down: bool,
    down_times_out: bool,
    fail_next: u32,
    fail_with: Option<RemoteError>,
    fail_op: Option<&'static str>,
    latency: Duration,
    page: usize,
    exclusive: HashMap<(u64, Vec<u8>), u64>,
    unmounted: bool,
}

/// The server: its tree and its knobs.
#[derive(Debug)]
pub struct MemServer {
    st: Mutex<State>,
    threads: Mutex<HashSet<ThreadId>>,
    calls: AtomicU64,
    ops: Mutex<HashMap<&'static str, u64>>,
}

fn now() -> i64 {
    crate::ops::now()
}

fn handle(id: u64, generation: u32) -> RHandle {
    let mut v = id.to_be_bytes().to_vec();
    v.extend_from_slice(&generation.to_be_bytes());
    RHandle(v)
}

fn err(e: Errno) -> RemoteError {
    RemoteError::Errno(e)
}

impl Default for MemServer {
    fn default() -> Self {
        Self::new()
    }
}

impl MemServer {
    /// An empty export: the root directory, `0o777`, owned by UID 0.
    #[must_use]
    pub fn new() -> Self {
        let mut st = State {
            next: ROOT + 1,
            gens: 1,
            change: 1,
            verifier: 0x51_5334,
            page: 64,
            ..State::default()
        };
        st.nodes.insert(
            ROOT,
            Node {
                generation: 1,
                kind: RKind::Dir,
                mode: 0o777,
                uid: 0,
                gid: 0,
                nlink: 2,
                data: Vec::new(),
                stable: Vec::new(),
                link: Vec::new(),
                entries: BTreeMap::new(),
                atime: now(),
                mtime: now(),
                ctime: now(),
                change: 1,
            },
        );
        Self {
            st: Mutex::new(st),
            threads: Mutex::new(HashSet::new()),
            calls: AtomicU64::new(0),
            ops: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every call waits this long first.
    pub fn set_latency(&self, d: Duration) {
        self.lock().latency = d;
    }

    /// The server stops answering: calls wait until it is back, or,
    /// with `times_out`, fail as a soft mount's do.
    pub fn set_down(&self, down: bool, times_out: bool) {
        let mut s = self.lock();
        s.down = down;
        s.down_times_out = times_out;
    }

    /// The next `n` calls (of `op` only, when given) fail with `e`.
    pub fn fail_next(&self, n: u32, e: RemoteError, op: Option<&'static str>) {
        let mut s = self.lock();
        s.fail_next = n;
        s.fail_with = Some(e);
        s.fail_op = op;
    }

    /// Entries per directory page.
    pub fn set_page(&self, n: usize) {
        self.lock().page = n.max(1);
    }

    /// The server restarts: unstable data is lost and the write
    /// verifier changes.
    pub fn restart(&self) {
        let mut s = self.lock();
        s.verifier += 1;
        for n in s.nodes.values_mut() {
            if n.kind == RKind::File && n.data != n.stable {
                n.data.clone_from(&n.stable);
            }
        }
        s.locks.clear();
    }

    /// The threads that made calls.
    pub fn threads(&self) -> HashSet<ThreadId> {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Calls so far.
    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }

    /// Calls of one kind so far (`getattr`, `read`, `write` ...).
    pub fn calls_of(&self, op: &str) -> u64 {
        self.ops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(op)
            .copied()
            .unwrap_or(0)
    }

    /// Whether the mount told the server it ended.
    pub fn unmounted(&self) -> bool {
        self.lock().unmounted
    }

    fn enter(&self, op: &'static str) -> RResult<()> {
        self.threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(std::thread::current().id());
        self.calls.fetch_add(1, Ordering::Relaxed);
        *self
            .ops
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(op)
            .or_insert(0) += 1;
        let lat = self.lock().latency;
        if !lat.is_zero() {
            std::thread::sleep(lat);
        }
        loop {
            let s = self.lock();
            if !s.down {
                break;
            }
            if s.down_times_out {
                return Err(RemoteError::TimedOut);
            }
            drop(s);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut s = self.lock();
        if s.fail_next > 0 && s.fail_op.is_none_or(|o| o == op) {
            s.fail_next -= 1;
            return Err(s.fail_with.unwrap_or(RemoteError::Errno(Errno::EIO)));
        }
        Ok(())
    }

    // The tree, as a test sees it from the server side.

    fn find(s: &State, path: &str) -> Option<u64> {
        let mut id = ROOT;
        for c in path.split('/').filter(|c| !c.is_empty()) {
            id = *s.nodes.get(&id)?.entries.get(c.as_bytes())?;
        }
        Some(id)
    }

    fn split(path: &str) -> (&str, &str) {
        let p = path.trim_end_matches('/');
        p.rfind('/').map_or(("", p), |i| (&p[..i], &p[i + 1..]))
    }

    /// Make or replace a file at `path` (its directory must exist).
    pub fn put_file(&self, path: &str, data: &[u8]) {
        let mut s = self.lock();
        let (dir, name) = Self::split(path);
        let d = Self::find(&s, dir).expect("directory");
        let existing = s.nodes[&d].entries.get(name.as_bytes()).copied();
        if let Some(id) = existing {
            s.change += 1;
            let ch = s.change;
            let n = s.nodes.get_mut(&id).expect("node");
            n.data = data.to_vec();
            n.stable = data.to_vec();
            n.mtime = now();
            n.change = ch;
            return;
        }
        let id = Self::make(&mut s, d, name.as_bytes(), RKind::File, 0o666, 0, 0);
        let n = s.nodes.get_mut(&id).expect("made");
        n.data = data.to_vec();
        n.stable = data.to_vec();
    }

    /// A file's bytes as the server holds them.
    pub fn file(&self, path: &str) -> Option<Vec<u8>> {
        let s = self.lock();
        let id = Self::find(&s, path)?;
        Some(s.nodes[&id].data.clone())
    }

    /// Make a directory at `path`.
    pub fn put_dir(&self, path: &str) {
        let mut s = self.lock();
        let (dir, name) = Self::split(path);
        let d = Self::find(&s, dir).expect("directory");
        Self::make(&mut s, d, name.as_bytes(), RKind::Dir, 0o777, 0, 0);
    }

    /// Set the owner and mode of `path`.
    pub fn set_owner(&self, path: &str, uid: u32, gid: u32, mode: u32) {
        let mut s = self.lock();
        let id = Self::find(&s, path).expect("path");
        s.change += 1;
        let ch = s.change;
        let n = s.nodes.get_mut(&id).expect("node");
        n.uid = uid;
        n.gid = gid;
        n.mode = mode;
        n.change = ch;
    }

    /// Remove `path` and make it again, empty: the old handle is stale.
    pub fn recreate(&self, path: &str) {
        let mut s = self.lock();
        let (dir, name) = Self::split(path);
        let d = Self::find(&s, dir).expect("directory");
        let old = s
            .nodes
            .get_mut(&d)
            .expect("dir")
            .entries
            .remove(name.as_bytes());
        let kind = old
            .and_then(|o| s.nodes.remove(&o))
            .map_or(RKind::File, |n| n.kind);
        Self::make(&mut s, d, name.as_bytes(), kind, 0o666, 0, 0);
    }

    /// Whether `path` exists.
    pub fn exists(&self, path: &str) -> bool {
        Self::find(&self.lock(), path).is_some()
    }

    /// The names in directory `path`.
    pub fn names(&self, path: &str) -> Vec<String> {
        let s = self.lock();
        let Some(id) = Self::find(&s, path) else {
            return Vec::new();
        };
        s.nodes[&id]
            .entries
            .keys()
            .map(|k| String::from_utf8_lossy(k).into_owned())
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn make(
        s: &mut State,
        dir: u64,
        name: &[u8],
        kind: RKind,
        mode: u32,
        uid: u32,
        gid: u32,
    ) -> u64 {
        let id = s.next;
        s.next += 1;
        s.gens += 1;
        s.change += 1;
        let t = now();
        s.nodes.insert(
            id,
            Node {
                generation: s.gens,
                kind,
                mode: mode & 0o7777,
                uid,
                gid,
                nlink: if kind == RKind::Dir { 2 } else { 1 },
                data: Vec::new(),
                stable: Vec::new(),
                link: Vec::new(),
                entries: BTreeMap::new(),
                atime: t,
                mtime: t,
                ctime: t,
                change: s.change,
            },
        );
        let ch = s.change;
        let d = s.nodes.get_mut(&dir).expect("directory");
        d.entries.insert(name.to_vec(), id);
        d.mtime = t;
        d.change = ch;
        id
    }

    fn touch(s: &mut State, id: u64) {
        s.change += 1;
        let ch = s.change;
        if let Some(n) = s.nodes.get_mut(&id) {
            n.mtime = now();
            n.ctime = n.mtime;
            n.change = ch;
        }
    }

    fn node(s: &State, h: &RHandle) -> RResult<u64> {
        if h.0.len() != 12 {
            return Err(err(Errno::EINVAL));
        }
        let id = u64::from_be_bytes(h.0[..8].try_into().expect("8"));
        let generation = u32::from_be_bytes(h.0[8..].try_into().expect("4"));
        match s.nodes.get(&id) {
            Some(n) if n.generation == generation => Ok(id),
            _ => Err(RemoteError::Stale),
        }
    }

    fn attr(s: &State, id: u64) -> RAttr {
        let n = &s.nodes[&id];
        RAttr {
            kind: n.kind,
            mode: n.mode,
            nlink: n.nlink,
            uid: n.uid,
            gid: n.gid,
            size: if n.kind == RKind::Symlink {
                n.link.len() as u64
            } else {
                n.data.len() as u64
            },
            used: n.data.len() as u64,
            fileid: id,
            rdev: 0,
            atime: n.atime,
            mtime: n.mtime,
            ctime: n.ctime,
            change: n.change,
        }
    }

    /// The access bits `cred` has to node `id`.
    fn bits(s: &State, id: u64, cred: &Cred) -> u32 {
        let n = &s.nodes[&id];
        if cred.uid == 0 {
            return 0x3f;
        }
        let rwx = if cred.uid == n.uid {
            n.mode >> 6
        } else if cred.gid == n.gid || cred.gids.contains(&n.gid) {
            n.mode >> 3
        } else {
            n.mode
        } & 7;
        let mut b = 0;
        if rwx & 4 != 0 {
            b |= access::READ;
        }
        if rwx & 2 != 0 {
            b |= access::MODIFY | access::EXTEND | access::DELETE;
        }
        if rwx & 1 != 0 {
            b |= access::EXECUTE | access::LOOKUP;
        }
        b
    }

    fn need(s: &State, id: u64, cred: &Cred, bit: u32) -> RResult<()> {
        if Self::bits(s, id, cred) & bit == 0 {
            return Err(err(Errno::EACCES));
        }
        Ok(())
    }

    fn apply(s: &mut State, id: u64, set: &SetAttr, cred: &Cred) -> RResult<()> {
        let owner = s.nodes[&id].uid;
        if (set.mode.is_some() || set.uid.is_some() || set.gid.is_some())
            && cred.uid != 0
            && cred.uid != owner
        {
            return Err(err(Errno::EPERM));
        }
        if set.uid.is_some_and(|u| u != owner) && cred.uid != 0 {
            return Err(err(Errno::EPERM));
        }
        if set.size.is_some() {
            Self::need(s, id, cred, access::MODIFY)?;
        }
        s.change += 1;
        let ch = s.change;
        let n = s.nodes.get_mut(&id).expect("node");
        if let Some(m) = set.mode {
            n.mode = m & 0o7777;
        }
        if let Some(u) = set.uid {
            n.uid = u;
        }
        if let Some(g) = set.gid {
            n.gid = g;
        }
        if let Some(z) = set.size {
            n.data
                .resize(usize::try_from(z).map_err(|_| err(Errno::EFBIG))?, 0);
            n.stable.clone_from(&n.data);
            n.mtime = now();
        }
        let t = |v: SetTime| match v {
            SetTime::Server => now(),
            SetTime::At(x) => x,
        };
        if let Some(a) = set.atime {
            n.atime = t(a);
        }
        if let Some(m) = set.mtime {
            n.mtime = t(m);
        }
        n.ctime = now();
        n.change = ch;
        Ok(())
    }

    fn dir_of(s: &State, h: &RHandle, cred: &Cred, write: bool) -> RResult<u64> {
        let d = Self::node(s, h)?;
        if s.nodes[&d].kind != RKind::Dir {
            return Err(err(Errno::ENOTDIR));
        }
        Self::need(
            s,
            d,
            cred,
            if write {
                access::MODIFY
            } else {
                access::LOOKUP
            },
        )?;
        Ok(d)
    }

    fn create_in(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        let mut s = self.lock();
        let d = Self::dir_of(&s, dir, cred, true)?;
        if s.nodes[&d].entries.contains_key(name) {
            return Err(err(Errno::EEXIST));
        }
        let mode = set.mode.unwrap_or(0o644);
        let id = Self::make(&mut s, d, name, kind, mode, cred.uid, cred.gid);
        if kind == RKind::Dir {
            s.nodes.get_mut(&d).expect("dir").nlink += 1;
        }
        Ok((handle(id, s.nodes[&id].generation), Self::attr(&s, id)))
    }
}

/// The mount of a [`MemServer`]: what the IFS calls.
#[derive(Debug, Clone)]
pub struct MemRemote(pub Arc<MemServer>);

impl RemoteFs for MemRemote {
    fn root(&self) -> RResult<(RHandle, RAttr)> {
        self.0.enter("root")?;
        let s = self.0.lock();
        Ok((
            handle(ROOT, s.nodes[&ROOT].generation),
            MemServer::attr(&s, ROOT),
        ))
    }

    fn getattr(&self, _cred: &Cred, h: &RHandle) -> RResult<RAttr> {
        self.0.enter("getattr")?;
        let s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        Ok(MemServer::attr(&s, id))
    }

    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr> {
        self.0.enter("setattr")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        if guard.is_some_and(|g| g != s.nodes[&id].ctime) {
            return Err(err(Errno::EAGAIN));
        }
        MemServer::apply(&mut s, id, set, cred)?;
        Ok(MemServer::attr(&s, id))
    }

    fn lookup(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)> {
        self.0.enter("lookup")?;
        let s = self.0.lock();
        let d = MemServer::dir_of(&s, dir, cred, false)?;
        let id = *s.nodes[&d].entries.get(name).ok_or(err(Errno::ENOENT))?;
        Ok((handle(id, s.nodes[&id].generation), MemServer::attr(&s, id)))
    }

    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32> {
        self.0.enter("access")?;
        let s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        Ok(MemServer::bits(&s, id, cred) & mask)
    }

    fn readlink(&self, _cred: &Cred, h: &RHandle) -> RResult<Vec<u8>> {
        self.0.enter("readlink")?;
        let s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        if s.nodes[&id].kind != RKind::Symlink {
            return Err(err(Errno::EINVAL));
        }
        Ok(s.nodes[&id].link.clone())
    }

    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)> {
        self.0.enter("read")?;
        let s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        MemServer::need(&s, id, cred, access::READ)?;
        let n = &s.nodes[&id];
        if n.kind == RKind::Dir {
            return Err(err(Errno::EISDIR));
        }
        let start = usize::try_from(off).unwrap_or(usize::MAX).min(n.data.len());
        let end = start.saturating_add(len as usize).min(n.data.len());
        Ok((n.data[start..end].to_vec(), end == n.data.len()))
    }

    fn write(
        &self,
        cred: &Cred,
        h: &RHandle,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> RResult<WriteReply> {
        self.0.enter("write")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        MemServer::need(&s, id, cred, access::MODIFY)?;
        let verifier = s.verifier;
        let n = s.nodes.get_mut(&id).expect("node");
        let start = usize::try_from(off).map_err(|_| err(Errno::EFBIG))?;
        if n.data.len() < start + data.len() {
            n.data.resize(start + data.len(), 0);
        }
        n.data[start..start + data.len()].copy_from_slice(data);
        if stable != Stable::Unstable {
            n.stable.clone_from(&n.data);
        }
        MemServer::touch(&mut s, id);
        Ok(WriteReply {
            count: u32::try_from(data.len()).unwrap_or(u32::MAX),
            committed: stable,
            verifier,
        })
    }

    fn commit(&self, _cred: &Cred, h: &RHandle, _off: u64, _len: u32) -> RResult<u64> {
        self.0.enter("commit")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        let v = s.verifier;
        let n = s.nodes.get_mut(&id).expect("node");
        n.stable.clone_from(&n.data);
        Ok(v)
    }

    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)> {
        self.0.enter("create")?;
        match how {
            CreateMode::Unchecked(set) => {
                {
                    let s = self.0.lock();
                    let d = MemServer::dir_of(&s, dir, cred, false)?;
                    if let Some(id) = s.nodes[&d].entries.get(name) {
                        return Ok((
                            handle(*id, s.nodes[id].generation),
                            MemServer::attr(&s, *id),
                        ));
                    }
                }
                self.0.create_in(cred, dir, name, RKind::File, &set)
            }
            CreateMode::Guarded(set) => self.0.create_in(cred, dir, name, RKind::File, &set),
            CreateMode::Exclusive(v) => {
                let mut s = self.0.lock();
                let d = MemServer::dir_of(&s, dir, cred, true)?;
                if let Some(id) = s.nodes[&d].entries.get(name).copied() {
                    if s.exclusive.get(&(d, name.to_vec())) == Some(&v) {
                        return Ok((handle(id, s.nodes[&id].generation), MemServer::attr(&s, id)));
                    }
                    return Err(err(Errno::EEXIST));
                }
                let id = MemServer::make(&mut s, d, name, RKind::File, 0, cred.uid, cred.gid);
                s.exclusive.insert((d, name.to_vec()), v);
                Ok((handle(id, s.nodes[&id].generation), MemServer::attr(&s, id)))
            }
        }
    }

    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        self.0.enter("mkdir")?;
        self.0.create_in(cred, dir, name, RKind::Dir, set)
    }

    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        target: &[u8],
        _set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        self.0.enter("symlink")?;
        let (h, _) = self.0.create_in(
            cred,
            dir,
            name,
            RKind::Symlink,
            &SetAttr {
                mode: Some(0o777),
                ..SetAttr::default()
            },
        )?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, &h)?;
        s.nodes.get_mut(&id).expect("link").link = target.to_vec();
        Ok((h, MemServer::attr(&s, id)))
    }

    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        self.0.enter("mknod")?;
        if kind != RKind::Fifo {
            return Err(err(Errno::ENOTSUP));
        }
        self.0.create_in(cred, dir, name, kind, set)
    }

    fn remove(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.0.enter("remove")?;
        let mut s = self.0.lock();
        let d = MemServer::dir_of(&s, dir, cred, true)?;
        let id = *s.nodes[&d].entries.get(name).ok_or(err(Errno::ENOENT))?;
        if s.nodes[&id].kind == RKind::Dir {
            return Err(err(Errno::EISDIR));
        }
        s.nodes.get_mut(&d).expect("dir").entries.remove(name);
        let n = s.nodes.get_mut(&id).expect("node");
        n.nlink -= 1;
        if n.nlink == 0 {
            s.nodes.remove(&id);
        } else {
            MemServer::touch(&mut s, id);
        }
        MemServer::touch(&mut s, d);
        Ok(())
    }

    fn rmdir(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.0.enter("rmdir")?;
        let mut s = self.0.lock();
        let d = MemServer::dir_of(&s, dir, cred, true)?;
        let id = *s.nodes[&d].entries.get(name).ok_or(err(Errno::ENOENT))?;
        let n = &s.nodes[&id];
        if n.kind != RKind::Dir {
            return Err(err(Errno::ENOTDIR));
        }
        if !n.entries.is_empty() {
            return Err(err(Errno::ENOTEMPTY));
        }
        s.nodes.get_mut(&d).expect("dir").entries.remove(name);
        s.nodes.remove(&id);
        s.nodes.get_mut(&d).expect("dir").nlink -= 1;
        MemServer::touch(&mut s, d);
        Ok(())
    }

    fn rename(
        &self,
        cred: &Cred,
        from_dir: &RHandle,
        from: &[u8],
        to_dir: &RHandle,
        to: &[u8],
    ) -> RResult<()> {
        self.0.enter("rename")?;
        let mut s = self.0.lock();
        let fd = MemServer::dir_of(&s, from_dir, cred, true)?;
        let td = MemServer::dir_of(&s, to_dir, cred, true)?;
        let id = *s.nodes[&fd].entries.get(from).ok_or(err(Errno::ENOENT))?;
        if let Some(old) = s.nodes[&td].entries.get(to).copied() {
            if old == id {
                return Ok(());
            }
            let o = &s.nodes[&old];
            if o.kind == RKind::Dir && !o.entries.is_empty() {
                return Err(err(Errno::ENOTEMPTY));
            }
            s.nodes.remove(&old);
        }
        s.nodes.get_mut(&fd).expect("dir").entries.remove(from);
        s.nodes
            .get_mut(&td)
            .expect("dir")
            .entries
            .insert(to.to_vec(), id);
        MemServer::touch(&mut s, fd);
        MemServer::touch(&mut s, td);
        MemServer::touch(&mut s, id);
        Ok(())
    }

    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, name: &[u8]) -> RResult<()> {
        self.0.enter("link")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        let d = MemServer::dir_of(&s, dir, cred, true)?;
        if s.nodes[&d].entries.contains_key(name) {
            return Err(err(Errno::EEXIST));
        }
        s.nodes
            .get_mut(&d)
            .expect("dir")
            .entries
            .insert(name.to_vec(), id);
        s.nodes.get_mut(&id).expect("node").nlink += 1;
        MemServer::touch(&mut s, d);
        MemServer::touch(&mut s, id);
        Ok(())
    }

    fn readdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        _verifier: u64,
        max: u32,
    ) -> RResult<DirPage> {
        self.0.enter("readdir")?;
        let s = self.0.lock();
        let d = MemServer::dir_of(&s, dir, cred, false)?;
        MemServer::need(&s, d, cred, access::READ)?;
        let n = &s.nodes[&d];
        let page = s.page.min(max as usize).max(1);
        let skip = usize::try_from(cookie).unwrap_or(usize::MAX);
        let entries: Vec<REntry> = n
            .entries
            .iter()
            .enumerate()
            .skip(skip)
            .take(page)
            .map(|(i, (name, id))| REntry {
                name: name.clone(),
                cookie: i as u64 + 1,
                fileid: *id,
                // Every other page without attributes: both paths run.
                attr: (i % 2 == 0).then(|| MemServer::attr(&s, *id)),
                handle: (i % 2 == 0).then(|| handle(*id, s.nodes[id].generation)),
            })
            .collect();
        let eof = skip + entries.len() >= n.entries.len();
        Ok(DirPage {
            entries,
            eof,
            verifier: n.change,
        })
    }

    fn statfs(&self, _cred: &Cred, _h: &RHandle) -> RResult<FsStat> {
        self.0.enter("statfs")?;
        let s = self.0.lock();
        let used: u64 = s.nodes.values().map(|n| n.data.len() as u64).sum();
        Ok(FsStat {
            total_bytes: 1 << 30,
            free_bytes: (1 << 30) - used,
            avail_bytes: (1 << 30) - used,
            total_files: 100_000,
            free_files: 100_000 - s.nodes.len() as u64,
            avail_files: 100_000 - s.nodes.len() as u64,
        })
    }

    fn pathconf(&self, _cred: &Cred, _h: &RHandle) -> RResult<PathConf> {
        self.0.enter("pathconf")?;
        Ok(PathConf::default())
    }

    fn open(
        &self,
        _cred: &Cred,
        h: &RHandle,
        _share: Share,
        _owner: LockOwner,
    ) -> RResult<OpenToken> {
        self.0.enter("open")?;
        let s = self.0.lock();
        MemServer::node(&s, h)?;
        Ok(OpenToken::default())
    }

    fn close(&self, _cred: &Cred, _h: &RHandle, _token: &OpenToken) -> RResult<()> {
        self.0.enter("close")?;
        Ok(())
    }

    fn lock(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        _token: &OpenToken,
    ) -> RResult<()> {
        self.0.enter("lock")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        if conflict(&s, id, owner, kind, off, len).is_some() {
            return Err(err(Errno::EAGAIN));
        }
        s.locks.push((id, owner, kind, off, len));
        Ok(())
    }

    fn unlock(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        self.0.enter("unlock")?;
        let mut s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        s.locks.retain(|l| {
            !(l.0 == id && l.1 == owner && (len == 0 || (l.3 >= off && l.3 < off + len)))
        });
        Ok(())
    }

    fn test(
        &self,
        _cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        self.0.enter("test")?;
        let s = self.0.lock();
        let id = MemServer::node(&s, h)?;
        Ok(conflict(&s, id, owner, kind, off, len))
    }

    fn unmount(&self) -> RResult<()> {
        self.0.enter("unmount")?;
        self.0.lock().unmounted = true;
        Ok(())
    }
}

fn conflict(
    s: &State,
    id: u64,
    owner: LockOwner,
    kind: LockKind,
    off: u64,
    len: u64,
) -> Option<LockConflict> {
    let end = |o: u64, l: u64| {
        if l == 0 {
            u64::MAX
        } else {
            o.saturating_add(l)
        }
    };
    s.locks
        .iter()
        .find(|l| {
            l.0 == id
                && l.1 != owner
                && (l.2 == LockKind::Write || kind == LockKind::Write)
                && l.3 < end(off, len)
                && off < end(l.3, l.4)
        })
        .map(|l| LockConflict {
            kind: l.2,
            off: l.3,
            len: l.4,
            holder: l.1.0,
        })
}

/// Mounts [`MemServer`]s by name (`host:/path`), for `*NFS` or any kind
/// it was made for.
#[derive(Default)]
pub struct MemFactory {
    kind: String,
    servers: Mutex<HashMap<String, (Arc<MemServer>, RemoteInfo)>>,
    /// The CCSIDs the last mount was asked for.
    pub last_ccsid: Mutex<Option<MountCcsid>>,
    /// The options string the last mount was given.
    pub last_options: Mutex<Option<String>>,
}

impl MemFactory {
    /// A factory for `kind` (`*NFS`).
    #[must_use]
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            ..Self::default()
        }
    }

    /// Offer `server` as `mfs`, with what the mount will say of itself.
    pub fn add(&self, mfs: &str, server: Arc<MemServer>, info: RemoteInfo) {
        self.servers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(mfs.to_string(), (server, info));
    }
}

impl RemoteFactory for MemFactory {
    fn supports(&self, kind: &str) -> bool {
        kind.eq_ignore_ascii_case(&self.kind)
    }

    fn mount(
        &self,
        _kind: &str,
        mfs: &str,
        options: &str,
        ccsid: MountCcsid,
        _cred: &Cred,
    ) -> RResult<Mounted> {
        *self
            .last_ccsid
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(ccsid);
        *self
            .last_options
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(options.to_string());
        let (server, mut info) = self
            .servers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(mfs)
            .cloned()
            .ok_or(RemoteError::Errno(Errno::ENOENT))?;
        server.enter("mount")?;
        if info.source.is_empty() {
            info.source = mfs.to_string();
        }
        if options.split(',').any(|o| o.trim() == "ro") {
            info.read_only = true;
        }
        Ok(Mounted {
            fs: Box::new(MemRemote(server)),
            info,
        })
    }
}

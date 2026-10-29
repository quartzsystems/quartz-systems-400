// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An in-memory `Sys` for tests: a small file system with the root's
//! naming rule (case-insensitive except under `/QOpenSys`), registered
//! utilities run on threads, recorded signals and a settable clock.

use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use super::stream::{Fd, Null, Stream};
use super::util::{Registry, UtilCtx, Utility};
use super::{
    Access, Child, Errno, FileType, FsInfo, Limit, OpenFlags, ProcInfo, SigAction, SpawnRequest,
    Stat, Sys, SysError, WaitStatus,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Clone)]
enum Kind {
    Dir(BTreeMap<String, usize>),
    File(Arc<Mutex<Vec<u8>>>),
    Symlink(String),
    Null,
    /// A `/QSYS.LIB` object that is not a library, file or member:
    /// its size.
    Object(u64),
}

#[derive(Clone)]
struct Node {
    kind: Kind,
    mode: u32,
    mtime: i64,
    parent: usize,
    uid: u32,
    gid: u32,
}

/// The user ID files are made with in a `MemSys` (`QSHUSER`).
pub const MEM_UID: u32 = 100;

struct Fs {
    nodes: Vec<Node>,
}

/// Is a directory's naming case-insensitive? The root is; `/QOpenSys`
/// is not.
fn icase_path(dir: &str) -> bool {
    let up = dir.to_ascii_uppercase();
    !(up == "/QOPENSYS" || up.starts_with("/QOPENSYS/"))
}

impl Fs {
    fn new() -> Self {
        Self {
            nodes: vec![Node {
                kind: Kind::Dir(BTreeMap::new()),
                mode: 0o755,
                mtime: 0,
                parent: 0,
                uid: 0,
                gid: 0,
            }],
        }
    }

    fn lookup(&self, dir: usize, dir_path: &str, name: &str) -> Option<(usize, String)> {
        let Kind::Dir(entries) = &self.nodes[dir].kind else {
            return None;
        };
        if let Some(&i) = entries.get(name) {
            return Some((i, name.to_string()));
        }
        if icase_path(dir_path) {
            for (k, &i) in entries {
                if k.eq_ignore_ascii_case(name) || k.to_lowercase() == name.to_lowercase() {
                    return Some((i, k.clone()));
                }
            }
        }
        None
    }

    /// Resolve to (node, canonical path).
    fn resolve(
        &self,
        path: &str,
        follow_last: bool,
        depth: u32,
    ) -> Result<(usize, String), SysError> {
        if depth > 20 {
            return Err(Errno::ELOOP.into());
        }
        if !path.starts_with('/') {
            return Err(Errno::ENOENT.into());
        }
        let comps: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        let mut cur = 0usize;
        let mut cur_path = "/".to_string();
        for (i, comp) in comps.iter().enumerate() {
            let last = i + 1 == comps.len();
            match *comp {
                "." => continue,
                ".." => {
                    cur = self.nodes[cur].parent;
                    cur_path = parent_path(&cur_path);
                    continue;
                }
                _ => {}
            }
            if !matches!(self.nodes[cur].kind, Kind::Dir(_)) {
                return Err(Errno::ENOTDIR.into());
            }
            let (next, name) = self
                .lookup(cur, &cur_path, comp)
                .ok_or(SysError::new(Errno::ENOENT))?;
            if let Kind::Symlink(target) = &self.nodes[next].kind
                && (!last || follow_last)
            {
                let base = if target.starts_with('/') {
                    target.clone()
                } else {
                    join(&cur_path, target)
                };
                let rest: Vec<&str> = comps[i + 1..].to_vec();
                let full = if rest.is_empty() {
                    base
                } else {
                    format!("{base}/{}", rest.join("/"))
                };
                return self.resolve(&full, follow_last, depth + 1);
            }
            cur = next;
            cur_path = join(&cur_path, &name);
        }
        Ok((cur, cur_path))
    }

    fn parent_of(&self, path: &str) -> Result<(usize, String, String), SysError> {
        let trimmed = path.trim_end_matches('/');
        let (dir, name) = match trimmed.rfind('/') {
            Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
            None => return Err(Errno::ENOENT.into()),
        };
        let dir = if dir.is_empty() { "/" } else { dir };
        let (d, dpath) = self.resolve(dir, true, 0)?;
        if !matches!(self.nodes[d].kind, Kind::Dir(_)) {
            return Err(Errno::ENOTDIR.into());
        }
        Ok((d, dpath, name.to_string()))
    }

    /// The directory entry for a path: (directory, stored name, node).
    fn entry(&self, path: &str) -> Result<(usize, String, usize), SysError> {
        let (d, dpath, name) = self.parent_of(path)?;
        let (i, stored) = self
            .lookup(d, &dpath, &name)
            .ok_or(SysError::new(Errno::ENOENT))?;
        Ok((d, stored, i))
    }

    fn remove_entry(&mut self, dir: usize, name: &str) {
        if let Kind::Dir(entries) = &mut self.nodes[dir].kind {
            entries.remove(name);
        }
    }

    fn add(&mut self, dir: usize, name: &str, kind: Kind, mode: u32, mtime: i64) -> usize {
        let i = self.nodes.len();
        self.nodes.push(Node {
            kind,
            mode,
            mtime,
            parent: dir,
            uid: MEM_UID,
            gid: 0,
        });
        if let Kind::Dir(entries) = &mut self.nodes[dir].kind {
            entries.insert(name.to_string(), i);
        }
        i
    }

    /// How many directory entries name a node.
    fn links_to(&self, i: usize) -> u32 {
        let n = self
            .nodes
            .iter()
            .filter_map(|n| match &n.kind {
                Kind::Dir(e) => Some(e.values().filter(|&&v| v == i).count()),
                _ => None,
            })
            .sum::<usize>();
        u32::try_from(n.max(1)).unwrap_or(u32::MAX)
    }

    fn stat_of(&self, i: usize) -> Stat {
        let n = &self.nodes[i];
        let (ftype, size) = match &n.kind {
            Kind::Dir(e) => (FileType::Dir, e.len() as u64),
            Kind::File(d) => (FileType::File, lock(d).len() as u64),
            Kind::Symlink(t) => (FileType::Symlink, t.len() as u64),
            Kind::Null => (FileType::Char, 0),
            Kind::Object(n) => (FileType::Other, *n),
        };
        let nlink = match &n.kind {
            Kind::Dir(e) => {
                let sub = e
                    .values()
                    .filter(|&&v| matches!(self.nodes[v].kind, Kind::Dir(_)))
                    .count();
                u32::try_from(2 + sub).unwrap_or(u32::MAX)
            }
            _ => self.links_to(i),
        };
        Stat {
            ftype,
            mode: n.mode,
            size,
            uid: n.uid,
            gid: n.gid,
            mtime: n.mtime,
            ino: i as u64 + 1,
            dev: 1,
            nlink,
            blocks: size.div_ceil(512),
        }
    }
}

fn join(dir: &str, name: &str) -> String {
    crate::pattern::path::join(dir, name)
}

fn parent_path(p: &str) -> String {
    match p.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => p[..i].to_string(),
    }
}

/// A file opened in memory.
struct MemFile {
    data: Arc<Mutex<Vec<u8>>>,
    pos: Mutex<usize>,
    read: bool,
    write: bool,
    append: bool,
}

impl Stream for MemFile {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.read {
            return Err(io::Error::from(SysError::new(Errno::EBADF)));
        }
        let data = lock(&self.data);
        let mut pos = lock(&self.pos);
        let start = (*pos).min(data.len());
        let n = (data.len() - start).min(buf.len());
        buf[..n].copy_from_slice(&data[start..start + n]);
        *pos = start + n;
        Ok(n)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if !self.write {
            return Err(io::Error::from(SysError::new(Errno::EBADF)));
        }
        let mut data = lock(&self.data);
        let mut pos = lock(&self.pos);
        if self.append {
            *pos = data.len();
        }
        let end = *pos + buf.len();
        if data.len() < end {
            data.resize(end, 0);
        }
        data[*pos..end].copy_from_slice(buf);
        *pos = end;
        Ok(buf.len())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A child running on a thread.
pub struct ThreadChild {
    pid: i32,
    handle: Option<JoinHandle<i32>>,
    done: Option<WaitStatus>,
}

impl ThreadChild {
    /// Run `body` on a new thread as process `pid`.
    pub fn start(
        pid: i32,
        name: &str,
        body: Box<dyn FnOnce() -> i32 + Send>,
    ) -> Result<Self, SysError> {
        let handle = std::thread::Builder::new()
            .name(name.to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(body)
            .map_err(|_| SysError::new(Errno::EAGAIN))?;
        Ok(Self {
            pid,
            handle: Some(handle),
            done: None,
        })
    }

    fn finish(&mut self) -> WaitStatus {
        if let Some(h) = self.handle.take() {
            let st = match h.join() {
                Ok(n) => WaitStatus::Exited(n),
                Err(_) => WaitStatus::Exited(255),
            };
            self.done = Some(st);
        }
        self.done.unwrap_or(WaitStatus::Exited(0))
    }
}

impl Child for ThreadChild {
    fn pid(&self) -> i32 {
        self.pid
    }

    fn wait(&mut self) -> Result<WaitStatus, SysError> {
        Ok(self.finish())
    }

    fn try_wait(&mut self) -> Result<Option<WaitStatus>, SysError> {
        match &self.handle {
            Some(h) if !h.is_finished() => Ok(None),
            _ => Ok(Some(self.finish())),
        }
    }
}

/// Run a utility for a spawn request on a thread.
pub fn start_utility(
    sys: Arc<dyn Sys>,
    pid: i32,
    util: Arc<dyn Utility>,
    req: SpawnRequest,
) -> Result<Box<dyn Child>, SysError> {
    let name = req.argv.first().cloned().unwrap_or_default();
    let body = Box::new(move || {
        let mut cx = UtilCtx {
            argv: req.argv,
            env: req.env,
            cwd: req.cwd,
            umask: req.umask,
            fds: req.fds.into_iter().collect(),
            sys: &*sys,
        };
        util.run(&mut cx)
    });
    Ok(Box::new(ThreadChild::start(pid, &name, body)?))
}

struct Inner {
    fs: Mutex<Fs>,
    utils: Mutex<Registry>,
    env: Mutex<Vec<(String, String)>>,
    cwd: Mutex<String>,
    homes: Mutex<BTreeMap<String, String>>,
    next_pid: AtomicI32,
    signals: Mutex<HashMap<i32, Vec<i32>>>,
    actions: Mutex<HashMap<(i32, i32), SigAction>>,
    clock: Mutex<Duration>,
    limits: Mutex<HashMap<(u8, bool), Option<u64>>>,
    spawned: Mutex<Vec<Vec<String>>>,
    users: Mutex<BTreeMap<u32, String>>,
    groups: Mutex<BTreeMap<u32, String>>,
    fsinfo: Mutex<FsInfo>,
    procs: Mutex<Vec<ProcInfo>>,
    nodename: Mutex<String>,
}

/// The in-memory system.
#[derive(Clone)]
pub struct MemSys(Arc<Inner>);

impl Default for MemSys {
    fn default() -> Self {
        Self::new()
    }
}

/// The shell's own process ID in a `MemSys`.
pub const MEM_PID: i32 = 100;

impl MemSys {
    /// A system with `/`, `/dev/null`, `/tmp`, `/home/QSHUSER` (the
    /// home of `QSHUSER`), `/usr/bin` and `/QOpenSys`, working directory
    /// `/home/QSHUSER`, `HOME` and `LOGNAME` set.
    pub fn new() -> Self {
        let s = Self(Arc::new(Inner {
            fs: Mutex::new(Fs::new()),
            utils: Mutex::new(Registry::new()),
            env: Mutex::new(vec![
                ("HOME".to_string(), "/home/QSHUSER".to_string()),
                ("LOGNAME".to_string(), "QSHUSER".to_string()),
            ]),
            cwd: Mutex::new("/home/QSHUSER".to_string()),
            homes: Mutex::new(BTreeMap::from([(
                "QSHUSER".to_string(),
                "/home/QSHUSER".to_string(),
            )])),
            next_pid: AtomicI32::new(1000),
            signals: Mutex::new(HashMap::new()),
            actions: Mutex::new(HashMap::new()),
            clock: Mutex::new(Duration::from_secs(1_790_000_000)),
            limits: Mutex::new(HashMap::new()),
            spawned: Mutex::new(Vec::new()),
            users: Mutex::new(BTreeMap::from([
                (0, "QSYS".to_string()),
                (MEM_UID, "QSHUSER".to_string()),
            ])),
            groups: Mutex::new(BTreeMap::from([(50, "QSHGRP".to_string())])),
            fsinfo: Mutex::new(FsInfo {
                name: "/dev/QASP01".to_string(),
                mounted_on: "/".to_string(),
                block_size: 4096,
                blocks: 1_000_000,
                free: 600_000,
                avail: 600_000,
            }),
            procs: Mutex::new(vec![ProcInfo {
                pid: MEM_PID,
                ppid: 1,
                pgid: MEM_PID,
                uid: MEM_UID,
                job: "000123/QSHUSER/QZSHSH".to_string(),
                status: "RUN".to_string(),
                function: "PGM-QZSHSH".to_string(),
                device: "-".to_string(),
                cpu: Duration::from_millis(1250),
                args: vec!["qsh".to_string()],
            }]),
            nodename: Mutex::new("QS400".to_string()),
        }));
        for d in [
            "/dev",
            "/tmp",
            "/home/QSHUSER",
            "/usr/bin",
            "/QOpenSys/usr/bin",
            "/QSYS.LIB",
        ] {
            s.mkdir_p(d);
        }
        {
            let mut fs = lock(&s.0.fs);
            let (dev, _) = fs.resolve("/dev", true, 0).expect("dev");
            fs.add(dev, "null", Kind::Null, 0o666, 0);
        }
        s
    }

    fn now_secs(&self) -> i64 {
        i64::try_from(lock(&self.0.clock).as_secs()).unwrap_or(0)
    }

    /// Create a directory and its parents.
    pub fn mkdir_p(&self, path: &str) {
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let mut cur = 0usize;
        let mut cur_path = "/".to_string();
        for comp in path.split('/').filter(|c| !c.is_empty()) {
            cur = match fs.lookup(cur, &cur_path, comp) {
                Some((i, name)) => {
                    cur_path = join(&cur_path, &name);
                    i
                }
                None => {
                    let i = fs.add(cur, comp, Kind::Dir(BTreeMap::new()), 0o755, t);
                    cur_path = join(&cur_path, comp);
                    i
                }
            };
        }
    }

    /// Create or replace a file, making its directory.
    pub fn write_file(&self, path: &str, data: impl AsRef<[u8]>) {
        self.mkdir_p(&parent_path(path));
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let (d, dpath, name) = fs.parent_of(path).expect("parent");
        match fs.lookup(d, &dpath, &name) {
            Some((i, _)) if matches!(fs.nodes[i].kind, Kind::File(_)) => {
                if let Kind::File(f) = &fs.nodes[i].kind {
                    *lock(f) = data.as_ref().to_vec();
                }
                fs.nodes[i].mtime = t;
            }
            _ => {
                fs.add(
                    d,
                    &name,
                    Kind::File(Arc::new(Mutex::new(data.as_ref().to_vec()))),
                    0o644,
                    t,
                );
            }
        }
    }

    /// A file's contents.
    pub fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        let fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0).ok()?;
        match &fs.nodes[i].kind {
            Kind::File(f) => Some(lock(f).clone()),
            _ => None,
        }
    }

    /// Set a path's permission bits.
    pub fn set_mode(&self, path: &str, mode: u32) {
        let mut fs = lock(&self.0.fs);
        if let Ok((i, _)) = fs.resolve(path, true, 0) {
            fs.nodes[i].mode = mode;
        }
    }

    /// Set a path's modification time.
    pub fn set_mtime(&self, path: &str, mtime: i64) {
        let mut fs = lock(&self.0.fs);
        if let Ok((i, _)) = fs.resolve(path, true, 0) {
            fs.nodes[i].mtime = mtime;
        }
    }

    /// Make a symbolic link at `path` pointing to `target`.
    pub fn make_symlink(&self, target: &str, path: &str) {
        self.mkdir_p(&parent_path(path));
        let mut fs = lock(&self.0.fs);
        let (d, _, name) = fs.parent_of(path).expect("parent");
        fs.add(d, &name, Kind::Symlink(target.to_string()), 0o777, 0);
    }

    /// Make a `/QSYS.LIB` object (`*PGM`, `*DTAARA`, ...) of a size: it
    /// reports type `Other` and cannot be opened.
    pub fn make_object(&self, path: &str, size: u64) {
        self.mkdir_p(&parent_path(path));
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let (d, _, name) = fs.parent_of(path).expect("parent");
        let i = fs.add(d, &name, Kind::Object(size), 0o700, t);
        fs.nodes[i].uid = 0;
    }

    /// Register a utility at a path; the path becomes an executable file.
    pub fn register(&self, path: &str, u: Arc<dyn Utility>) {
        self.write_file(path, b"");
        self.set_mode(path, 0o755);
        lock(&self.0.utils).add(path, u);
    }

    /// Set a variable in the starting environment.
    pub fn set_env(&self, name: &str, value: &str) {
        let mut env = lock(&self.0.env);
        env.retain(|(k, _)| k != name);
        env.push((name.to_string(), value.to_string()));
    }

    /// Remove a variable from the starting environment.
    pub fn unset_env(&self, name: &str) {
        lock(&self.0.env).retain(|(k, _)| k != name);
    }

    /// Set the starting working directory (created if missing).
    pub fn set_cwd(&self, path: &str) {
        self.mkdir_p(path);
        *lock(&self.0.cwd) = path.to_string();
    }

    /// Set a user's home directory.
    pub fn set_home(&self, user: &str, dir: &str) {
        lock(&self.0.homes).insert(user.to_string(), dir.to_string());
    }

    /// Move the clock.
    pub fn advance(&self, d: Duration) {
        *lock(&self.0.clock) += d;
    }

    /// The argument lists of every program spawned so far.
    pub fn spawned(&self) -> Vec<Vec<String>> {
        lock(&self.0.spawned).clone()
    }

    /// The action last set for a signal of a process.
    pub fn action(&self, pid: i32, sig: i32) -> SigAction {
        lock(&self.0.actions)
            .get(&(pid, sig))
            .copied()
            .unwrap_or(SigAction::Default)
    }

    /// Deliver a signal to a process as the system would: recorded when
    /// caught.
    pub fn raise(&self, pid: i32, sig: i32) {
        lock(&self.0.signals).entry(pid).or_default().push(sig);
    }

    /// Name a user ID.
    pub fn set_user(&self, uid: u32, name: &str) {
        lock(&self.0.users).insert(uid, name.to_string());
    }

    /// Name a group ID.
    pub fn set_group(&self, gid: u32, name: &str) {
        lock(&self.0.groups).insert(gid, name.to_string());
    }

    /// Set a path's owner and group.
    pub fn set_owner(&self, path: &str, uid: u32, gid: u32) {
        let mut fs = lock(&self.0.fs);
        if let Ok((i, _)) = fs.resolve(path, true, 0) {
            fs.nodes[i].uid = uid;
            fs.nodes[i].gid = gid;
        }
    }

    /// Set what `statfs` reports.
    pub fn set_fsinfo(&self, info: FsInfo) {
        *lock(&self.0.fsinfo) = info;
    }

    /// Add a process for `ps`.
    pub fn add_process(&self, p: ProcInfo) {
        lock(&self.0.procs).push(p);
    }

    /// Set the system's name.
    pub fn set_nodename(&self, name: &str) {
        *lock(&self.0.nodename) = name.to_string();
    }

    fn alloc_pid(&self) -> i32 {
        self.0.next_pid.fetch_add(1, Ordering::Relaxed)
    }
}

fn limit_key(l: Limit) -> u8 {
    match l {
        Limit::Core => 0,
        Limit::Data => 1,
        Limit::FileSize => 2,
        Limit::OpenFiles => 3,
        Limit::Stack => 4,
        Limit::CpuTime => 5,
        Limit::Memory => 6,
    }
}

impl Sys for MemSys {
    fn environ(&self) -> Vec<(String, String)> {
        lock(&self.0.env).clone()
    }

    fn initial_cwd(&self) -> String {
        lock(&self.0.cwd).clone()
    }

    fn pid(&self) -> i32 {
        MEM_PID
    }

    fn ppid(&self) -> i32 {
        1
    }

    fn home_of(&self, user: &str) -> Option<String> {
        let homes = lock(&self.0.homes);
        homes
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(user))
            .map(|(_, v)| v.clone())
    }

    fn open(&self, path: &str, flags: OpenFlags, mode: u32) -> Result<Fd, SysError> {
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let node = match fs.resolve(path, true, 0) {
            Ok((i, _)) => {
                if flags.create && flags.excl {
                    return Err(Errno::EEXIST.into());
                }
                i
            }
            Err(e) if e.errno == Errno::ENOENT && flags.create => {
                let (d, _, name) = fs.parent_of(path)?;
                if fs.nodes[d].mode & 0o200 == 0 {
                    return Err(Errno::EACCES.into());
                }
                fs.add(
                    d,
                    &name,
                    Kind::File(Arc::new(Mutex::new(Vec::new()))),
                    mode & 0o7777,
                    t,
                )
            }
            Err(e) => return Err(e),
        };
        let n = &fs.nodes[node];
        if (flags.read && n.mode & 0o400 == 0) || (flags.write && n.mode & 0o200 == 0) {
            return Err(Errno::EACCES.into());
        }
        match &n.kind {
            Kind::Null => Ok(Null::fd()),
            Kind::Dir(_) if flags.write => Err(Errno::EISDIR.into()),
            Kind::Dir(_) => Ok(Arc::new(MemFile {
                data: Arc::new(Mutex::new(Vec::new())),
                pos: Mutex::new(0),
                read: flags.read,
                write: false,
                append: false,
            })),
            Kind::File(data) => {
                let data = Arc::clone(data);
                if flags.truncate && flags.write {
                    lock(&data).clear();
                    fs.nodes[node].mtime = t;
                }
                Ok(Arc::new(MemFile {
                    data,
                    pos: Mutex::new(0),
                    read: flags.read,
                    write: flags.write,
                    append: flags.append,
                }))
            }
            Kind::Symlink(_) => Err(Errno::ELOOP.into()),
            Kind::Object(_) => Err(Errno::EOPNOTSUPP.into()),
        }
    }

    fn stat(&self, path: &str) -> Result<Stat, SysError> {
        let fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0)?;
        Ok(fs.stat_of(i))
    }

    fn lstat(&self, path: &str) -> Result<Stat, SysError> {
        let fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, false, 0)?;
        Ok(fs.stat_of(i))
    }

    fn read_dir(&self, path: &str) -> Result<Vec<String>, SysError> {
        let fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0)?;
        match &fs.nodes[i].kind {
            Kind::Dir(e) => {
                if fs.nodes[i].mode & 0o400 == 0 {
                    return Err(Errno::EACCES.into());
                }
                Ok(e.keys().cloned().collect())
            }
            _ => Err(Errno::ENOTDIR.into()),
        }
    }

    fn access(&self, path: &str, how: Access) -> Result<(), SysError> {
        let st = self.stat(path)?;
        let bit = match how {
            Access::Exists => return Ok(()),
            Access::Read => 0o444,
            Access::Write => 0o222,
            Access::Exec => 0o111,
        };
        if st.mode & bit == 0 {
            Err(Errno::EACCES.into())
        } else {
            Ok(())
        }
    }

    fn realpath(&self, path: &str) -> Result<String, SysError> {
        let fs = lock(&self.0.fs);
        fs.resolve(path, true, 0).map(|(_, p)| p)
    }

    fn case_insensitive(&self, dir: &str) -> bool {
        icase_path(dir)
    }

    fn mkdir(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let (d, dpath, name) = fs.parent_of(path)?;
        if fs.lookup(d, &dpath, &name).is_some() {
            return Err(Errno::EEXIST.into());
        }
        fs.add(d, &name, Kind::Dir(BTreeMap::new()), mode & 0o7777, t);
        Ok(())
    }

    fn rmdir(&self, path: &str) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (d, name, i) = fs.entry(path)?;
        match &fs.nodes[i].kind {
            Kind::Dir(e) if e.is_empty() => {}
            Kind::Dir(_) => return Err(Errno::ENOTEMPTY.into()),
            _ => return Err(Errno::ENOTDIR.into()),
        }
        fs.remove_entry(d, &name);
        Ok(())
    }

    fn unlink(&self, path: &str) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (d, name, i) = fs.entry(path)?;
        if matches!(fs.nodes[i].kind, Kind::Dir(_)) {
            return Err(Errno::EISDIR.into());
        }
        fs.remove_entry(d, &name);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (fd, fname, node) = fs.entry(from)?;
        let (td, tdpath, tname) = fs.parent_of(to)?;
        if let Some((old, stored)) = fs.lookup(td, &tdpath, &tname) {
            if old == node {
                return Ok(());
            }
            if let Kind::Dir(e) = &fs.nodes[old].kind
                && !e.is_empty()
            {
                return Err(Errno::ENOTEMPTY.into());
            }
            fs.remove_entry(td, &stored);
        }
        fs.remove_entry(fd, &fname);
        if let Kind::Dir(entries) = &mut fs.nodes[td].kind {
            entries.insert(tname, node);
        }
        fs.nodes[node].parent = td;
        Ok(())
    }

    fn link(&self, existing: &str, new: &str) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(existing, true, 0)?;
        if matches!(fs.nodes[i].kind, Kind::Dir(_)) {
            return Err(Errno::EPERM.into());
        }
        let (d, dpath, name) = fs.parent_of(new)?;
        if fs.lookup(d, &dpath, &name).is_some() {
            return Err(Errno::EEXIST.into());
        }
        if let Kind::Dir(entries) = &mut fs.nodes[d].kind {
            entries.insert(name, i);
        }
        Ok(())
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), SysError> {
        let t = self.now_secs();
        let mut fs = lock(&self.0.fs);
        let (d, dpath, name) = fs.parent_of(path)?;
        if fs.lookup(d, &dpath, &name).is_some() {
            return Err(Errno::EEXIST.into());
        }
        fs.add(d, &name, Kind::Symlink(target.to_string()), 0o777, t);
        Ok(())
    }

    fn readlink(&self, path: &str) -> Result<String, SysError> {
        let fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, false, 0)?;
        match &fs.nodes[i].kind {
            Kind::Symlink(t) => Ok(t.clone()),
            _ => Err(Errno::EINVAL.into()),
        }
    }

    fn chmod(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0)?;
        fs.nodes[i].mode = mode & 0o7777;
        Ok(())
    }

    fn utimes(&self, path: &str, mtime: i64) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0)?;
        fs.nodes[i].mtime = mtime;
        Ok(())
    }

    fn chown(&self, path: &str, uid: Option<u32>, gid: Option<u32>) -> Result<(), SysError> {
        let mut fs = lock(&self.0.fs);
        let (i, _) = fs.resolve(path, true, 0)?;
        if let Some(u) = uid {
            fs.nodes[i].uid = u;
        }
        if let Some(g) = gid {
            fs.nodes[i].gid = g;
        }
        Ok(())
    }

    fn statfs(&self, path: &str) -> Result<FsInfo, SysError> {
        self.stat(path)?;
        Ok(lock(&self.0.fsinfo).clone())
    }

    fn user_name(&self, uid: u32) -> Option<String> {
        lock(&self.0.users).get(&uid).cloned()
    }

    fn group_name(&self, gid: u32) -> Option<String> {
        lock(&self.0.groups).get(&gid).cloned()
    }

    fn user_id(&self, name: &str) -> Option<u32> {
        lock(&self.0.users)
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .map(|(k, _)| *k)
    }

    fn group_id(&self, name: &str) -> Option<u32> {
        lock(&self.0.groups)
            .iter()
            .find(|(_, v)| v.eq_ignore_ascii_case(name))
            .map(|(k, _)| *k)
    }

    fn processes(&self) -> Result<Vec<ProcInfo>, SysError> {
        Ok(lock(&self.0.procs).clone())
    }

    fn nodename(&self) -> String {
        lock(&self.0.nodename).clone()
    }

    fn sleep(&self, d: Duration) {
        self.advance(d);
    }

    fn spawn(&self, req: SpawnRequest) -> Result<Box<dyn Child>, SysError> {
        let real = self.realpath(&req.path)?;
        lock(&self.0.spawned).push(req.argv.clone());
        let util = {
            let utils = lock(&self.0.utils);
            utils.get(&real).or_else(|| utils.get(&req.path))
        };
        if let Some(u) = util {
            let pid = self.alloc_pid();
            return start_utility(Arc::new(self.clone()), pid, u, req);
        }
        let st = self.stat(&real)?;
        if st.ftype == FileType::Dir || st.mode & 0o111 == 0 {
            return Err(Errno::EACCES.into());
        }
        Err(Errno::ENOEXEC.into())
    }

    fn fork(
        &self,
        name: &str,
        body: Box<dyn FnOnce() -> i32 + Send>,
    ) -> Result<Box<dyn Child>, SysError> {
        let pid = self.alloc_pid();
        Ok(Box::new(ThreadChild::start(pid, name, body)?))
    }

    fn kill(&self, pid: i32, sig: i32) -> Result<(), SysError> {
        if pid <= 0 {
            return Err(Errno::ESRCH.into());
        }
        if sig != 0 {
            self.raise(pid, sig);
        }
        Ok(())
    }

    fn take_signals(&self, pid: i32) -> Vec<i32> {
        lock(&self.0.signals).remove(&pid).unwrap_or_default()
    }

    fn signal_action(&self, pid: i32, sig: i32, action: SigAction) {
        lock(&self.0.actions).insert((pid, sig), action);
    }

    fn now(&self) -> Duration {
        *lock(&self.0.clock)
    }

    fn get_limit(&self, limit: Limit, hard: bool) -> Result<Option<u64>, SysError> {
        Ok(lock(&self.0.limits)
            .get(&(limit_key(limit), hard))
            .copied()
            .flatten())
    }

    fn set_limit(&self, limit: Limit, hard: bool, value: Option<u64>) -> Result<(), SysError> {
        lock(&self.0.limits).insert((limit_key(limit), hard), value);
        Ok(())
    }
}

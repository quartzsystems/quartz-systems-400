// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`JobSys`]: the `Sys` of a shell in a QS/400 job.
//!
//! - Files are the job's IFS ([`files`]); `/usr/bin` also shows every
//!   registered utility, as IBM's shows its links to `QSHELL`.
//! - Pipes are in-process; utilities and the shell itself
//!   (`/usr/bin/sh`, `/usr/bin/qsh`) run on threads of the job.
//! - A `/QSYS.LIB/LIB.LIB/PGM.PGM` path is called in the job with the
//!   arguments as parameters; an ELF file runs as a PASE program in a
//!   child job; anything else executable is a script (`ENOEXEC`).
//! - The CL, SQL and resolver hooks and the IBM i objects reach the
//!   job's [`Side`](crate::Side) through the broker ([`hooks`]).

pub mod files;
mod hooks;
mod procs;

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use qsh::sys::{
    Access, Child, ClRunner, Credentials, DataArea, DtaqOp, DtaqReply, Errno, Fd, FileType,
    FsInfo, LibEntry, LiblChange, Limit, OpenFlags, ProcInfo, Registry, Resolver, SigAction,
    SpawnRequest, SplfInfo, SplfNumber, Spooled, SqlRunner, Stat, Sys, SysError, Times,
};

use crate::broker::Broker;
use crate::side::Ids;
use files::{IfsFile, err, fsinfo_of, open_args, stat_of};

/// Where the utilities live.
pub const BIN: &str = "/usr/bin";
/// The shell's own paths: each runs a new shell.
pub const SHELLS: &[&str] = &["/usr/bin/qsh", "/usr/bin/sh"];

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a job's shells share.
struct Core {
    broker: Broker,
    utils: Registry,
    next_pid: AtomicI32,
    signals: Mutex<HashMap<i32, Vec<i32>>>,
    actions: Mutex<HashMap<(i32, i32), SigAction>>,
    limits: Mutex<HashMap<(u8, bool), Option<u64>>>,
    ids: OnceLock<Vec<Ids>>,
    started: Instant,
    nodename: String,
    hooks: hooks::Hooks,
}

/// The `Sys` of shells in one job. Cloning is cheap; a clone made for a
/// child shell has its own environment, directory and process ID.
#[derive(Clone)]
pub struct JobSys {
    core: Arc<Core>,
    env: Arc<Vec<(String, String)>>,
    cwd: Arc<String>,
    pid: i32,
    ppid: i32,
}

/// How a job's shells start.
#[derive(Clone, Debug, Default)]
pub struct Setup {
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory.
    pub cwd: String,
    /// The job's process ID (`$$` of its first shell).
    pub pid: i32,
    /// The system name (`uname -n`).
    pub nodename: String,
}

/// Process IDs of in-process children start here, above any Linux
/// process ID, so they never name a job.
const THREAD_PIDS: i32 = 4_200_000;

impl JobSys {
    /// A system over `broker`, with every utility of `qsh-utils`.
    #[must_use]
    pub fn new(broker: Broker, setup: Setup) -> Self {
        let mut utils = Registry::new();
        qsh_utils::register(&mut utils);
        let core = Core {
            hooks: hooks::Hooks::new(broker.clone()),
            broker,
            utils,
            next_pid: AtomicI32::new(THREAD_PIDS),
            signals: Mutex::new(HashMap::new()),
            actions: Mutex::new(HashMap::new()),
            limits: Mutex::new(HashMap::new()),
            ids: OnceLock::new(),
            started: Instant::now(),
            nodename: setup.nodename,
        };
        Self {
            core: Arc::new(core),
            env: Arc::new(setup.env),
            cwd: Arc::new(if setup.cwd.is_empty() {
                "/".into()
            } else {
                setup.cwd
            }),
            pid: setup.pid,
            ppid: 1,
        }
    }

    /// The broker its tasks go through.
    #[must_use]
    pub fn broker(&self) -> &Broker {
        &self.core.broker
    }

    /// A clone for a child shell: the environment, directory and a new
    /// process ID.
    #[must_use]
    pub fn child(&self, env: Vec<(String, String)>, cwd: &str, pid: i32) -> Self {
        Self {
            core: self.core.clone(),
            env: Arc::new(env),
            cwd: Arc::new(cwd.to_string()),
            pid,
            ppid: self.pid,
        }
    }

    fn alloc_pid(&self) -> i32 {
        self.core.next_pid.fetch_add(1, Ordering::Relaxed)
    }

    /// Run `f` against the job's IFS.
    fn ifs<T: Send + 'static>(
        &self,
        f: impl FnOnce(&dyn ifs::client::IfsClient) -> ifs::Result<T> + Send + 'static,
    ) -> Result<T, SysError> {
        self.core.broker.call(move |s| f(s.ifs()))?.map_err(err)
    }

    fn ids(&self) -> &[Ids] {
        self.core.ids.get_or_init(|| {
            self.core
                .broker
                .call(|s| s.profiles())
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default()
        })
    }

    /// The registered utility at a path, in any case outside `/QOpenSys`.
    fn utility_path(&self, path: &str) -> Option<String> {
        let low = path.to_ascii_lowercase();
        if !low.starts_with("/usr/bin/") {
            return None;
        }
        let known = self.core.utils.get(&low).is_some() || SHELLS.contains(&low.as_str());
        known.then_some(low)
    }

    fn synthetic(ftype: FileType, mode: u32) -> Stat {
        Stat {
            ftype,
            mode,
            size: 0,
            uid: 0,
            gid: 0,
            mtime: 0,
            ino: 0,
            dev: 1,
            nlink: 1,
            blocks: 0,
        }
    }

    /// `/usr` and `/usr/bin` exist for the utilities even when the IFS
    /// has no such directories.
    fn overlay_stat(&self, path: &str) -> Option<Stat> {
        let low = path.to_ascii_lowercase();
        let low = low.trim_end_matches('/');
        if low == "/usr" || low == BIN {
            return Some(Self::synthetic(FileType::Dir, 0o755));
        }
        self.utility_path(low)
            .map(|_| Self::synthetic(FileType::File, 0o755))
    }

    /// A resolved path with each name as its directory spells it, where
    /// names are not case-sensitive.
    fn canonical_case(&self, path: &str) -> String {
        let mut out = String::new();
        for part in path.split('/').filter(|p| !p.is_empty()) {
            let dir = if out.is_empty() { "/".to_string() } else { out.clone() };
            let name = if self.case_insensitive(&dir) {
                self.read_dir(&dir)
                    .ok()
                    .and_then(|names| {
                        names
                            .iter()
                            .find(|n| n.as_str() == part)
                            .or_else(|| names.iter().find(|n| n.eq_ignore_ascii_case(part)))
                            .cloned()
                    })
                    .unwrap_or_else(|| part.to_string())
            } else {
                part.to_string()
            };
            out.push('/');
            out.push_str(&name);
        }
        if out.is_empty() { "/".into() } else { out }
    }

    fn stat_with(&self, path: &str, follow: bool) -> Result<Stat, SysError> {
        let p = path.to_string();
        match self.ifs(move |c| c.stat(&p, follow)) {
            Ok(s) => Ok(stat_of(&s)),
            Err(e) if e.errno == Errno::ENOENT => self.overlay_stat(path).ok_or(e),
            Err(e) => Err(e),
        }
    }
}

impl Sys for JobSys {
    fn environ(&self) -> Vec<(String, String)> {
        (*self.env).clone()
    }

    fn initial_cwd(&self) -> String {
        (*self.cwd).clone()
    }

    fn pid(&self) -> i32 {
        self.pid
    }

    fn ppid(&self) -> i32 {
        self.ppid
    }

    fn home_of(&self, user: &str) -> Option<String> {
        self.ids()
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(user))
            .map(|i| i.home.clone())
    }

    fn open(&self, path: &str, flags: OpenFlags, mode: u32) -> Result<Fd, SysError> {
        if path == "/dev/null" {
            return Ok(qsh::sys::Null::fd());
        }
        let p = path.to_string();
        let member = files::is_member(path);
        let mut args = open_args(flags, mode);
        if member {
            // Members are text: records as lines in ASCII.
            args.oflag |= ifs::ops::flags::O_TEXTDATA | ifs::ops::flags::O_CCSID;
            args.conversion_id = files::MEMBER_CCSID;
        }
        let fd = self.ifs(move |c| c.open(&p, args))?;
        let f = IfsFile::new(fd, self.core.broker.clone());
        if member && !flags.write {
            return Ok(Arc::new(files::Trimmed::new(f)));
        }
        Ok(Arc::new(f))
    }

    fn stat(&self, path: &str) -> Result<Stat, SysError> {
        self.stat_with(path, true)
    }

    fn lstat(&self, path: &str) -> Result<Stat, SysError> {
        self.stat_with(path, false)
    }

    fn read_dir(&self, path: &str) -> Result<Vec<String>, SysError> {
        let p = path.to_string();
        let low = path.to_ascii_lowercase();
        let low = low.trim_end_matches('/');
        let mut names = match self.ifs(move |c| c.read_dir(&p)) {
            Ok(v) => v.into_iter().map(|e| e.name).collect::<Vec<_>>(),
            Err(e) if e.errno == Errno::ENOENT && (low == "/usr" || low == BIN) => Vec::new(),
            Err(e) => return Err(e),
        };
        let add = |names: &mut Vec<String>, n: &str| {
            if !names.iter().any(|x| x.eq_ignore_ascii_case(n)) {
                names.push(n.to_string());
            }
        };
        if low == BIN {
            for p in self.core.utils.paths() {
                add(&mut names, p.rsplit('/').next().unwrap_or(p));
            }
            for p in SHELLS {
                add(&mut names, p.rsplit('/').next().unwrap_or(p));
            }
        } else if low == "/usr" {
            add(&mut names, "bin");
        } else if low.is_empty() {
            add(&mut names, "usr");
        }
        Ok(names)
    }

    fn access(&self, path: &str, how: Access) -> Result<(), SysError> {
        if path == "/dev/null" {
            return Ok(());
        }
        let amode = match how {
            Access::Exists => ifs::ops::flags::F_OK,
            Access::Read => ifs::ops::flags::R_OK,
            Access::Write => ifs::ops::flags::W_OK,
            Access::Exec => ifs::ops::flags::X_OK,
        };
        let p = path.to_string();
        match self.ifs(move |c| c.access(&p, amode)) {
            Err(e) if e.errno == Errno::ENOENT && self.overlay_stat(path).is_some() => {
                if how == Access::Write {
                    Err(Errno::EACCES.into())
                } else {
                    Ok(())
                }
            }
            r => r,
        }
    }

    fn realpath(&self, path: &str) -> Result<String, SysError> {
        let r = files::realpath(&|p| self.lstat(p), &|p| self.readlink(p), path)?;
        Ok(self.canonical_case(&r))
    }

    fn case_insensitive(&self, dir: &str) -> bool {
        let up = dir.to_ascii_uppercase();
        !(up == "/QOPENSYS" || up.starts_with("/QOPENSYS/"))
    }

    fn chdir(&self, path: &str) -> Result<(), SysError> {
        let st = self.stat(path)?;
        if st.ftype != FileType::Dir {
            return Err(Errno::ENOTDIR.into());
        }
        self.access(path, Access::Exec)
    }

    fn mkdir(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.mkdir(&p, mode))
    }

    fn rmdir(&self, path: &str) -> Result<(), SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.rmdir(&p))
    }

    fn unlink(&self, path: &str) -> Result<(), SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.unlink(&p))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), SysError> {
        let (a, b) = (from.to_string(), to.to_string());
        self.ifs(move |c| c.rename(&a, &b, ifs::ops::RenameMode::Unlink))
    }

    fn link(&self, existing: &str, new: &str) -> Result<(), SysError> {
        let (a, b) = (existing.to_string(), new.to_string());
        self.ifs(move |c| c.link(&a, &b))
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), SysError> {
        let (a, b) = (target.to_string(), path.to_string());
        self.ifs(move |c| c.symlink(&a, &b))
    }

    fn readlink(&self, path: &str) -> Result<String, SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.readlink(&p))
    }

    fn chmod(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.chmod(&p, mode))
    }

    fn chown(&self, path: &str, uid: Option<u32>, gid: Option<u32>) -> Result<(), SysError> {
        let st = self.stat(path)?;
        let (u, g) = (uid.unwrap_or(st.uid), gid.unwrap_or(st.gid));
        let p = path.to_string();
        self.ifs(move |c| c.chown(&p, u, g))
    }

    fn utimes(&self, path: &str, mtime: i64) -> Result<(), SysError> {
        let p = path.to_string();
        let t = mtime.saturating_mul(1_000_000_000);
        self.ifs(move |c| c.utime(&p, Some((t, t))))
    }

    fn statfs(&self, path: &str) -> Result<FsInfo, SysError> {
        let p = path.to_string();
        let v = self.ifs(move |c| c.statvfs(&p))?;
        Ok(fsinfo_of(&v, path))
    }

    fn user_name(&self, uid: u32) -> Option<String> {
        self.ids()
            .iter()
            .find(|i| i.uid == uid)
            .map(|i| i.name.clone())
    }

    fn group_name(&self, gid: u32) -> Option<String> {
        if gid == 0 {
            return None;
        }
        self.ids()
            .iter()
            .find(|i| i.uid == gid)
            .map(|i| i.name.clone())
    }

    fn user_id(&self, name: &str) -> Option<u32> {
        self.ids()
            .iter()
            .find(|i| i.name.eq_ignore_ascii_case(name))
            .map(|i| i.uid)
    }

    fn group_id(&self, name: &str) -> Option<u32> {
        self.user_id(name)
    }

    fn processes(&self) -> Result<Vec<ProcInfo>, SysError> {
        self.core.broker.call(|s| s.processes())?
    }

    fn nodename(&self) -> String {
        self.core.nodename.clone()
    }

    fn resolver(&self) -> Result<&dyn Resolver, SysError> {
        Ok(&self.core.hooks)
    }

    fn cl_runner(&self) -> Result<&dyn ClRunner, SysError> {
        Ok(&self.core.hooks)
    }

    fn sql_runner(&self) -> Result<&dyn SqlRunner, SysError> {
        Ok(&self.core.hooks)
    }

    fn credentials(&self) -> Result<Credentials, SysError> {
        self.core.broker.call(|s| s.credentials())?
    }

    fn mkfifo(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let p = path.to_string();
        self.ifs(move |c| c.mkfifo(&p, mode))
    }

    fn library_list(&self) -> Result<Vec<LibEntry>, SysError> {
        self.core.broker.call(|s| s.library_list())?
    }

    fn change_library_list(&self, change: &LiblChange) -> Result<(), SysError> {
        let c = change.clone();
        self.core.broker.call(move |s| s.change_library_list(&c))?
    }

    fn system_value(&self, name: &str) -> Result<String, SysError> {
        let n = name.to_string();
        self.core.broker.call(move |s| s.system_value(&n, false))?
    }

    fn network_attribute(&self, name: &str) -> Result<String, SysError> {
        let n = name.to_string();
        self.core.broker.call(move |s| s.system_value(&n, true))?
    }

    fn read_data_area(&self, name: &str) -> Result<DataArea, SysError> {
        let n = name.to_string();
        self.core.broker.call(move |s| s.read_data_area(&n))?
    }

    fn write_data_area(
        &self,
        name: &str,
        part: Option<(usize, usize)>,
        value: &str,
    ) -> Result<(), SysError> {
        let (n, v) = (name.to_string(), value.to_string());
        self.core
            .broker
            .call(move |s| s.write_data_area(&n, part, &v))?
    }

    fn data_queue(&self, path: &str, op: &DtaqOp) -> Result<DtaqReply, SysError> {
        let (p, o) = (path.to_string(), op.clone());
        self.core.broker.call(move |s| s.data_queue(&p, &o))?
    }

    fn spooled_files(&self, job: &str) -> Result<Vec<SplfInfo>, SysError> {
        let j = job.to_string();
        self.core.broker.call(move |s| s.spooled_files(&j))?
    }

    fn spooled_file(
        &self,
        job: &str,
        name: &str,
        number: SplfNumber,
    ) -> Result<Spooled, SysError> {
        let (j, n) = (job.to_string(), name.to_string());
        self.core
            .broker
            .call(move |s| s.spooled_file(&j, &n, number))?
    }

    fn spawn(&self, req: SpawnRequest) -> Result<Box<dyn Child>, SysError> {
        procs::spawn(self, req)
    }

    fn fork(
        &self,
        name: &str,
        body: Box<dyn FnOnce() -> i32 + Send>,
    ) -> Result<Box<dyn Child>, SysError> {
        let pid = self.alloc_pid();
        Ok(Box::new(qsh::sys::mem::ThreadChild::start(pid, name, body)?))
    }

    fn kill(&self, pid: i32, sig: i32) -> Result<(), SysError> {
        procs::kill(self, pid, sig)
    }

    fn take_signals(&self, pid: i32) -> Vec<i32> {
        lock(&self.core.signals).remove(&pid).unwrap_or_default()
    }

    fn signal_action(&self, pid: i32, sig: i32, action: SigAction) {
        lock(&self.core.actions).insert((pid, sig), action);
    }

    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }

    fn times(&self) -> Times {
        Times {
            user: self.core.started.elapsed(),
            ..Times::default()
        }
    }

    fn get_limit(&self, limit: Limit, hard: bool) -> Result<Option<u64>, SysError> {
        Ok(lock(&self.core.limits)
            .get(&(limit_code(limit), hard))
            .copied()
            .flatten())
    }

    fn set_limit(&self, limit: Limit, hard: bool, value: Option<u64>) -> Result<(), SysError> {
        lock(&self.core.limits).insert((limit_code(limit), hard), value);
        Ok(())
    }
}

fn limit_code(l: Limit) -> u8 {
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

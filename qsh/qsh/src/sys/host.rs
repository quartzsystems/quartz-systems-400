// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `Sys` over the host through `std`: the shell's `/` is a host
//! directory, utilities come from a registry and anything else runs as
//! a host process. For tests and host tools, never the image.

use std::any::Any;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::mem::{ThreadChild, start_utility};
use super::stream::{Fd, Null, Stream, read_to_end, write_all};
use super::util::{Registry, Utility};
use super::{
    Access, Child, Errno, FileType, OpenFlags, SpawnRequest, Stat, Sys, SysError, WaitStatus,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A host file opened by the shell.
pub struct HostFile(Mutex<File>);

impl HostFile {
    /// Another handle on the same open file, for a child process.
    pub fn try_clone(&self) -> io::Result<File> {
        lock(&self.0).try_clone()
    }
}

impl Stream for HostFile {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        lock(&self.0).read(buf)
    }

    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        lock(&self.0).write(buf)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct Inner {
    root: PathBuf,
    utils: Mutex<Registry>,
    signals: Mutex<HashMap<i32, Vec<i32>>>,
    next_pid: AtomicI32,
}

/// The host system, with the shell's `/` at a host directory.
#[derive(Clone)]
pub struct HostSys(Arc<Inner>);

impl HostSys {
    /// The shell's `/` is the host directory `root`.
    pub fn rooted(root: impl Into<PathBuf>) -> Self {
        Self(Arc::new(Inner {
            root: root.into(),
            utils: Mutex::new(Registry::new()),
            signals: Mutex::new(HashMap::new()),
            next_pid: AtomicI32::new(1000),
        }))
    }

    /// Register an in-process utility at a shell path.
    pub fn register(&self, path: &str, u: Arc<dyn Utility>) {
        lock(&self.0.utils).add(path, u);
    }

    /// The host path of a shell path.
    pub fn host_path(&self, path: &str) -> PathBuf {
        let mut p = self.0.root.clone();
        for c in path.split('/').filter(|c| !c.is_empty()) {
            p.push(c);
        }
        p
    }

    /// A registered utility looks like an executable file at its path.
    fn utility_stat(&self, path: &str) -> Option<Stat> {
        let utils = lock(&self.0.utils);
        utils.get(path).map(|_| Stat {
            ftype: FileType::File,
            mode: 0o755,
            size: 0,
            uid: 0,
            gid: 0,
            mtime: 0,
            ino: 0,
            dev: 0,
            nlink: 1,
            blocks: 0,
        })
    }

    fn shell_path(&self, host: &Path) -> Option<String> {
        let rel = host.strip_prefix(&self.0.root).ok()?;
        let mut s = String::new();
        for c in rel.components() {
            s.push('/');
            s.push_str(&c.as_os_str().to_string_lossy());
        }
        if s.is_empty() {
            s.push('/');
        }
        Some(s)
    }
}

fn stat_of(m: &fs::Metadata) -> Stat {
    let ft = m.file_type();
    let ftype = if ft.is_symlink() {
        FileType::Symlink
    } else if ft.is_dir() {
        FileType::Dir
    } else if ft.is_file() {
        FileType::File
    } else {
        FileType::Other
    };
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    #[cfg(unix)]
    let (mode, uid, gid, ino, dev, nlink, blocks) = {
        use std::os::unix::fs::MetadataExt;
        (
            m.mode() & 0o7777,
            m.uid(),
            m.gid(),
            m.ino(),
            m.dev(),
            u32::try_from(m.nlink()).unwrap_or(u32::MAX),
            m.blocks(),
        )
    };
    #[cfg(not(unix))]
    let (mode, uid, gid, ino, dev, nlink, blocks) = {
        let base = if m.permissions().readonly() {
            0o555
        } else {
            0o755
        };
        (
            if ft.is_dir() {
                base
            } else {
                base & !0o111 | 0o100
            },
            0,
            0,
            0,
            0,
            1,
            m.len().div_ceil(512),
        )
    };
    Stat {
        ftype,
        mode,
        size: m.len(),
        uid,
        gid,
        mtime,
        ino,
        dev,
        nlink,
        blocks,
    }
}

/// A host process and the threads copying its streams.
struct HostChild {
    child: std::process::Child,
    pumps: Vec<JoinHandle<()>>,
    pid: i32,
    done: Option<WaitStatus>,
}

fn exit_status(st: std::process::ExitStatus) -> WaitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = st.signal() {
            return WaitStatus::Signaled(sig);
        }
    }
    WaitStatus::Exited(st.code().unwrap_or(255))
}

impl Child for HostChild {
    fn pid(&self) -> i32 {
        self.pid
    }

    fn wait(&mut self) -> Result<WaitStatus, SysError> {
        if let Some(d) = self.done {
            return Ok(d);
        }
        let st = exit_status(self.child.wait()?);
        for p in self.pumps.drain(..) {
            let _ = p.join();
        }
        self.done = Some(st);
        Ok(st)
    }

    fn try_wait(&mut self) -> Result<Option<WaitStatus>, SysError> {
        if self.done.is_some() {
            return Ok(self.done);
        }
        match self.child.try_wait()? {
            Some(_) => self.wait().map(Some),
            None => Ok(None),
        }
    }
}

impl Sys for HostSys {
    fn environ(&self) -> Vec<(String, String)> {
        std::env::vars().collect()
    }

    fn initial_cwd(&self) -> String {
        std::env::current_dir()
            .ok()
            .and_then(|d| self.shell_path(&d))
            .unwrap_or_else(|| "/".to_string())
    }

    fn pid(&self) -> i32 {
        i32::try_from(std::process::id()).unwrap_or(1)
    }

    fn ppid(&self) -> i32 {
        1
    }

    fn home_of(&self, _user: &str) -> Option<String> {
        None
    }

    fn open(&self, path: &str, flags: OpenFlags, _mode: u32) -> Result<Fd, SysError> {
        let hp = self.host_path(path);
        if hp.is_dir() {
            if flags.write {
                return Err(Errno::EISDIR.into());
            }
            return Ok(Null::fd());
        }
        let f = OpenOptions::new()
            .read(flags.read)
            .write(flags.write && !flags.append)
            .append(flags.append)
            .create(flags.create && !flags.excl)
            .create_new(flags.create && flags.excl)
            .truncate(flags.truncate && !flags.append)
            .open(&hp)?;
        Ok(Arc::new(HostFile(Mutex::new(f))))
    }

    fn stat(&self, path: &str) -> Result<Stat, SysError> {
        if let Some(st) = self.utility_stat(path) {
            return Ok(st);
        }
        Ok(stat_of(&fs::metadata(self.host_path(path))?))
    }

    fn lstat(&self, path: &str) -> Result<Stat, SysError> {
        if let Some(st) = self.utility_stat(path) {
            return Ok(st);
        }
        Ok(stat_of(&fs::symlink_metadata(self.host_path(path))?))
    }

    fn read_dir(&self, path: &str) -> Result<Vec<String>, SysError> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.host_path(path))? {
            out.push(e?.file_name().to_string_lossy().into_owned());
        }
        Ok(out)
    }

    fn access(&self, path: &str, how: Access) -> Result<(), SysError> {
        let st = self.stat(path)?;
        let ok = match how {
            Access::Exists | Access::Read => true,
            Access::Write => st.mode & 0o222 != 0,
            Access::Exec => st.mode & 0o111 != 0,
        };
        if ok {
            Ok(())
        } else {
            Err(Errno::EACCES.into())
        }
    }

    fn realpath(&self, path: &str) -> Result<String, SysError> {
        let mut parts: Vec<&str> = Vec::new();
        for c in path.split('/').filter(|c| !c.is_empty()) {
            match c {
                "." => {}
                ".." => {
                    parts.pop();
                }
                _ => parts.push(c),
            }
        }
        let p = format!("/{}", parts.join("/"));
        self.stat(&p)?;
        Ok(p)
    }

    fn case_insensitive(&self, _dir: &str) -> bool {
        cfg!(any(windows, target_os = "macos"))
    }

    fn mkdir(&self, path: &str, _mode: u32) -> Result<(), SysError> {
        Ok(fs::create_dir(self.host_path(path))?)
    }

    fn rmdir(&self, path: &str) -> Result<(), SysError> {
        Ok(fs::remove_dir(self.host_path(path))?)
    }

    fn unlink(&self, path: &str) -> Result<(), SysError> {
        Ok(fs::remove_file(self.host_path(path))?)
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), SysError> {
        Ok(fs::rename(self.host_path(from), self.host_path(to))?)
    }

    fn link(&self, existing: &str, new: &str) -> Result<(), SysError> {
        Ok(fs::hard_link(
            self.host_path(existing),
            self.host_path(new),
        )?)
    }

    fn readlink(&self, path: &str) -> Result<String, SysError> {
        Ok(fs::read_link(self.host_path(path))?
            .to_string_lossy()
            .into_owned())
    }

    fn symlink(&self, target: &str, path: &str) -> Result<(), SysError> {
        let hp = self.host_path(path);
        #[cfg(unix)]
        {
            Ok(std::os::unix::fs::symlink(target, hp)?)
        }
        #[cfg(windows)]
        {
            let abs = if target.starts_with('/') {
                self.host_path(target)
            } else {
                hp.parent().map_or_else(PathBuf::new, Path::to_path_buf).join(target)
            };
            if abs.is_dir() {
                Ok(std::os::windows::fs::symlink_dir(target, hp)?)
            } else {
                Ok(std::os::windows::fs::symlink_file(target, hp)?)
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (target, hp);
            Err(Errno::ENOSYS.into())
        }
    }

    fn chmod(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let hp = self.host_path(path);
        #[cfg(unix)]
        let perm = std::os::unix::fs::PermissionsExt::from_mode(mode & 0o7777);
        #[cfg(not(unix))]
        let perm = {
            let mut p = fs::metadata(&hp)?.permissions();
            p.set_readonly(mode & 0o200 == 0);
            p
        };
        Ok(fs::set_permissions(hp, perm)?)
    }

    fn chown(&self, path: &str, uid: Option<u32>, gid: Option<u32>) -> Result<(), SysError> {
        let hp = self.host_path(path);
        #[cfg(unix)]
        {
            Ok(std::os::unix::fs::chown(hp, uid, gid)?)
        }
        #[cfg(not(unix))]
        {
            let _ = (hp, uid, gid);
            Err(Errno::ENOSYS.into())
        }
    }

    fn utimes(&self, path: &str, mtime: i64) -> Result<(), SysError> {
        let f = File::options().write(true).open(self.host_path(path))?;
        let t = UNIX_EPOCH + Duration::from_secs(u64::try_from(mtime).unwrap_or(0));
        Ok(f.set_modified(t)?)
    }

    fn spawn(&self, req: SpawnRequest) -> Result<Box<dyn Child>, SysError> {
        let util = lock(&self.0.utils).get(&req.path);
        if let Some(u) = util {
            let pid = self.0.next_pid.fetch_add(1, Ordering::Relaxed);
            return start_utility(Arc::new(self.clone()), pid, u, req);
        }
        let mut cmd = Command::new(self.host_path(&req.path));
        cmd.args(req.argv.iter().skip(1));
        cmd.env_clear().envs(req.env.iter().map(|(k, v)| (k, v)));
        cmd.current_dir(self.host_path(&req.cwd));
        let fd = |n: u32| {
            req.fds
                .iter()
                .find(|(k, _)| *k == n)
                .map(|(_, f)| Arc::clone(f))
        };
        let mut pump_in = None;
        let mut pump_out = Vec::new();
        for n in 0..3u32 {
            let stdio = match fd(n) {
                None => Stdio::null(),
                Some(f) if f.as_any().is::<Null>() => Stdio::null(),
                Some(f) => match f.as_any().downcast_ref::<HostFile>() {
                    Some(h) => Stdio::from(h.try_clone()?),
                    None => {
                        if n == 0 {
                            pump_in = Some(f);
                        } else {
                            pump_out.push((n, f));
                        }
                        Stdio::piped()
                    }
                },
            };
            match n {
                0 => cmd.stdin(stdio),
                1 => cmd.stdout(stdio),
                _ => cmd.stderr(stdio),
            };
        }
        let mut child = cmd.spawn().map_err(|e| match e.raw_os_error() {
            Some(8 | 193) => SysError::new(Errno::ENOEXEC),
            _ => SysError::from(e),
        })?;
        let mut pumps = Vec::new();
        if let (Some(src), Some(mut dst)) = (pump_in, child.stdin.take()) {
            // Not joined: the source may never end.
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = src.read(&mut buf) {
                    if n == 0 || dst.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
        for (n, dst) in pump_out {
            let src: Option<Box<dyn Read + Send>> = if n == 1 {
                child
                    .stdout
                    .take()
                    .map(|s| Box::new(s) as Box<dyn Read + Send>)
            } else {
                child
                    .stderr
                    .take()
                    .map(|s| Box::new(s) as Box<dyn Read + Send>)
            };
            if let Some(mut src) = src {
                pumps.push(std::thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    while let Ok(n) = src.read(&mut buf) {
                        if n == 0 || write_all(&*dst, &buf[..n]).is_err() {
                            break;
                        }
                    }
                }));
            }
        }
        let pid = i32::try_from(child.id()).unwrap_or(0);
        Ok(Box::new(HostChild {
            child,
            pumps,
            pid,
            done: None,
        }))
    }

    fn fork(
        &self,
        name: &str,
        body: Box<dyn FnOnce() -> i32 + Send>,
    ) -> Result<Box<dyn Child>, SysError> {
        let pid = self.0.next_pid.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(ThreadChild::start(pid, name, body)?))
    }

    fn kill(&self, pid: i32, sig: i32) -> Result<(), SysError> {
        if sig != 0 {
            lock(&self.0.signals).entry(pid).or_default().push(sig);
        }
        Ok(())
    }

    fn take_signals(&self, pid: i32) -> Vec<i32> {
        lock(&self.0.signals).remove(&pid).unwrap_or_default()
    }

    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
    }
}

/// Read a whole host-side stream (helper for tests).
pub fn slurp(fd: &Fd) -> Vec<u8> {
    read_to_end(&**fd).unwrap_or_default()
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Child jobs: what `spawn()` and a shell's programs start.
//!
//! A child job is a batch immediate job (type `B`, subtype `D`, shown as
//! `BCI`), named `QP0ZSPWN` as IBM names spawned jobs (the parent's name
//! with `SPAWN_SETJOBNAMEPARENT_NP`, the program's with
//! `SPAWN_SETJOBNAMEARGV_NP`). It is a `qsjob` process this job starts
//! with the role `SPAWN`, its request data the [`SpawnData`]; its
//! standard output and error are the descriptors the parent gives, its
//! standard input a descriptor whose number the data names (the control
//! channel is the process's standard input). It runs the shell, a
//! `*PGM` or a PASE program and ends with its exit status, which the
//! parent's `waitpid` sees; a signal (`kill`) reaches its process.
//!
//! [`start_shell`] is the entry for other jobs that want a shell (an SSH
//! session): `qsh` with arguments, as a user, over given descriptors.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Arc;

use ipc::chan::Chan;
use ipc::wire_struct;
use qsh::sys::{Child, Errno, Fd, Stream, SysError, WaitStatus};
use qsh_job::{ChildKind, JobSpawn};
use stg::{Client, Shared};
use sysapi::envproc::{Procs, SpawnReq, Var};
use wm::ctl::{FromJob, Start, ToJob};

use crate::console::host::Host;
use crate::console::Console;

/// The role of a child job.
pub const ROLE: &str = "SPAWN";
/// The name IBM gives spawned jobs.
pub const JOB_NAME: &str = "QP0ZSPWN";
/// `SPAWN_SETJOBNAMEPARENT_NP`.
pub const SETJOBNAMEPARENT: u32 = 0x80;
/// `SPAWN_SETJOBNAMEARGV_NP`.
pub const SETJOBNAMEARGV: u32 = 0x100;

/// What a child job runs, as its request data carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnData {
    /// 0 the shell, 1 a PASE program, 2 a `*PGM`.
    pub kind: u8,
    /// The program's path.
    pub path: String,
    /// Arguments, `argv[0]` first.
    pub argv: Vec<String>,
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory.
    pub cwd: String,
    /// The process's descriptor for standard input; -1 for none.
    pub stdin_fd: i32,
    /// `QIBM_MULTI_THREADED=Y` when it started: the job may run threads.
    pub threads: bool,
}

wire_struct!(SpawnData {
    kind,
    path,
    argv,
    env,
    cwd,
    stdin_fd,
    threads
});

fn kind_code(k: ChildKind) -> u8 {
    match k {
        ChildKind::Shell => 0,
        ChildKind::Pase => 1,
        ChildKind::Program => 2,
    }
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A started child job.
pub struct ChildJob {
    /// Its process.
    pub process: std::process::Child,
    /// The control channel to it: open while the parent keeps the
    /// child, for `ENDJOB`-style requests a caller may send.
    #[allow(dead_code)] // the SSH server's interface
    pub chan: Chan,
    /// Its job number.
    #[allow(dead_code)] // the SSH server's interface
    pub number: u32,
}

impl ChildJob {
    /// Its process ID.
    #[must_use]
    pub fn pid(&self) -> i32 {
        i32::try_from(self.process.id()).unwrap_or(0)
    }
}

/// How a child job's process ended, as `waitpid` reports it.
#[must_use]
pub fn wait_status(st: std::process::ExitStatus) -> WaitStatus {
    match (st.code(), st.signal()) {
        (Some(c), _) => WaitStatus::Exited(c),
        (None, Some(s)) => WaitStatus::Signaled(super::signals::from_linux(s)),
        _ => WaitStatus::Exited(255),
    }
}

/// The `<sys/wait.h>` encoding of a status.
#[must_use]
pub fn encoded(w: WaitStatus) -> i32 {
    match w {
        WaitStatus::Exited(c) => c & 0xff,
        WaitStatus::Signaled(s) => 0x0002_0000 | (s & 0xffff),
        WaitStatus::Stopped(s) => 0x0004_0000 | (s & 0xffff),
    }
}

/// Start a child job named `name` as `user` running `data`, its
/// standard input, output and error the descriptors given.
pub fn start(
    host: &Host,
    name: &str,
    user: &str,
    mut data: SpawnData,
    stdio: [OwnedFd; 3],
) -> Result<ChildJob, String> {
    let number = host
        .link
        .borrow_mut()
        .with(Client::job_number)
        .map_err(|e| e.clone())?;
    let [input, out, err] = stdio;
    // Standard input crosses as a numbered descriptor.
    rustix::io::fcntl_setfd(&input, rustix::io::FdFlags::empty()).map_err(|e| e.to_string())?;
    data.stdin_fd = input.as_raw_fd();
    let start = Start {
        name: name.to_string(),
        user: user.to_string(),
        number,
        handle: u64::from(number),
        kind: 'B',
        subsystem: host.start.subsystem.clone(),
        role: ROLE.into(),
        jobd: host.start.jobd.clone(),
        class: host.start.class.clone(),
        pool: host.start.pool.clone(),
        rqsdta: hex(&ipc::encode(&data)),
        ..Start::default()
    };
    let (mut parent, child) = Chan::pair().map_err(|e| e.to_string())?;
    let process = Command::new(crate::work::spawn::QSJOB)
        .env_clear()
        .stdin(Stdio::from(child.into_fd()))
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("{}: {e}", crate::work::spawn::QSJOB))?;
    drop(input);
    if !start.pool.is_empty() {
        crate::work::cgroup::place(&start.pool, process.id());
    }
    parent
        .send(&ToJob::Start {
            start: Box::new(start.clone()),
        })
        .map_err(|e| format!("starting {name}: {e}"))?;
    match parent.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => {}
        Ok(FromJob::Ending { reason }) => return Err(reason),
        Ok(other) => return Err(format!("{name}: unexpected {other:?}")),
        Err(e) => return Err(format!("{name}: {e}")),
    }
    Ok(ChildJob {
        process,
        chan: parent,
        number,
    })
}

/// A child job running `qsh` with `argv` (`argv[0]` first: `-c
/// command`, `-l` for a login shell, a script) as `user`, in `cwd` with
/// `env`, over the descriptors given. For an SSH session or any job that
/// wants a shell of its own; `wait` on the process gives the status.
#[allow(dead_code)] // the SSH server's entry
pub fn start_shell(
    host: &Host,
    user: &str,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    cwd: &str,
    stdio: [OwnedFd; 3],
) -> Result<ChildJob, String> {
    let data = SpawnData {
        kind: 0,
        path: "/usr/bin/qsh".into(),
        argv,
        threads: env.iter().any(|(k, v)| k == "QIBM_MULTI_THREADED" && v == "Y"),
        env,
        cwd: cwd.to_string(),
        stdin_fd: -1,
    };
    start(host, JOB_NAME, user, data, stdio)
}

/// Copy a stream into a writer until either ends.
fn pump_in(from: &Fd, mut to: std::io::PipeWriter) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if to.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
        }
    }
}

/// Copy a reader into a stream until either ends.
fn pump_out(mut from: std::io::PipeReader, to: Option<&Fd>) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if let Some(t) = to
                    && qsh::sys::stream::write_all(&**t, &buf[..n]).is_err()
                {
                    return;
                }
            }
        }
    }
}

/// A child job of a shell, its streams copied through pipes.
struct ShellChild {
    job: ChildJob,
    pumps: Vec<std::thread::JoinHandle<()>>,
    done: Option<WaitStatus>,
}

impl ShellChild {
    fn finish(&mut self, st: std::process::ExitStatus) -> WaitStatus {
        for p in self.pumps.drain(..) {
            let _ = p.join();
        }
        let w = wait_status(st);
        self.done = Some(w);
        w
    }
}

impl Child for ShellChild {
    fn pid(&self) -> i32 {
        self.job.pid()
    }

    fn wait(&mut self) -> Result<WaitStatus, SysError> {
        if let Some(d) = self.done {
            return Ok(d);
        }
        let st = self
            .job
            .process
            .wait()
            .map_err(|_| SysError::new(Errno::ECHILD))?;
        Ok(self.finish(st))
    }

    fn try_wait(&mut self) -> Result<Option<WaitStatus>, SysError> {
        if let Some(d) = self.done {
            return Ok(Some(d));
        }
        match self.job.process.try_wait() {
            Ok(Some(st)) => Ok(Some(self.finish(st))),
            Ok(None) => Ok(None),
            Err(_) => Err(SysError::new(Errno::ECHILD)),
        }
    }
}

/// A shell's program as a child job (`JobSys::spawn` of a PASE program).
pub fn spawn_for_shell(host: &Host, req: &JobSpawn) -> Result<Box<dyn Child>, SysError> {
    let fd = |n: u32| req.fds.iter().find(|(k, _)| *k == n).map(|(_, f)| f.clone());
    let io = |_| SysError::new(Errno::EIO);
    let (in_r, in_w) = std::io::pipe().map_err(io)?;
    let (out_r, out_w) = std::io::pipe().map_err(io)?;
    let (err_r, err_w) = std::io::pipe().map_err(io)?;
    let mut pumps = Vec::new();
    if let Some(i) = fd(0) {
        // Not joined: it ends when the child's input is closed.
        std::thread::spawn(move || pump_in(&i, in_w));
    } else {
        drop(in_w);
    }
    let o = fd(1);
    pumps.push(std::thread::spawn(move || pump_out(out_r, o.as_ref())));
    let e = fd(2);
    pumps.push(std::thread::spawn(move || pump_out(err_r, e.as_ref())));
    let data = SpawnData {
        kind: kind_code(req.kind),
        path: req.path.clone(),
        argv: req.argv.clone(),
        threads: req.env.iter().any(|(k, v)| k == "QIBM_MULTI_THREADED" && v == "Y"),
        env: req.env.clone(),
        cwd: req.cwd.clone(),
        stdin_fd: -1,
    };
    let job = start(
        host,
        JOB_NAME,
        &host.start.user,
        data,
        [in_r.into(), out_w.into(), err_w.into()],
    )
    .map_err(|_| SysError::new(Errno::EAGAIN))?;
    Ok(Box::new(ShellChild {
        job,
        pumps,
        done: None,
    }))
}

/// Send the system's signal `sig` to the job whose process is `pid`:
/// a job of the same user, or any with `*JOBCTL`. The `errno` on
/// failure.
pub fn signal(host: &Host, pid: i32, sig: i32) -> Result<(), i32> {
    let jobs = host.link.borrow_mut().with(Client::active_jobs);
    let Some(j) = jobs.iter().find(|j| i32::try_from(j.pid).ok() == Some(pid)) else {
        return Err(Errno::ESRCH.0);
    };
    if j.user != host.start.user {
        let me = host.link.borrow_mut().with(Client::whoami);
        if me.require(sec::profile::SpecialAuth::JOBCTL, "signal another user's job").is_err() {
            return Err(Errno::EPERM.0);
        }
    }
    if sig == 0 {
        return Ok(());
    }
    let linux = super::signals::to_linux(sig).ok_or(Errno::EINVAL.0)?;
    let target = rustix::process::Pid::from_raw(pid).ok_or(Errno::ESRCH.0)?;
    let s = rustix::process::Signal::from_named_raw(linux).ok_or(Errno::EINVAL.0)?;
    rustix::process::kill_process(target, s).map_err(|_| Errno::EPERM.0)
}

/// The job's children for the C-style `spawn` and `waitpid`.
pub struct JobProcs {
    host: Host,
    children: RefCell<BTreeMap<i32, ChildJob>>,
}

impl JobProcs {
    /// Over the job's surroundings.
    pub fn new(host: Host) -> Self {
        Self {
            host,
            children: RefCell::new(BTreeMap::new()),
        }
    }

    fn kind_of(&self, path: &str) -> Result<ChildKind, i32> {
        if qsh_job::sys::SHELLS.contains(&path.to_ascii_lowercase().as_str()) {
            return Ok(ChildKind::Shell);
        }
        if super::object_of(path, ".PGM").is_some() && path.to_ascii_uppercase().starts_with("/QSYS.LIB/") {
            return Ok(ChildKind::Program);
        }
        if super::pase::is_elf(&self.host, path) {
            return Ok(ChildKind::Pase);
        }
        Err(Errno::ENOENT.0)
    }

    fn resolve(&self, req: &SpawnReq, env: &[(String, String)]) -> Result<String, i32> {
        if !req.search || req.path.contains('/') {
            return Ok(req.path.clone());
        }
        let path = env
            .iter()
            .find(|(k, _)| k == "PATH")
            .map_or("/usr/bin", |(_, v)| v.as_str());
        for dir in path.split(':') {
            let cand = format!("{}/{}", dir.trim_end_matches('/'), req.path);
            if self.kind_of(&cand).is_ok() {
                return Ok(cand);
            }
        }
        Err(Errno::ENOENT.0)
    }
}

fn own(fd: impl AsFd) -> Result<OwnedFd, i32> {
    fd.as_fd().try_clone_to_owned().map_err(|_| Errno::EMFILE.0)
}

impl Procs for JobProcs {
    fn spawn(&self, req: SpawnReq) -> Result<i32, i32> {
        let env: Vec<(String, String)> = match &req.envp {
            Some(v) => v
                .iter()
                .map(|s| {
                    let (k, v) = sysapi::envproc::split(s);
                    (k, v.unwrap_or_default())
                })
                .collect(),
            None => sysapi::envproc::job_pairs(),
        };
        let path = self.resolve(&req, &env)?;
        let kind = self.kind_of(&path)?;
        let name = if req.flags & SETJOBNAMEPARENT != 0 {
            self.host.start.name.clone()
        } else if req.flags & SETJOBNAMEARGV != 0 {
            req.argv.first().map_or_else(
                || JOB_NAME.to_string(),
                |a| {
                    let base = a.rsplit('/').next().unwrap_or(a);
                    base.to_ascii_uppercase().chars().take(10).collect()
                },
            )
        } else {
            JOB_NAME.to_string()
        };
        // The child's input is the null device; its output and error the
        // job's own (descriptor passing of the map's other entries is
        // not done).
        let null = std::fs::File::open("/dev/null").map_err(|_| Errno::EIO.0)?;
        let stdio = [
            OwnedFd::from(null),
            own(std::io::stdout())?,
            own(std::io::stderr())?,
        ];
        let data = SpawnData {
            kind: kind_code(kind),
            path,
            argv: req.argv,
            threads: env.iter().any(|(k, v)| k == "QIBM_MULTI_THREADED" && v == "Y"),
            env,
            cwd: "/".into(),
            stdin_fd: -1,
        };
        let job = start(&self.host, &name, &self.host.start.user, data, stdio)
            .map_err(|_| Errno::EAGAIN.0)?;
        let pid = job.pid();
        self.children.borrow_mut().insert(pid, job);
        Ok(pid)
    }

    fn wait(&self, pid: i32, nohang: bool) -> Result<Option<(i32, i32)>, i32> {
        loop {
            let mut kids = self.children.borrow_mut();
            if kids.is_empty() || (pid > 0 && !kids.contains_key(&pid)) {
                return Err(Errno::ECHILD.0);
            }
            let pids: Vec<i32> = if pid > 0 {
                vec![pid]
            } else {
                kids.keys().copied().collect()
            };
            for p in pids {
                let Some(c) = kids.get_mut(&p) else { continue };
                if let Ok(Some(st)) = c.process.try_wait() {
                    kids.remove(&p);
                    return Ok(Some((p, encoded(wait_status(st)))));
                }
            }
            drop(kids);
            if nohang {
                return Ok(None);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn kill(&self, pid: i32, sig: i32) -> Result<(), i32> {
        signal(&self.host, pid, sig)
    }

    fn ids(&self) -> (i32, i32) {
        (
            i32::try_from(std::process::id()).unwrap_or(0),
            i32::try_from(std::os::unix::process::parent_id()).unwrap_or(1),
        )
    }

    fn run_pase(&self, path: &str, argv: Vec<String>, env: Vec<String>) -> Result<i32, i32> {
        let env = env
            .iter()
            .map(|s| {
                let (k, v) = sysapi::envproc::split(s);
                (k, v.unwrap_or_default())
            })
            .collect();
        super::pase::run_to_end(&self.host, path, argv, env, None)
            .map(|(code, _)| code)
            .map_err(|_| Errno::ENOENT.0)
    }
}

/// A standard stream of this process as a shell stream.
struct OsOut<W: Write + Send>(std::sync::Mutex<W>);

impl<W: Write + Send + 'static> Stream for OsOut<W> {
    fn read(&self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
    fn write(&self, buf: &[u8]) -> std::io::Result<usize> {
        let mut w = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        w.write_all(buf)?;
        w.flush()?;
        Ok(buf.len())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// An input file as a shell stream.
struct OsIn(std::sync::Mutex<std::fs::File>);

impl Stream for OsIn {
    fn read(&self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .read(buf)
    }
    fn write(&self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::PermissionDenied.into())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The `SPAWN` role: run what the request data names, end with its
/// status. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = ctl.try_clone().ok();
    let data: SpawnData = unhex(&start.rqsdta)
        .and_then(|b| ipc::decode(&b).ok())
        .unwrap_or_default();
    let mut c = Console::new(ctl, link, start);
    c.started();
    sysapi::envproc::seed(
        data.env
            .iter()
            .map(|(k, v)| Var {
                name: k.clone(),
                value: Some(v.clone()),
                ccsid: 0,
            })
            .collect(),
    );
    stg::remote::status(link, wm::job::Status::Run, &format!("PGM-{}", program_name(&data.path)));
    let input: Option<Fd> = (data.stdin_fd >= 0)
        .then(|| std::fs::File::open(format!("/proc/self/fd/{}", data.stdin_fd)).ok())
        .flatten()
        .map(|f| Arc::new(OsIn(std::sync::Mutex::new(f))) as Fd);
    let out: Fd = Arc::new(OsOut(std::sync::Mutex::new(std::io::stdout())));
    let err: Fd = Arc::new(OsOut(std::sync::Mutex::new(std::io::stderr())));
    let code = match data.kind {
        0 => {
            let host = c.host.clone();
            let mut setup = super::setup(&host, "PIPELINE");
            setup.env.clone_from(&data.env);
            if !data.cwd.is_empty() {
                setup.cwd.clone_from(&data.cwd);
            }
            let job = c.job();
            let mut side = super::side::JobSide::new(job, &host);
            qsh_job::run::invoke(&mut side, setup, data.argv.clone(), [input, Some(out), Some(err)])
        }
        2 => {
            let host = c.host.clone();
            let job = c.job();
            let mut side = super::side::JobSide::new(job, &host);
            let (lib, pgm) = super::object_of(&data.path, ".PGM").unwrap_or_default();
            let args: Vec<String> = data.argv.iter().skip(1).cloned().collect();
            match qsh_job::Side::call_program(&mut side, &lib, &pgm, &args) {
                Ok(r) => {
                    let _ = out.write(r.output.as_bytes());
                    r.status
                }
                Err(_) => 127,
            }
        }
        _ => {
            let host = c.host.clone();
            let fds = [data.stdin_fd, 1, 2];
            match super::pase::run_with_fds(&host, &data.path, data.argv.clone(), data.env.clone(), &data.cwd, fds) {
                Ok(code) => code,
                Err(e) => {
                    let _ = err.write(format!("{e}\n").as_bytes());
                    127
                }
            }
        }
    };
    c.ended(u16::try_from(code.clamp(0, 255)).unwrap_or(0));
    if let Some(mut e) = ending {
        let _ = e.send(&FromJob::Ending {
            reason: format!("ended, status {code}"),
        });
    }
    let _ = std::io::stdout().flush();
    std::process::exit(code & 0xff)
}

fn program_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase()
        .chars()
        .take(10)
        .collect()
}

/// Install the job's `spawn()` keeper and environment keeper.
pub fn install(host: &Host) {
    sysapi::envproc::install_procs(Rc::new(JobProcs::new(host.clone())));
    sysapi::envproc::install_system(Rc::new(super::env::LinkEnv::new(host.link.clone())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_through_request_data() {
        let d = SpawnData {
            kind: 1,
            path: "/QOpenSys/tmp/x".into(),
            argv: vec!["x".into(), "a b".into()],
            env: vec![("A".into(), "1".into())],
            cwd: "/home".into(),
            stdin_fd: 7,
            threads: true,
        };
        let back: SpawnData = ipc::decode(&unhex(&hex(&ipc::encode(&d))).unwrap()).unwrap();
        assert_eq!(back, d);
        assert_eq!(encoded(WaitStatus::Exited(3)), 3);
        assert_eq!(encoded(WaitStatus::Signaled(6)), 0x0002_0006);
        assert_eq!(program_name("/QSYS.LIB/QGPL.LIB/HELLO.PGM"), "HELLO");
    }
}

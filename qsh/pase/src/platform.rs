// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting a PASE process: the one place this crate calls the system
//! directly (`unsafe`).
//!
//! The job forks a keeper (`C`), which unshares its user, mount and PID
//! namespaces and waits; the job writes the keeper's UID and GID maps
//! (the profile's IDs map to themselves) and lets it go on. The keeper
//! forks the program's process (`G`, PID 1 of the new namespace), which
//! builds the view ([`crate::view`]), pivots its root there, drops to
//! the profile's IDs, sets `no_new_privs`, installs the seccomp filter
//! and executes the program. The keeper waits for it and ends as it
//! did. A step that fails in the child writes its number and `errno` on
//! a close-on-exec pipe; an empty pipe means the program started.
//!
//! Everything the children use is prepared before the fork: after it
//! they make system calls only, never allocating, since the job is
//! multi-threaded.

use std::ffi::{CString, c_char};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::seccomp::{self, Insn};
use crate::view::Step;
use crate::{Exit, Launch, PaseError};

/// What a child step failed at.
pub const STEPS: &[&str] = &[
    "unshare",
    "the namespace handshake",
    "fork",
    "a private mount namespace",
    "the root tmpfs",
    "a directory",
    "a mount point",
    "a symbolic link",
    "a bind mount",
    "proc",
    "pivot_root",
    "moving the root",
    "detaching the old root",
    "the read-only root",
    "the working directory",
    "the standard descriptors",
    "the groups",
    "the group ID",
    "the user ID",
    "no_new_privs",
    "the seccomp filter",
    "exec",
];

enum CStep {
    Dir(CString),
    File(CString),
    Link(CString, CString),
    Bind(CString, CString),
    Proc(CString),
}

fn cpath(p: &Path) -> Result<CString, PaseError> {
    CString::new(p.as_os_str().as_bytes()).map_err(|_| PaseError::Io("a NUL in a path".into()))
}

fn cstr(s: &str) -> Result<CString, PaseError> {
    CString::new(s).map_err(|_| PaseError::Io(format!("a NUL in {s:?}")))
}

/// Everything the children need, made before the fork.
struct Prepared {
    steps: Vec<CStep>,
    stage: CString,
    oldroot: CString,
    oldroot_inside: CString,
    dot: CString,
    slash: CString,
    tmpfs: CString,
    tmpfs_opts: CString,
    proc_name: CString,
    cwd: CString,
    path: CString,
    _argv: Vec<CString>,
    argv: Vec<*const c_char>,
    _envp: Vec<CString>,
    envp: Vec<*const c_char>,
    filter: Vec<Insn>,
    uid: u32,
    gid: u32,
    stdio: [i32; 3],
}

fn prepare(l: &Launch) -> Result<Prepared, PaseError> {
    let plan = crate::view::plan(&l.ifs, l.qopensys.as_deref(), &l.stage, &l.dev).map_err(|e| PaseError::Io(e.to_string()))?;
    let mut steps = Vec::with_capacity(plan.len());
    for s in plan {
        steps.push(match s {
            Step::Dir(p) => CStep::Dir(cpath(&p)?),
            Step::File(p) => CStep::File(cpath(&p)?),
            Step::Link(p, t) => CStep::Link(cpath(&p)?, cpath(&t)?),
            Step::Bind(a, b) => CStep::Bind(cpath(&a)?, cpath(&b)?),
            Step::Proc(p) => CStep::Proc(cpath(&p)?),
        });
    }
    let program = crate::view::inside(&l.program).ok_or_else(|| PaseError::NotVisible(l.program.clone()))?;
    let cwd = crate::view::inside(&l.cwd).unwrap_or_else(|| "/".into());
    let argv: Vec<CString> = l.argv.iter().map(|a| cstr(a)).collect::<Result<_, _>>()?;
    let envp: Vec<CString> = l
        .env
        .iter()
        .map(|(k, v)| cstr(&format!("{k}={v}")))
        .collect::<Result<_, _>>()?;
    let mut argv_p: Vec<*const c_char> = argv.iter().map(|c| c.as_ptr()).collect();
    argv_p.push(std::ptr::null());
    let mut envp_p: Vec<*const c_char> = envp.iter().map(|c| c.as_ptr()).collect();
    envp_p.push(std::ptr::null());
    Ok(Prepared {
        steps,
        stage: cpath(&l.stage)?,
        oldroot: cpath(&l.stage.join(".oldroot"))?,
        oldroot_inside: cstr("/.oldroot")?,
        dot: cstr(".")?,
        slash: cstr("/")?,
        tmpfs: cstr("tmpfs")?,
        tmpfs_opts: cstr("mode=0755")?,
        proc_name: cstr("proc")?,
        cwd: cstr(&cwd)?,
        path: cstr(&program)?,
        _argv: argv,
        argv: argv_p,
        _envp: envp,
        envp: envp_p,
        filter: seccomp::program(),
        uid: l.uid,
        gid: l.gid,
        stdio: l.stdio,
    })
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn pipe() -> Result<[i32; 2], PaseError> {
    let mut fds = [0i32; 2];
    // SAFETY: `fds` is a valid array of two ints for pipe2 to fill.
    let r = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if r != 0 {
        return Err(PaseError::Io(std::io::Error::last_os_error().to_string()));
    }
    Ok(fds)
}

fn close(fd: i32) {
    // SAFETY: closing a descriptor this module made; a bad one only fails.
    unsafe {
        libc::close(fd);
    }
}

/// Write all of `b`; nothing on failure.
fn write_all(fd: i32, b: &[u8]) {
    let mut off = 0;
    while off < b.len() {
        // SAFETY: the pointer and length are within `b`.
        let n = unsafe { libc::write(fd, b[off..].as_ptr().cast(), b.len() - off) };
        if n <= 0 {
            return;
        }
        off += n.unsigned_abs();
    }
}

/// Read exactly `b.len()` bytes; how many came.
fn read_full(fd: i32, b: &mut [u8]) -> usize {
    let mut off = 0;
    while off < b.len() {
        // SAFETY: the pointer and length are within `b`.
        let n = unsafe { libc::read(fd, b[off..].as_mut_ptr().cast(), b.len() - off) };
        if n < 0 && errno() == libc::EINTR {
            continue;
        }
        if n <= 0 {
            break;
        }
        off += n.unsigned_abs();
    }
    off
}

/// Report a failed step and end the child.
fn fail(err: i32, step: usize) -> ! {
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&u32::try_from(step).unwrap_or(u32::MAX).to_le_bytes());
    b[4..].copy_from_slice(&errno().to_le_bytes());
    write_all(err, &b);
    // SAFETY: ending a forked child without running the parent's
    // destructors or exit handlers.
    unsafe { libc::_exit(127) }
}

fn step_no(name: &str) -> usize {
    STEPS.iter().position(|s| *s == name).unwrap_or(0)
}

/// Start the program.
pub fn start(l: &Launch) -> Result<crate::Child, PaseError> {
    let p = prepare(l)?;
    std::fs::create_dir_all(&l.stage).map_err(|e| PaseError::Io(format!("{}: {e}", l.stage.display())))?;
    let ready = pipe()?;
    let go = pipe()?;
    let err = pipe()?;
    // SAFETY: fork in a multi-threaded process; the child only makes
    // system calls on memory prepared above until it executes or exits.
    let keeper = unsafe { libc::fork() };
    if keeper < 0 {
        return Err(PaseError::Io(std::io::Error::last_os_error().to_string()));
    }
    if keeper == 0 {
        keeper_main(&p, ready, go, err);
    }
    close(ready[1]);
    close(go[0]);
    close(err[1]);
    let done = |e: PaseError| -> Result<crate::Child, PaseError> {
        close(ready[0]);
        close(go[1]);
        close(err[0]);
        let _ = wait_pid(keeper);
        Err(e)
    };
    let mut b = [0u8; 1];
    if read_full(ready[0], &mut b) != 1 {
        return done(child_error(err[0]));
    }
    // Root maps to root so the child can build the view; the profile's
    // IDs map to themselves. The program runs as the profile.
    let map = |id: u32| {
        if id == 0 {
            "0 0 1\n".to_string()
        } else {
            format!("0 0 1\n{id} {id} 1\n")
        }
    };
    let maps = [
        (format!("/proc/{keeper}/uid_map"), map(l.uid)),
        (format!("/proc/{keeper}/gid_map"), map(l.gid)),
    ];
    for (f, text) in maps {
        if let Err(e) = std::fs::write(&f, text) {
            return done(PaseError::Io(format!("{f}: {e}")));
        }
    }
    write_all(go[1], b"g");
    close(go[1]);
    let mut pid = [0u8; 4];
    let got = read_full(ready[0], &mut pid);
    close(ready[0]);
    let e = child_error(err[0]);
    close(err[0]);
    if got != 4 || !matches!(e, PaseError::Started) {
        let _ = wait_pid(keeper);
        return Err(if matches!(e, PaseError::Started) {
            PaseError::Io("the program's process did not start".into())
        } else {
            e
        });
    }
    Ok(crate::Child {
        pid: keeper,
        program_pid: i32::from_le_bytes(pid),
    })
}

/// What the error pipe said: [`PaseError::Started`] when it closed empty.
fn child_error(fd: i32) -> PaseError {
    let mut b = [0u8; 8];
    if read_full(fd, &mut b) != 8 {
        return PaseError::Started;
    }
    let step = u32::from_le_bytes(b[..4].try_into().expect("four")) as usize;
    let errno = i32::from_le_bytes(b[4..].try_into().expect("four"));
    PaseError::Setup {
        step: STEPS.get(step).copied().unwrap_or("?"),
        errno,
    }
}

/// The keeper: new namespaces, the handshake, the program's process,
/// then wait for it.
fn keeper_main(p: &Prepared, ready: [i32; 2], go: [i32; 2], err: [i32; 2]) -> ! {
    close(ready[0]);
    close(go[1]);
    close(err[0]);
    // SAFETY: unshare in the single-threaded child.
    if unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWPID) } != 0 {
        fail(err[1], step_no("unshare"));
    }
    write_all(ready[1], b"r");
    let mut b = [0u8; 1];
    if read_full(go[0], &mut b) != 1 {
        fail(err[1], step_no("the namespace handshake"));
    }
    close(go[0]);
    // SAFETY: fork in the single-threaded keeper.
    let g = unsafe { libc::fork() };
    if g < 0 {
        fail(err[1], step_no("fork"));
    }
    if g == 0 {
        close(ready[1]);
        program_main(p, err[1]);
    }
    write_all(ready[1], &g.to_le_bytes());
    close(ready[1]);
    close(err[1]);
    for fd in p.stdio {
        close(fd);
    }
    let mut status = 0;
    loop {
        // SAFETY: waiting for the keeper's own child.
        let r = unsafe { libc::waitpid(g, &raw mut status, 0) };
        if r == g {
            break;
        }
        if r < 0 && errno() != libc::EINTR {
            // SAFETY: ending the keeper.
            unsafe { libc::_exit(127) }
        }
    }
    if libc::WIFSIGNALED(status) {
        let sig = libc::WTERMSIG(status);
        // SAFETY: the keeper ends as its child did: by the same signal.
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::kill(libc::getpid(), sig);
        }
    }
    // SAFETY: ending the keeper with the child's status.
    unsafe { libc::_exit(libc::WEXITSTATUS(status)) }
}

/// The program's process: the view, the identity, the filter, exec.
#[allow(clippy::too_many_lines)] // one straight sequence of system calls
fn program_main(p: &Prepared, err: i32) -> ! {
    let null = std::ptr::null::<libc::c_void>();
    let none = std::ptr::null::<c_char>();
    // SAFETY: each call below is a system call on NUL-terminated strings
    // and arrays prepared before the fork, which outlive the process.
    unsafe {
        if libc::mount(none, p.slash.as_ptr(), none, libc::MS_REC | libc::MS_PRIVATE, null) != 0 {
            fail(err, step_no("a private mount namespace"));
        }
        if libc::mount(
            p.tmpfs.as_ptr(),
            p.stage.as_ptr(),
            p.tmpfs.as_ptr(),
            libc::MS_NOSUID | libc::MS_NODEV,
            p.tmpfs_opts.as_ptr().cast(),
        ) != 0
        {
            fail(err, step_no("the root tmpfs"));
        }
        for s in &p.steps {
            match s {
                CStep::Dir(d) => {
                    if libc::mkdir(d.as_ptr(), 0o755) != 0 && errno() != libc::EEXIST {
                        fail(err, step_no("a directory"));
                    }
                }
                CStep::File(f) => {
                    let fd = libc::open(f.as_ptr(), libc::O_CREAT | libc::O_WRONLY | libc::O_CLOEXEC, 0o644);
                    if fd < 0 {
                        fail(err, step_no("a mount point"));
                    }
                    libc::close(fd);
                }
                CStep::Link(l, t) => {
                    if libc::symlink(t.as_ptr(), l.as_ptr()) != 0 {
                        fail(err, step_no("a symbolic link"));
                    }
                }
                CStep::Bind(a, b) => {
                    if libc::mount(a.as_ptr(), b.as_ptr(), none, libc::MS_BIND | libc::MS_REC, null) != 0 {
                        fail(err, step_no("a bind mount"));
                    }
                }
                CStep::Proc(d) => {
                    if libc::mount(
                        p.proc_name.as_ptr(),
                        d.as_ptr(),
                        p.proc_name.as_ptr(),
                        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                        null,
                    ) != 0
                    {
                        fail(err, step_no("proc"));
                    }
                }
            }
        }
        if libc::mkdir(p.oldroot.as_ptr(), 0o700) != 0 {
            fail(err, step_no("a directory"));
        }
        if libc::syscall(libc::SYS_pivot_root, p.stage.as_ptr(), p.oldroot.as_ptr()) == 0 {
            if libc::chdir(p.slash.as_ptr()) != 0
                || libc::umount2(p.oldroot_inside.as_ptr(), libc::MNT_DETACH) != 0
            {
                fail(err, step_no("detaching the old root"));
            }
            libc::rmdir(p.oldroot_inside.as_ptr());
        } else if *libc::__errno_location() == libc::EINVAL {
            // The root is the initramfs, which pivot_root cannot move: the
            // view is moved over it instead (switch_root's way); the old
            // root stays covered, out of every path.
            libc::rmdir(p.oldroot.as_ptr());
            if libc::chdir(p.stage.as_ptr()) != 0
                || libc::mount(p.dot.as_ptr(), p.slash.as_ptr(), none, libc::MS_MOVE, null) != 0
                || libc::chroot(p.dot.as_ptr()) != 0
                || libc::chdir(p.slash.as_ptr()) != 0
            {
                fail(err, step_no("moving the root"));
            }
        } else {
            fail(err, step_no("pivot_root"));
        }
        if libc::mount(
            none,
            p.slash.as_ptr(),
            none,
            libc::MS_REMOUNT | libc::MS_BIND | libc::MS_RDONLY,
            null,
        ) != 0
        {
            fail(err, step_no("the read-only root"));
        }
        if libc::chdir(p.cwd.as_ptr()) != 0 && libc::chdir(p.slash.as_ptr()) != 0 {
            fail(err, step_no("the working directory"));
        }
        for (i, fd) in p.stdio.iter().enumerate() {
            let to = i32::try_from(i).unwrap_or(0);
            if *fd != to && libc::dup2(*fd, to) < 0 {
                fail(err, step_no("the standard descriptors"));
            }
        }
        // Every other descriptor closes at exec.
        libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, libc::CLOSE_RANGE_CLOEXEC);
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&raw mut set);
        libc::sigprocmask(libc::SIG_SETMASK, &raw const set, std::ptr::null_mut());
        if libc::setgroups(0, std::ptr::null()) != 0 {
            fail(err, step_no("the groups"));
        }
        if libc::setresgid(p.gid, p.gid, p.gid) != 0 {
            fail(err, step_no("the group ID"));
        }
        if libc::setresuid(p.uid, p.uid, p.uid) != 0 {
            fail(err, step_no("the user ID"));
        }
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            fail(err, step_no("no_new_privs"));
        }
        let prog = libc::sock_fprog {
            len: u16::try_from(p.filter.len()).unwrap_or(u16::MAX),
            filter: p.filter.as_ptr().cast_mut().cast(),
        };
        if libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            0u32,
            &raw const prog,
        ) != 0
        {
            fail(err, step_no("the seccomp filter"));
        }
        libc::execve(p.path.as_ptr(), p.argv.as_ptr(), p.envp.as_ptr());
    }
    fail(err, step_no("exec"))
}

/// Wait for a process; how it ended.
pub fn wait_pid(pid: i32) -> Result<Exit, PaseError> {
    let mut status = 0;
    loop {
        // SAFETY: waiting for a child of this process.
        let r = unsafe { libc::waitpid(pid, &raw mut status, 0) };
        if r == pid {
            break;
        }
        if r < 0 && errno() != libc::EINTR {
            return Err(PaseError::Io(std::io::Error::last_os_error().to_string()));
        }
    }
    Ok(if libc::WIFSIGNALED(status) {
        Exit::Signal(libc::WTERMSIG(status))
    } else {
        Exit::Code(libc::WEXITSTATUS(status))
    })
}

/// Whether a process has ended, without waiting.
pub fn try_wait_pid(pid: i32) -> Result<Option<Exit>, PaseError> {
    let mut status = 0;
    // SAFETY: polling a child of this process.
    let r = unsafe { libc::waitpid(pid, &raw mut status, libc::WNOHANG) };
    if r == 0 {
        return Ok(None);
    }
    if r < 0 {
        return Err(PaseError::Io(std::io::Error::last_os_error().to_string()));
    }
    Ok(Some(if libc::WIFSIGNALED(status) {
        Exit::Signal(libc::WTERMSIG(status))
    } else {
        Exit::Code(libc::WEXITSTATUS(status))
    }))
}

/// Send a signal to a process.
pub fn signal(pid: i32, sig: i32) -> Result<(), PaseError> {
    // SAFETY: kill takes plain integers.
    if unsafe { libc::kill(pid, sig) } != 0 {
        return Err(PaseError::Io(std::io::Error::last_os_error().to_string()));
    }
    Ok(())
}

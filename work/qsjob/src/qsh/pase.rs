// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PASE in the job: `QP2SHELL`, `QP2SHELL2` and `QP2TERM` as programs of
//! `QSYS`, `Qp2RunPase` (through `sysapi::envproc`), and an ELF file a
//! shell runs. Each starts the program through the `pase` crate as the
//! job's user profile (its UID and GID), in the job's pool cgroup (the
//! program is a descendant of the job's process).

use std::io::Read;
use std::os::fd::AsRawFd;
use std::rc::Rc;

use ifs::client::IfsClient;
use mi::Job;

use crate::console::host::Host;

/// Whether an IFS file starts with the ELF magic number.
pub fn is_elf(host: &Host, path: &str) -> bool {
    let c = stg::remote::RemoteIfs::new(host.link.clone());
    let Ok(fd) = c.open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY)) else {
        return false;
    };
    let head = c.read(fd, 4).unwrap_or_default();
    let _ = c.close(fd);
    head == b"\x7fELF"
}

/// The job user's UID and GID.
fn ids(host: &Host) -> (u32, u32) {
    super::side::profiles(host)
        .into_iter()
        .find(|i| i.name == host.start.user)
        .map_or((0, 0), |i| (i.uid, i.gid))
}

/// Run a PASE program over the descriptors given; its exit status (128
/// plus the system's number when a signal ended it).
pub fn run_with_fds(
    host: &Host,
    path: &str,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    cwd: &str,
    fds: [i32; 3],
) -> Result<i32, String> {
    let (uid, gid) = ids(host);
    let mut l = pase::Launch::new(path, uid, gid, fds);
    if !argv.is_empty() {
        l.argv = argv;
    }
    l.env = env;
    if !cwd.is_empty() {
        l.cwd = cwd.to_string();
    }
    let mut child = pase::start(&l).map_err(|e| e.to_string())?;
    match child.wait().map_err(|e| e.to_string())? {
        pase::Exit::Code(c) => Ok(c),
        pase::Exit::Signal(s) => Ok(128 + super::signals::from_linux(s)),
    }
}

/// Run a PASE program with no input, its output and error collected:
/// the status and the text.
pub fn run_to_end(
    host: &Host,
    path: &str,
    argv: Vec<String>,
    env: Vec<(String, String)>,
    cwd: Option<&str>,
) -> Result<(i32, String), String> {
    let null = std::fs::File::open("/dev/null").map_err(|e| e.to_string())?;
    let (mut r, w) = std::io::pipe().map_err(|e| e.to_string())?;
    let fds = [null.as_raw_fd(), w.as_raw_fd(), w.as_raw_fd()];
    let (uid, gid) = ids(host);
    let mut l = pase::Launch::new(path, uid, gid, fds);
    if !argv.is_empty() {
        l.argv = argv;
    }
    l.env = env;
    l.cwd = cwd.unwrap_or("/").to_string();
    let mut child = pase::start(&l).map_err(|e| e.to_string())?;
    drop(w);
    drop(null);
    let mut out = Vec::new();
    let _ = r.read_to_end(&mut out);
    let code = match child.wait().map_err(|e| e.to_string())? {
        pase::Exit::Code(c) => c,
        pase::Exit::Signal(s) => 128 + super::signals::from_linux(s),
    };
    Ok((code, String::from_utf8_lossy(&out).into_owned()))
}

/// A NUL-terminated (or blank-padded) EBCDIC parameter at `addr`.
fn parm(addr: u64) -> String {
    if addr == 0 {
        return String::new();
    }
    let mut b = Vec::new();
    for i in 0..4096u64 {
        let c = mi::storage::view(addr + i, 1)[0];
        if c == 0 {
            break;
        }
        b.push(c);
    }
    ccsid::from_ebcdic(&b).trim_end().to_string()
}

/// `QP2SHELL(path, arg ...)`: run the program to its end; its output
/// becomes the call's output.
fn qp2shell(job: &mut Job, host: &Host, args: &[u64]) {
    let words: Vec<String> = args.iter().map(|a| parm(*a)).collect();
    let Some(path) = words.first().filter(|p| !p.is_empty()).cloned() else {
        job.console.push("QP2SHELL: no PASE program was named.".into());
        return;
    };
    let mut env = super::setup(host, "PIPELINE").env;
    env.retain(|(k, _)| k != "PATH");
    env.push(("PATH".into(), "/QOpenSys/usr/bin:/usr/ccs/bin:/QOpenSys/usr/bin/X11:/usr/sbin:.:/usr/bin".into()));
    match run_to_end(host, &path, words, env, None) {
        Ok((code, text)) => {
            for l in text.lines() {
                job.console.push(l.to_string());
            }
            if code != 0 {
                job.console.push(format!("PASE program {path} ended with status {code}."));
            }
        }
        Err(e) => job.console.push(format!("PASE program {path} did not start: {e}")),
    }
}

/// `QP2TERM`: with a program, as `QP2SHELL`; with none, the PASE shell,
/// which QS/400 does not ship.
fn qp2term(job: &mut Job, host: &Host, args: &[u64]) {
    if args.is_empty() || parm(args[0]).is_empty() {
        let sh = "/QOpenSys/usr/bin/sh";
        if !is_elf(host, sh) {
            job.console
                .push(format!("QP2TERM: no PASE shell is installed ({sh})."));
            return;
        }
        let a = [sh.to_string(), "-c".into(), "exit 0".into()];
        let _ = run_to_end(host, sh, a.to_vec(), Vec::new(), None);
        return;
    }
    qp2shell(job, host, args);
}

/// Install the programs.
pub fn install(job: &mut Job, host: &Host) {
    for name in ["QP2SHELL", "QP2SHELL2"] {
        let h = host.clone();
        job.natives.insert(
            name.into(),
            Rc::new(move |job: &mut Job, args: &[u64]| qp2shell(job, &h, args)),
        );
    }
    let h = host.clone();
    job.natives.insert(
        "QP2TERM".into(),
        Rc::new(move |job: &mut Job, args: &[u64]| qp2term(job, &h, args)),
    );
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running programs: the shell itself and the utilities on threads, a
//! `*PGM` called in the job, a PASE program in a child job; signals to
//! in-process children recorded for their shells, to jobs sent through
//! the job.

use std::sync::Arc;

use qsh::session::{self, Invocation};
use qsh::sys::mem::{ThreadChild, start_utility};
use qsh::sys::stream::write_all;
use qsh::sys::{Child, Errno, FileType, OpenFlags, SpawnRequest, Sys, SysError};

use super::{JobSys, SHELLS, THREAD_PIDS, lock};
use crate::side::{ChildKind, JobSpawn};

/// The library and program a `/QSYS.LIB` path names, if it names a
/// program: `/QSYS.LIB/LIB.LIB/PGM.PGM` or `/QSYS.LIB/PGM.PGM`.
#[must_use]
pub fn qsys_program(path: &str) -> Option<(String, String)> {
    let up = path.to_ascii_uppercase();
    let rest = up.strip_prefix("/QSYS.LIB/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    let (lib, obj) = match parts.as_slice() {
        [obj] => ("QSYS", *obj),
        [lib, obj] => (lib.strip_suffix(".LIB")?, *obj),
        _ => return None,
    };
    let pgm = obj.strip_suffix(".PGM")?;
    Some((lib.to_string(), pgm.to_string()))
}

/// `Sys::spawn` for a job.
pub fn spawn(sys: &JobSys, req: SpawnRequest) -> Result<Box<dyn Child>, SysError> {
    let real = sys.realpath(&req.path).unwrap_or_else(|_| req.path.clone());
    let found = [&req.path, &real]
        .into_iter()
        .find_map(|c| sys.utility_path(c));
    if let Some(u) = found {
        {
            if SHELLS.contains(&u.as_str()) {
                return shell(sys, &req);
            }
            if let Some(util) = sys.core.utils.get(&u) {
                let pid = sys.alloc_pid();
                return start_utility(Arc::new(sys.clone()), pid, util, req);
            }
        }
    }
    if let Some((lib, pgm)) = qsys_program(&real) {
        return program(sys, lib, pgm, &req);
    }
    let st = sys.stat(&real)?;
    if st.ftype == FileType::Dir || st.mode & 0o111 == 0 {
        return Err(Errno::EACCES.into());
    }
    if is_elf(sys, &real) {
        let r = JobSpawn {
            kind: ChildKind::Pase,
            path: real,
            argv: req.argv,
            env: req.env,
            cwd: req.cwd,
            fds: req.fds,
        };
        return sys.core.broker.call(move |s| s.spawn_job(r))?;
    }
    Err(Errno::ENOEXEC.into())
}

/// Whether a file starts with the ELF magic number.
fn is_elf(sys: &JobSys, path: &str) -> bool {
    let read = OpenFlags {
        read: true,
        ..OpenFlags::default()
    };
    let Ok(f) = sys.open(path, read, 0) else {
        return false;
    };
    let mut head = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match f.read(&mut head[got..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    got == 4 && head == *b"\x7fELF"
}

/// A new shell on a thread: `qsh` with the request's arguments, its
/// environment and directory.
fn shell(sys: &JobSys, req: &SpawnRequest) -> Result<Box<dyn Child>, SysError> {
    let pid = sys.alloc_pid();
    let child = sys.child(req.env.clone(), &req.cwd, pid);
    let fd = |n: u32| {
        req.fds
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, f)| f.clone())
    };
    let (i, o, e) = (fd(0), fd(1), fd(2));
    let argv = req.argv.clone();
    let name = argv.first().cloned().unwrap_or_else(|| "qsh".into());
    let body = Box::new(move || match Invocation::parse(&argv) {
        Ok(inv) => session::run(Arc::new(child), &inv, i, o, e),
        Err(m) => {
            if let Some(e) = &e {
                let _ = write_all(&**e, format!("qsh: 001-0062 {m}\n").as_bytes());
            }
            2
        }
    });
    Ok(Box::new(ThreadChild::start(pid, &name, body)?))
}

/// A `*PGM` called in the job, its display output on standard output.
fn program(
    sys: &JobSys,
    lib: String,
    pgm: String,
    req: &SpawnRequest,
) -> Result<Box<dyn Child>, SysError> {
    let pid = sys.alloc_pid();
    let broker = sys.core.broker.clone();
    let out = req
        .fds
        .iter()
        .find(|(k, _)| *k == 1)
        .map(|(_, f)| f.clone());
    let args: Vec<String> = req.argv.iter().skip(1).cloned().collect();
    let body = Box::new(move || {
        match broker.call(move |s| s.call_program(&lib, &pgm, &args)) {
            Ok(Ok(run)) => {
                if let Some(o) = &out {
                    let _ = write_all(&**o, run.output.as_bytes());
                }
                run.status
            }
            _ => 126,
        }
    });
    let name = req.argv.first().cloned().unwrap_or_default();
    Ok(Box::new(ThreadChild::start(pid, &name, body)?))
}

/// `Sys::kill` for a job: an in-process child or the shell records the
/// signal; another process ID is a job.
pub fn kill(sys: &JobSys, pid: i32, sig: i32) -> Result<(), SysError> {
    if pid <= 0 {
        return Err(Errno::ESRCH.into());
    }
    if pid >= THREAD_PIDS || pid == sys.pid {
        if sig != 0 {
            lock(&sys.core.signals).entry(pid).or_default().push(sig);
        }
        return Ok(());
    }
    sys.core.broker.call(move |s| s.signal_job(pid, sig))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_paths() {
        assert_eq!(
            qsys_program("/qsys.lib/qgpl.lib/hello.pgm"),
            Some(("QGPL".into(), "HELLO".into()))
        );
        assert_eq!(
            qsys_program("/QSYS.LIB/QCMDEXC.PGM"),
            Some(("QSYS".into(), "QCMDEXC".into()))
        );
        assert_eq!(qsys_program("/QSYS.LIB/QGPL.LIB/X.FILE"), None);
        assert_eq!(qsys_program("/usr/bin/ls"), None);
    }
}

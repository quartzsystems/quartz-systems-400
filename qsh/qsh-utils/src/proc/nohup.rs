// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nohup utility [argument...]`: run a program with `SIGHUP` ignored.
//! When standard output is a terminal it goes to `nohup.out` (or
//! `$HOME/nohup.out`), appended, mode 0600, with a note on standard
//! error; standard error follows it when that is a terminal too.
//! Status: the program's, 126 when it cannot run, 127 when it is not
//! found or `nohup` itself fails.

use qsh::signal::SIGHUP;
use qsh::sys::{Errno, Fd, OpenFlags, SigAction, SpawnRequest, UtilCtx};

use crate::common::complain;
use crate::common::exec::{ExecError, SHELL, find_program};

const NAME: &str = "nohup";

fn open_out(cx: &UtilCtx<'_>) -> Option<(Fd, String)> {
    let flags = OpenFlags {
        write: true,
        create: true,
        append: true,
        ..OpenFlags::default()
    };
    let mut cands = vec![("nohup.out".to_string(), cx.path("nohup.out"))];
    if let Some(h) = cx.getenv("HOME") {
        let p = crate::file::join(h, "nohup.out");
        cands.push((p.clone(), p));
    }
    for (shown, abs) in cands {
        if let Ok(fd) = cx.sys.open(&abs, flags, 0o600) {
            return Some((fd, shown));
        }
    }
    None
}

fn spawn_wait(cx: &UtilCtx<'_>, mut req: SpawnRequest) -> Result<i32, ExecError> {
    let argv = req.argv.clone();
    let mut child = match cx.sys.spawn(req.clone()) {
        Ok(c) => c,
        Err(e) if e.errno == Errno::ENOEXEC => {
            let mut sh = vec!["sh".to_string(), req.path.clone()];
            sh.extend(argv.iter().skip(1).cloned());
            req.path = SHELL.to_string();
            req.argv = sh;
            cx.sys
                .spawn(req)
                .map_err(|_| ExecError::NotExecutable)?
        }
        Err(e) => return Err(ExecError::Sys(e)),
    };
    cx.sys.signal_action(child.pid(), SIGHUP, SigAction::Ignore);
    child.wait().map(qsh::sys::WaitStatus::code).map_err(ExecError::Sys)
}

/// Run `nohup`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let mut args = &cx.argv[1..];
    if args.first().is_some_and(|a| a == "--") {
        args = &args[1..];
    }
    if args.is_empty() {
        complain(cx, NAME, "A utility to run is needed.");
        return 127;
    }
    let path = match find_program(cx, &args[0]) {
        Ok(p) => p,
        Err(e) => {
            complain(cx, NAME, &format!("{}: {}", args[0], e.text()));
            return e.status();
        }
    };
    let mut fds: Vec<(u32, Fd)> = cx.fds.iter().map(|(k, v)| (*k, v.clone())).collect();
    let term = |n: u32| cx.fd(n).is_some_and(|f| f.is_terminal());
    if term(1) {
        let Some((fd, shown)) = open_out(cx) else {
            complain(cx, NAME, "Neither nohup.out nor $HOME/nohup.out can be opened.");
            return 127;
        };
        complain(cx, NAME, &format!("Output goes to {shown}."));
        fds.retain(|(k, _)| *k != 1);
        fds.push((1, fd.clone()));
        if term(2) {
            fds.retain(|(k, _)| *k != 2);
            fds.push((2, fd));
        }
    } else if term(2)
        && let Some(out) = cx.fd(1).cloned()
    {
        fds.retain(|(k, _)| *k != 2);
        fds.push((2, out));
    }
    let req = SpawnRequest {
        path,
        argv: args.to_vec(),
        env: cx.env.clone(),
        cwd: cx.cwd.clone(),
        umask: cx.umask,
        fds,
        background: false,
    };
    match spawn_wait(cx, req) {
        Ok(n) => n,
        Err(e) => {
            complain(cx, NAME, &format!("{}: {}", args[0], e.text()));
            e.status()
        }
    }
}

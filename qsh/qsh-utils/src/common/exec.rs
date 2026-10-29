// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running another program from a utility (`xargs`, `find -exec`,
//! `env`): `PATH` search and a `Sys::spawn` with the utility's
//! environment and descriptors.

use qsh::sys::{Access, Errno, Fd, FileType, SpawnRequest, SysError, UtilCtx};

/// The shell that runs scripts.
pub const SHELL: &str = "/usr/bin/sh";

/// Why a program could not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    /// Not found on `PATH` (status 127).
    NotFound,
    /// Found but not runnable (status 126).
    NotExecutable,
    /// The system refused.
    Sys(SysError),
}

impl ExecError {
    /// The conventional exit status.
    pub fn status(self) -> i32 {
        match self {
            Self::NotFound => 127,
            Self::NotExecutable | Self::Sys(_) => 126,
        }
    }

    /// Text for a message.
    pub fn text(self) -> String {
        match self {
            Self::NotFound => "not found.".to_string(),
            Self::NotExecutable => "cannot be run.".to_string(),
            Self::Sys(e) => e.errno.text().to_string(),
        }
    }
}

fn runnable(cx: &UtilCtx<'_>, abs: &str) -> Option<bool> {
    let st = cx.sys.stat(abs).ok()?;
    Some(st.ftype != FileType::Dir && cx.sys.access(abs, Access::Exec).is_ok())
}

/// The absolute path of a program: as given when it has a slash, else
/// the first runnable file on `PATH` (`/usr/bin:.` by default).
pub fn find_program(cx: &UtilCtx<'_>, name: &str) -> Result<String, ExecError> {
    if name.contains('/') {
        let abs = cx.path(name);
        return match runnable(cx, &abs) {
            Some(true) => Ok(abs),
            Some(false) => Err(ExecError::NotExecutable),
            None => Err(ExecError::NotFound),
        };
    }
    let path = cx.getenv("PATH").unwrap_or("/usr/bin:.");
    let mut seen = false;
    for dir in path.split(':') {
        let dir = if dir.is_empty() { "." } else { dir };
        let cand = cx.path(&format!("{dir}/{name}"));
        match runnable(cx, &cand) {
            Some(true) => return Ok(cand),
            Some(false) => seen = true,
            None => {}
        }
    }
    Err(if seen {
        ExecError::NotExecutable
    } else {
        ExecError::NotFound
    })
}

/// Run a program and wait: `argv[0]` is looked up; `stdin` replaces
/// descriptor 0 when given; `env` replaces the environment when given.
/// The result is the exit status (128+n for a signal).
pub fn run(
    cx: &UtilCtx<'_>,
    argv: &[String],
    stdin: Option<Fd>,
    env: Option<Vec<(String, String)>>,
) -> Result<i32, ExecError> {
    let name = argv.first().ok_or(ExecError::NotFound)?;
    let path = find_program(cx, name)?;
    let mut fds: Vec<(u32, Fd)> = cx.fds.iter().map(|(k, v)| (*k, v.clone())).collect();
    if let Some(s) = stdin {
        fds.retain(|(k, _)| *k != 0);
        fds.push((0, s));
    }
    let mut req = SpawnRequest {
        path,
        argv: argv.to_vec(),
        env: env.unwrap_or_else(|| cx.env.clone()),
        cwd: cx.cwd.clone(),
        umask: cx.umask,
        fds,
        background: false,
    };
    let mut child = match cx.sys.spawn(req.clone()) {
        Ok(c) => c,
        // A file that is not a program is a script: run it with the
        // shell, as `execvp` does.
        Err(e) if e.errno == Errno::ENOEXEC => {
            let mut sh_argv = vec!["sh".to_string(), req.path.clone()];
            sh_argv.extend(argv.iter().skip(1).cloned());
            req.path = SHELL.to_string();
            req.argv = sh_argv;
            cx.sys.spawn(req).map_err(|e| match e.errno {
                Errno::ENOENT | Errno::ENOEXEC => ExecError::NotExecutable,
                _ => ExecError::Sys(e),
            })?
        }
        Err(e) => return Err(ExecError::Sys(e)),
    };
    let st = child.wait().map_err(ExecError::Sys)?;
    Ok(st.code())
}

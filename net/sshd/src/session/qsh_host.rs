// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSH as the login shell: `exec` runs `qsh -c`, `shell` runs a login
//! shell (interactive over the line discipline with a terminal, reading
//! its commands from standard input without one).
//!
//! The [`qsh::Sys`] a session runs over comes from a [`JobSys`]: in
//! production the session's job (a `QP0ZSPWT`-style job running as the
//! user, its environment set from [`SessionRequest::environment`]); in
//! tests a `MemSys`.

use std::any::Any;
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use qsh::session::{self, Feed, Interactive, Invocation};
use qsh::sys::{Fd, Stream};

use super::ldisc::{Input, LineDiscipline, Onlcr};
use super::{ChannelIo, Exit, SessionHost, SessionKind, SessionRequest};

/// Gives each session the system its shell runs over.
pub trait JobSys: Send + Sync {
    /// The `Sys` for the job that runs `req`, with its environment.
    fn sys_for(&self, req: &SessionRequest) -> Arc<dyn qsh::Sys>;
}

impl<F: Fn(&SessionRequest) -> Arc<dyn qsh::Sys> + Send + Sync> JobSys for F {
    fn sys_for(&self, req: &SessionRequest) -> Arc<dyn qsh::Sys> {
        self(req)
    }
}

/// Runs QSH for sessions.
pub struct QshHost<J: JobSys> {
    jobs: J,
}

impl<J: JobSys> QshHost<J> {
    /// A host over `jobs`.
    pub fn new(jobs: J) -> Self {
        Self { jobs }
    }
}

struct InFd {
    r: Mutex<Box<dyn Read + Send>>,
    tty: bool,
}

impl Stream for InFd {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.r
            .lock()
            .map_err(|_| io::Error::other("stdin poisoned"))?
            .read(buf)
    }
    fn write(&self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from_raw_os_error(9))
    }
    fn is_terminal(&self) -> bool {
        self.tty
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct OutFd {
    w: Mutex<Box<dyn Write + Send>>,
    tty: bool,
}

impl Stream for OutFd {
    fn read(&self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::from_raw_os_error(9))
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut w = self
            .w
            .lock()
            .map_err(|_| io::Error::other("stdout poisoned"))?;
        w.write_all(buf)?;
        Ok(buf.len())
    }
    fn is_terminal(&self) -> bool {
        self.tty
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn code(n: i32) -> Exit {
    Exit::Status(u32::try_from(n).unwrap_or(255) & 0xff)
}

impl<J: JobSys> SessionHost for QshHost<J> {
    fn run(&self, req: SessionRequest, io: ChannelIo) -> Exit {
        let sys = self.jobs.sys_for(&req);
        let tty = req.pty.is_some();
        let stdin: Fd = Arc::new(InFd {
            r: Mutex::new(io.stdin),
            tty,
        });
        let (stdout, stderr): (Fd, Fd) = if tty {
            let out: Fd = Arc::new(OutFd {
                w: Mutex::new(Box::new(Onlcr(io.stdout))),
                tty,
            });
            (out.clone(), out)
        } else {
            (
                Arc::new(OutFd {
                    w: Mutex::new(io.stdout),
                    tty,
                }),
                Arc::new(OutFd {
                    w: Mutex::new(io.stderr),
                    tty,
                }),
            )
        };
        match &req.kind {
            SessionKind::Exec(cmd) => code(session::run(
                sys,
                &Invocation::command(cmd),
                Some(stdin),
                Some(stdout),
                Some(stderr),
            )),
            SessionKind::Shell if tty => interactive(sys, &stdin, &stdout),
            SessionKind::Shell => {
                let inv = Invocation {
                    arg0: "-qsh".into(),
                    login: true,
                    stdin: true,
                    interactive: Some(false),
                    ..Invocation::default()
                };
                code(session::run(sys, &inv, Some(stdin), Some(stdout), Some(stderr)))
            }
            SessionKind::Subsystem { name, .. } => {
                let _ = stderr.write(format!("qsh: subsystem {name} is not available\n").as_bytes());
                Exit::Status(127)
            }
        }
    }
}

/// A terminal session: echo and edit, feed whole lines to the shell.
fn interactive(sys: Arc<dyn qsh::Sys>, stdin: &Fd, out: &Fd) -> Exit {
    let sh = session::shell_for(
        sys,
        &Invocation::session(),
        Some(stdin.clone()),
        Some(out.clone()),
        Some(out.clone()),
    );
    let mut i = Interactive::new(sh);
    let mut ld = LineDiscipline::default();
    let show = |text: &str| {
        let _ = out.write(text.as_bytes());
    };
    show(&i.prompt());
    let mut buf = [0u8; 4096];
    loop {
        let n = stdin.read(&mut buf).unwrap_or(0);
        let mut echo = Vec::new();
        let inputs = if n == 0 {
            vec![Input::Eof]
        } else {
            ld.feed(&buf[..n], &mut echo)
        };
        if !echo.is_empty() {
            // Echo goes out raw: the line discipline already wrote CR LF.
            let raw = String::from_utf8_lossy(&echo).replace("\r\n", "\n");
            show(&raw);
        }
        for input in inputs {
            let feed = match input {
                Input::Line(l) => i.feed_line(&l),
                Input::Interrupt => Feed::Ready,
                Input::Eof => i.feed_eof(),
            };
            match feed {
                Feed::Exit(n) => return code(n),
                Feed::Ready | Feed::More => {
                    show(&i.notices());
                    show(&i.prompt());
                }
            }
        }
    }
}

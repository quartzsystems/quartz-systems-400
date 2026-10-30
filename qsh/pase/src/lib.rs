// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PASE mechanism: a program from the IFS run as a Linux process in
//! new user, mount and PID namespaces, its root built from `/QOpenSys`
//! ([`view`]), the job's user profile mapped to its own UID and GID, in
//! the job's pool cgroup (the job's process is in it; the program is its
//! descendant), under a seccomp filter ([`seccomp`]). QS/400 ships
//! nothing to run inside it.
//!
//! `QP2SHELL`, `QP2SHELL2`, `QP2TERM`, `Qp2RunPase` and a shell running
//! an ELF file all come here (`work/qsjob/src/qsh/pase.rs`). `docs/QSH.md`
//! ("PASE") is the reference.

#![deny(missing_docs)]

#[cfg(target_os = "linux")]
mod platform;
pub mod seccomp;
pub mod view;

use std::path::PathBuf;

/// Where the storage service mounts the IFS.
pub const IFS_ROOT: &str = "/run/qs400/ifs";
/// Where `slic` mounts `/QOpenSys` again with programs allowed: the IFS
/// mount itself is `noexec`.
pub const QOPENSYS_EXEC: &str = "/run/qs400/qopensys";
/// Where a PASE process's root is built (in its own mount namespace,
/// so every process may use the same directory).
pub const STAGE: &str = "/run/qs400/pase";

/// Why a PASE program did not start or could not be waited for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PaseError {
    /// The program is outside the view (only `/QOpenSys` and `/home`
    /// are in it).
    #[error("{0} is not in the PASE view of the IFS")]
    NotVisible(String),
    /// A step in the child failed.
    #[error("starting the PASE program failed at {step}, errno {errno}")]
    Setup {
        /// The step.
        step: &'static str,
        /// The Linux `errno`.
        errno: i32,
    },
    /// The parent's side failed.
    #[error("starting the PASE program: {0}")]
    Io(String),
    /// Not on Linux.
    #[error("PASE needs Linux")]
    Unsupported,
    /// The program started (never returned as an error).
    #[error("started")]
    Started,
}

/// A program to start.
#[derive(Clone, Debug)]
pub struct Launch {
    /// The IFS mount ([`IFS_ROOT`]).
    pub ifs: PathBuf,
    /// The mount of `/QOpenSys` that allows programs ([`QOPENSYS_EXEC`]);
    /// `None` uses the IFS mount's.
    pub qopensys: Option<PathBuf>,
    /// The directory the root is built on ([`STAGE`]).
    pub stage: PathBuf,
    /// The host's device directory.
    pub dev: PathBuf,
    /// The program's IFS path.
    pub program: String,
    /// The arguments, `argv[0]` first.
    pub argv: Vec<String>,
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory, an IFS path (`/` when outside the view).
    pub cwd: String,
    /// The profile's UID.
    pub uid: u32,
    /// The profile's GID.
    pub gid: u32,
    /// Descriptors for standard input, output and error.
    pub stdio: [i32; 3],
}

impl Launch {
    /// A launch of `program` over the system's IFS and devices.
    #[must_use]
    pub fn new(program: &str, uid: u32, gid: u32, stdio: [i32; 3]) -> Self {
        Self {
            ifs: IFS_ROOT.into(),
            qopensys: std::path::Path::new(QOPENSYS_EXEC)
                .is_dir()
                .then(|| QOPENSYS_EXEC.into()),
            stage: STAGE.into(),
            dev: "/dev".into(),
            program: program.to_string(),
            argv: vec![program.to_string()],
            env: Vec::new(),
            cwd: "/".into(),
            uid,
            gid,
            stdio,
        }
    }
}

/// How the program ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Its exit status.
    Code(i32),
    /// The Linux signal that ended it.
    Signal(i32),
}

/// A started program.
#[derive(Debug)]
pub struct Child {
    /// The keeper process, the job's child.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pid: i32,
    /// The program's process, as the job's namespace numbers it.
    program_pid: i32,
}

impl Child {
    /// The program's process ID outside its namespace.
    #[must_use]
    pub fn program_pid(&self) -> i32 {
        self.program_pid
    }

    /// Wait for the program to end.
    pub fn wait(&mut self) -> Result<Exit, PaseError> {
        #[cfg(target_os = "linux")]
        return platform::wait_pid(self.pid);
        #[cfg(not(target_os = "linux"))]
        Err(PaseError::Unsupported)
    }

    /// Whether it has ended, without waiting.
    pub fn try_wait(&mut self) -> Result<Option<Exit>, PaseError> {
        #[cfg(target_os = "linux")]
        return platform::try_wait_pid(self.pid);
        #[cfg(not(target_os = "linux"))]
        Err(PaseError::Unsupported)
    }

    /// Send the program a Linux signal.
    pub fn signal(&self, sig: i32) -> Result<(), PaseError> {
        #[cfg(target_os = "linux")]
        return platform::signal(self.program_pid, sig);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = sig;
            Err(PaseError::Unsupported)
        }
    }
}

/// Start a program.
pub fn start(l: &Launch) -> Result<Child, PaseError> {
    #[cfg(target_os = "linux")]
    return platform::start(l);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = l;
        Err(PaseError::Unsupported)
    }
}

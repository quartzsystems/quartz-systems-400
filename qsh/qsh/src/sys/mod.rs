// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The seam between the language and the system. Everything the shell
//! does outside its own memory goes through [`Sys`]: opening paths
//! (the IFS client on QS/400), pipes, running utilities in-process or
//! as child jobs, waiting, signals, time and limits.
//!
//! The shell keeps its own descriptor table of [`Fd`]s, its working
//! directory and its umask, and passes absolute paths and descriptor
//! maps to `Sys`; a `Sys` holds no per-shell state, so one value serves
//! a shell and all its subshells on other threads.

mod errno;
pub mod host;
pub mod mem;
pub mod stream;
pub mod util;

use std::time::Duration;

pub use errno::{Errno, SysError};
pub use host::HostSys;
pub use mem::MemSys;
pub use stream::{Buffer, Bytes, Fd, Null, Stream};
pub use util::{Registry, UtilCtx, Utility};

/// How to open a path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct OpenFlags {
    /// Open for reading.
    pub read: bool,
    /// Open for writing.
    pub write: bool,
    /// Create when missing.
    pub create: bool,
    /// Truncate to zero length.
    pub truncate: bool,
    /// Every write goes to the end.
    pub append: bool,
    /// Fail when it exists.
    pub excl: bool,
}

/// The type of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    /// A regular (stream) file.
    File,
    /// A directory.
    Dir,
    /// A symbolic link (from `lstat` only).
    Symlink,
    /// A FIFO.
    Fifo,
    /// A character special file.
    Char,
    /// A block special file.
    Block,
    /// A socket.
    Socket,
    /// Anything else (a `/QSYS.LIB` object, say).
    Other,
}

/// What `stat` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    /// The type.
    pub ftype: FileType,
    /// Permission bits plus set-user-ID (0o4000), set-group-ID (0o2000)
    /// and sticky (0o1000).
    pub mode: u32,
    /// Size in bytes.
    pub size: u64,
    /// Owner.
    pub uid: u32,
    /// Group.
    pub gid: u32,
    /// Modification time, seconds since the epoch.
    pub mtime: i64,
    /// File ID.
    pub ino: u64,
    /// File system ID.
    pub dev: u64,
}

/// What `access` checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// The path exists.
    Exists,
    /// The job may read it.
    Read,
    /// The job may write it.
    Write,
    /// The job may run or search it.
    Exec,
}

/// A request to run a program as a child: an in-process utility or a
/// child job (`spawn()` on QS/400).
#[derive(Clone)]
pub struct SpawnRequest {
    /// The resolved path of the program.
    pub path: String,
    /// The arguments, `argv[0]` first.
    pub argv: Vec<String>,
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory.
    pub cwd: String,
    /// The file mode creation mask.
    pub umask: u32,
    /// The descriptor map: the child's descriptor number and what it
    /// refers to. Descriptors not listed are closed in the child.
    pub fds: Vec<(u32, Fd)>,
    /// Started with `&`.
    pub background: bool,
}

/// How a child ended or changed state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitStatus {
    /// Exited with a status.
    Exited(i32),
    /// Ended by a signal.
    Signaled(i32),
    /// Stopped by a signal (job control).
    Stopped(i32),
}

impl WaitStatus {
    /// The shell's `$?` for this status.
    pub fn code(self) -> i32 {
        match self {
            Self::Exited(n) => n,
            Self::Signaled(s) | Self::Stopped(s) => crate::signal::status(s),
        }
    }
}

/// A running child: a utility thread, a subshell thread or a child job.
pub trait Child: Send {
    /// Its process ID (`$!`, `jobs -p`, `kill`).
    fn pid(&self) -> i32;
    /// Wait until it ends (or stops).
    fn wait(&mut self) -> Result<WaitStatus, SysError>;
    /// Its status if it has ended, without waiting.
    fn try_wait(&mut self) -> Result<Option<WaitStatus>, SysError>;
}

/// What the shell wants done with a signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigAction {
    /// The system default.
    Default,
    /// Ignore it.
    Ignore,
    /// Record it for [`Sys::take_signals`] (a trap is set).
    Catch,
}

/// CPU times for `times`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Times {
    /// The shell's user time.
    pub user: Duration,
    /// The shell's system time.
    pub system: Duration,
    /// Children's user time.
    pub child_user: Duration,
    /// Children's system time.
    pub child_system: Duration,
}

/// A resource limit for `ulimit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// `-c` core file size, 512-byte blocks.
    Core,
    /// `-d` data segment size, KB.
    Data,
    /// `-f` file size, 512-byte blocks.
    FileSize,
    /// `-n` open descriptors.
    OpenFiles,
    /// `-s` stack size, KB.
    Stack,
    /// `-t` CPU seconds.
    CpuTime,
    /// `-v` virtual memory, KB.
    Memory,
}

/// Terminal job control, when the session has a terminal (SSH TTY).
pub trait JobControl: Send + Sync {
    /// Give the terminal to a job's process group.
    fn give_terminal(&self, pgid: i32) -> Result<(), SysError>;
    /// Take the terminal back for the shell.
    fn take_terminal(&self) -> Result<(), SysError>;
}

/// Everything the shell needs from the system.
pub trait Sys: Send + Sync {
    /// The environment the shell starts with.
    fn environ(&self) -> Vec<(String, String)>;
    /// The working directory the shell starts in.
    fn initial_cwd(&self) -> String;
    /// The shell's process ID (`$$`).
    fn pid(&self) -> i32;
    /// The parent's process ID (`PPID`).
    fn ppid(&self) -> i32;
    /// A user's home directory, for `~user`.
    fn home_of(&self, user: &str) -> Option<String>;

    /// Open a path (absolute). `mode` is the creation mode with the
    /// umask already applied.
    fn open(&self, path: &str, flags: OpenFlags, mode: u32) -> Result<Fd, SysError>;
    /// Status of a path, following symbolic links.
    fn stat(&self, path: &str) -> Result<Stat, SysError>;
    /// Status of a path, not following a final symbolic link.
    fn lstat(&self, path: &str) -> Result<Stat, SysError>;
    /// The names in a directory, without `.` and `..`.
    fn read_dir(&self, path: &str) -> Result<Vec<String>, SysError>;
    /// Check the job's access to a path.
    fn access(&self, path: &str, how: Access) -> Result<(), SysError>;
    /// The path with symbolic links, `.` and `..` resolved.
    fn realpath(&self, path: &str) -> Result<String, SysError>;
    /// Are names in this directory compared without regard to case?
    fn case_insensitive(&self, dir: &str) -> bool;
    /// The shell's working directory changed (the job's own current
    /// directory follows it).
    fn chdir(&self, path: &str) -> Result<(), SysError> {
        let st = self.stat(path)?;
        if st.ftype != FileType::Dir {
            return Err(Errno::ENOTDIR.into());
        }
        self.access(path, Access::Exec)
    }
    /// Make a directory. (Defaults here and below are `ENOSYS`: the
    /// language needs none of them; utilities do.)
    fn mkdir(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let _ = (path, mode);
        Err(Errno::ENOSYS.into())
    }
    /// Remove an empty directory.
    fn rmdir(&self, path: &str) -> Result<(), SysError> {
        let _ = path;
        Err(Errno::ENOSYS.into())
    }
    /// Remove a link to a file.
    fn unlink(&self, path: &str) -> Result<(), SysError> {
        let _ = path;
        Err(Errno::ENOSYS.into())
    }
    /// Rename a link.
    fn rename(&self, from: &str, to: &str) -> Result<(), SysError> {
        let _ = (from, to);
        Err(Errno::ENOSYS.into())
    }
    /// Make a hard link.
    fn link(&self, existing: &str, new: &str) -> Result<(), SysError> {
        let _ = (existing, new);
        Err(Errno::ENOSYS.into())
    }
    /// Make a symbolic link at `path` holding `target`.
    fn symlink(&self, target: &str, path: &str) -> Result<(), SysError> {
        let _ = (target, path);
        Err(Errno::ENOSYS.into())
    }
    /// The target of a symbolic link.
    fn readlink(&self, path: &str) -> Result<String, SysError> {
        let _ = path;
        Err(Errno::ENOSYS.into())
    }
    /// Change permission bits.
    fn chmod(&self, path: &str, mode: u32) -> Result<(), SysError> {
        let _ = (path, mode);
        Err(Errno::ENOSYS.into())
    }
    /// Change owner and group (`None` leaves one as is).
    fn chown(&self, path: &str, uid: Option<u32>, gid: Option<u32>) -> Result<(), SysError> {
        let _ = (path, uid, gid);
        Err(Errno::ENOSYS.into())
    }
    /// Set the modification time (seconds since the epoch).
    fn utimes(&self, path: &str, mtime: i64) -> Result<(), SysError> {
        let _ = (path, mtime);
        Err(Errno::ENOSYS.into())
    }
    /// A pipe: (read end, write end).
    fn pipe(&self) -> Result<(Fd, Fd), SysError> {
        Ok(stream::pipe())
    }

    /// Start a program. `ENOEXEC` means the file exists and may be run
    /// but is not a program: the shell runs it as a script.
    fn spawn(&self, req: SpawnRequest) -> Result<Box<dyn Child>, SysError>;
    /// Run shell code concurrently (a pipeline element, `&`). The body
    /// returns the exit status.
    fn fork(
        &self,
        name: &str,
        body: Box<dyn FnOnce() -> i32 + Send>,
    ) -> Result<Box<dyn Child>, SysError>;
    /// Send a signal.
    fn kill(&self, pid: i32, sig: i32) -> Result<(), SysError>;
    /// Signals caught for this process ID since the last call.
    fn take_signals(&self, pid: i32) -> Vec<i32>;
    /// Set what happens to a signal for a process ID.
    fn signal_action(&self, pid: i32, sig: i32, action: SigAction) {
        let _ = (pid, sig, action);
    }
    /// Terminal job control, if the session has a terminal.
    fn job_control(&self) -> Option<&dyn JobControl> {
        None
    }

    /// Time since the epoch.
    fn now(&self) -> Duration;
    /// CPU times.
    fn times(&self) -> Times {
        Times::default()
    }
    /// A resource limit (soft, `hard` false, or hard); `None` is
    /// unlimited.
    fn get_limit(&self, limit: Limit, hard: bool) -> Result<Option<u64>, SysError> {
        let _ = (limit, hard);
        Ok(None)
    }
    /// Set a resource limit.
    fn set_limit(&self, limit: Limit, hard: bool, value: Option<u64>) -> Result<(), SysError> {
        let _ = (limit, hard, value);
        Err(Errno::ENOSYS.into())
    }
}

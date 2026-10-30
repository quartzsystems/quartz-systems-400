// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the job answers on its own thread. Every method but [`Side::ifs`]
//! has a default that says the job cannot (`ENOSYS`), so a test side
//! implements only what it exercises.

use ifs::client::IfsClient;
use qsh::sys::{
    Child, ClOutcome, ClRequest, Credentials, DataArea, DnsAnswer, DnsQuery, DtaqOp, DtaqReply,
    Errno, Fd, LibEntry, LiblChange, ProcInfo, SplfInfo, SplfNumber, Spooled, SqlOutcome,
    SqlRequest, SysError,
};

/// A profile's identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ids {
    /// The profile name.
    pub name: String,
    /// `UID`.
    pub uid: u32,
    /// `GID`, 0 for none.
    pub gid: u32,
    /// The home directory.
    pub home: String,
}

/// What calling a program in the job did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramRun {
    /// 0 when it returned, 1 when it ended with an escape.
    pub status: i32,
    /// What it wrote to the job's display.
    pub output: String,
}

/// What a child job runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildKind {
    /// The shell (`/usr/bin/qsh`, `/usr/bin/sh`) with the arguments.
    Shell,
    /// A PASE program (an ELF file in the IFS).
    Pase,
    /// A `*PGM` called with the arguments as parameters.
    Program,
}

/// A child job to start (`spawn()`).
#[derive(Clone)]
pub struct JobSpawn {
    /// What the job runs.
    pub kind: ChildKind,
    /// The program's path.
    pub path: String,
    /// Arguments, `argv[0]` first.
    pub argv: Vec<String>,
    /// The environment.
    pub env: Vec<(String, String)>,
    /// The working directory.
    pub cwd: String,
    /// Descriptors 0, 1 and 2 of the child; the rest are closed.
    pub fds: Vec<(u32, Fd)>,
}

fn nosys<T>() -> Result<T, SysError> {
    Err(Errno::ENOSYS.into())
}

/// The job, as the shell's tasks see it.
#[allow(unused_variables)]
pub trait Side {
    /// The job's IFS.
    fn ifs(&self) -> &dyn IfsClient;
    /// Run a CL command in the job.
    fn run_cl(&mut self, req: &ClRequest) -> Result<ClOutcome, SysError> {
        nosys()
    }
    /// Run an SQL statement.
    fn run_sql(&mut self, req: &SqlRequest) -> Result<SqlOutcome, SysError> {
        nosys()
    }
    /// Call `lib/pgm` with the arguments as parameters.
    fn call_program(&mut self, lib: &str, pgm: &str, args: &[String]) -> Result<ProgramRun, SysError> {
        nosys()
    }
    /// Start a child job.
    fn spawn_job(&mut self, req: JobSpawn) -> Result<Box<dyn Child>, SysError> {
        nosys()
    }
    /// Send a signal to another job (its process ID).
    fn signal_job(&mut self, pid: i32, sig: i32) -> Result<(), SysError> {
        Err(Errno::ESRCH.into())
    }
    /// The active jobs as processes.
    fn processes(&mut self) -> Result<Vec<ProcInfo>, SysError> {
        nosys()
    }
    /// Who the job runs as.
    fn credentials(&mut self) -> Result<Credentials, SysError> {
        nosys()
    }
    /// Every profile's IDs.
    fn profiles(&mut self) -> Result<Vec<Ids>, SysError> {
        nosys()
    }
    /// Ask the resolver.
    fn resolve(&mut self, q: &DnsQuery) -> Result<DnsAnswer, SysError> {
        nosys()
    }
    /// The TCP/IP host name, qualified.
    fn host_name(&mut self) -> Result<String, SysError> {
        nosys()
    }
    /// The library list.
    fn library_list(&mut self) -> Result<Vec<LibEntry>, SysError> {
        nosys()
    }
    /// Change the library list.
    fn change_library_list(&mut self, change: &LiblChange) -> Result<(), SysError> {
        nosys()
    }
    /// A system value as text; a network attribute with `net`.
    fn system_value(&mut self, name: &str, net: bool) -> Result<String, SysError> {
        nosys()
    }
    /// A data area.
    fn read_data_area(&mut self, name: &str) -> Result<DataArea, SysError> {
        nosys()
    }
    /// Write a data area.
    fn write_data_area(
        &mut self,
        name: &str,
        part: Option<(usize, usize)>,
        value: &str,
    ) -> Result<(), SysError> {
        nosys()
    }
    /// A data queue operation.
    fn data_queue(&mut self, path: &str, op: &DtaqOp) -> Result<DtaqReply, SysError> {
        nosys()
    }
    /// A job's spooled files.
    fn spooled_files(&mut self, job: &str) -> Result<Vec<SplfInfo>, SysError> {
        nosys()
    }
    /// A spooled file's text.
    fn spooled_file(&mut self, job: &str, name: &str, number: SplfNumber) -> Result<Spooled, SysError> {
        nosys()
    }
}

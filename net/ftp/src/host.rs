// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a session needs from the job it runs in: the IFS, sign-on and
//! the profile swap, CL commands, exit programs, database file
//! descriptions and the clock. The job implements [`Host`] over its
//! link to the storage service; tests implement it in memory.

use ifs::client::IfsClient;
use regfac::{ExitAnswer, ExitRequest};

/// A sign-on that was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    /// The message identifier (`CPF1107` ...).
    pub msgid: String,
    /// Its text.
    pub text: String,
}

/// A CL command that ended in an escape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdFail {
    /// The escape message identifier.
    pub msgid: String,
    /// Its first-level text.
    pub text: String,
}

/// What a session needs of a user profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// The profile name.
    pub user: String,
    /// `HOMEDIR`.
    pub home: String,
    /// `CURLIB`, `*CRTDFT` for none.
    pub curlib: String,
    /// The profile's CCSID (`*USER` of `CRTCCSID`), 0 for the system's.
    pub ccsid: u16,
}

/// A database file's description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbFile {
    /// The record length; for a source file the data part (without
    /// sequence number and date).
    pub rec_len: usize,
    /// A source file.
    pub source: bool,
    /// The data CCSID.
    pub ccsid: u16,
    /// A logical file.
    pub logical: bool,
}

/// The job's reach.
pub trait Host {
    /// The IFS, as the session's user once signed on.
    fn ifs(&self) -> &dyn IfsClient;
    /// Check a user and password; the profile name.
    fn sign_on(&mut self, user: &str, password: &str) -> Result<String, Refused>;
    /// A profile's home directory, current library and CCSID.
    fn profile(&mut self, user: &str) -> Option<Profile>;
    /// Run as `user` from now on (`QSYGETPH`, `QWTSETP`): with the
    /// password [`Host::sign_on`] checked, or, with `None`, as a sign-on
    /// exit program named it (the server's profile needs `*USE` to it).
    fn become_user(&mut self, user: &str, password: Option<&str>) -> Result<(), String>;
    /// Run a CL command in the session's job.
    fn command(&mut self, line: &str) -> Result<(), CmdFail>;
    /// Call the exit programs of `point`. An answer that called none
    /// says the point has no programs.
    fn exit(&mut self, point: &str, req: &ExitRequest) -> ExitAnswer;
    /// A database file's description, `None` when there is none.
    fn file(&mut self, lib: &str, file: &str) -> Option<DbFile>;
    /// A system value as text (`QCCSID`).
    fn sysval(&mut self, name: &str) -> Option<String>;
    /// The job's EBCDIC CCSID (`TYPE E`, files `STOR` makes).
    fn job_ccsid(&self) -> u16;
    /// Now, Unix seconds.
    fn now(&self) -> i64;
    /// The local time's offset from UTC, seconds.
    fn utc_offset(&self) -> i64;
}

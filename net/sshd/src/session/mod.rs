// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions: what a `session` channel asks for, and the
//! [`SessionHost`] that runs it as a QS/400 job. The server calls
//! [`SessionHost::run`] on a thread of its own with the channel's byte
//! streams; the host returns the exit status.
//!
//! - [`qsh_host::QshHost`] runs QSH over a [`qsh::Sys`] it is given per
//!   session (the job's, in production; a `MemSys` in tests).
//! - [`transfer::FileTransfer`] wraps any host and answers the `sftp`
//!   subsystem and the legacy `scp -t`/`scp -f` itself, over the user's
//!   IFS.
//! - [`ldisc`] is the line discipline a terminal session needs.

pub mod ldisc;
pub mod qsh_host;
pub mod transfer;

use std::io::{Read, Write};
use std::sync::mpsc::Receiver;

use crate::auth::{Account, Peer};

/// What the session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionKind {
    /// `shell`: the login shell.
    Shell,
    /// `exec`: one command line, run as `qsh -c`.
    Exec(String),
    /// `subsystem`: the name and the configured command.
    Subsystem {
        /// The name (`sftp`).
        name: String,
        /// The `Subsystem` command.
        command: String,
    },
}

/// `pty-req`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pty {
    /// `TERM`.
    pub term: String,
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// The encoded terminal modes.
    pub modes: Vec<u8>,
}

/// A session to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRequest {
    /// The signed-on profile.
    pub account: Account,
    /// What to run.
    pub kind: SessionKind,
    /// The terminal, if one was asked for.
    pub pty: Option<Pty>,
    /// `env` requests the configuration accepted.
    pub env: Vec<(String, String)>,
    /// The client.
    pub peer: Peer,
    /// The server's address and port, for `SSH_CONNECTION`.
    pub local: Peer,
    /// The command the client asked for when `command=` replaced it.
    pub original_command: Option<String>,
}

impl SessionRequest {
    /// The environment OpenSSH gives a session: `USER`, `LOGNAME`,
    /// `HOME`, `SHELL`, `SSH_CLIENT`, `SSH_CONNECTION`, `TERM` with a
    /// terminal, `SSH_ORIGINAL_COMMAND`, then the accepted `env` values.
    #[must_use]
    pub fn environment(&self) -> Vec<(String, String)> {
        let a = &self.account;
        let mut v = vec![
            ("USER".to_string(), a.name.clone()),
            ("LOGNAME".to_string(), a.name.clone()),
            ("HOME".to_string(), a.home.clone()),
            ("SHELL".to_string(), "/usr/bin/qsh".to_string()),
            (
                "SSH_CLIENT".to_string(),
                format!("{} {} {}", self.peer.addr, self.peer.port, self.local.port),
            ),
            (
                "SSH_CONNECTION".to_string(),
                format!(
                    "{} {} {} {}",
                    self.peer.addr, self.peer.port, self.local.addr, self.local.port
                ),
            ),
        ];
        if let Some(p) = &self.pty {
            v.push(("TERM".to_string(), p.term.clone()));
        }
        if let Some(c) = &self.original_command {
            v.push(("SSH_ORIGINAL_COMMAND".to_string(), c.clone()));
        }
        v.extend(self.env.iter().cloned());
        v
    }
}

/// Requests that arrive while a session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    /// `window-change`.
    WindowChange {
        /// Columns.
        cols: u32,
        /// Rows.
        rows: u32,
    },
    /// `signal`: the name without `SIG`.
    Signal(String),
}

/// A session's byte streams. With a terminal, `stderr` is the same
/// stream as `stdout` on the client's side; hosts write both there.
pub struct ChannelIo {
    /// The client's input; end of file at `EOF`.
    pub stdin: Box<dyn Read + Send>,
    /// Output, `CHANNEL_DATA`; blocks while the client's window is full.
    pub stdout: Box<dyn Write + Send>,
    /// Errors, `CHANNEL_EXTENDED_DATA` 1.
    pub stderr: Box<dyn Write + Send>,
    /// `window-change` and `signal`.
    pub control: Receiver<Control>,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// `exit-status`.
    Status(u32),
    /// `exit-signal`: the name without `SIG`.
    Signal(String),
}

/// Runs sessions as QS/400 jobs. Called on a thread per session; the
/// call returns when the job ends.
pub trait SessionHost: Send + Sync {
    /// Run `req` over `io` to its end.
    fn run(&self, req: SessionRequest, io: ChannelIo) -> Exit;
}

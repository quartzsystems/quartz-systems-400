// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSH CMD()` in batch: where output goes (`QIBM_QSH_CMD_OUTPUT`) and
//! which message ends the command (`QIBM_QSH_CMD_ESCAPE_MSG`).

use std::sync::Arc;

use crate::sys::{Fd, OpenFlags, Sys, SysError};

/// `QIBM_QSH_CMD_OUTPUT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmdOutput {
    /// `STDOUT` (the default): the job's terminal or spooled output.
    Stdout,
    /// `NONE`: discarded.
    None,
    /// `FILE=path`: replaces the file.
    File(String),
    /// `FILEAPPEND=path`: appends to the file.
    FileAppend(String),
}

impl CmdOutput {
    /// Interpret the variable's value; anything not understood is
    /// `STDOUT`. Keywords are not case-sensitive.
    pub fn parse(value: Option<&str>) -> Self {
        let Some(v) = value else { return Self::Stdout };
        let v = v.trim();
        let up = v.to_ascii_uppercase();
        if up == "NONE" {
            Self::None
        } else if up.starts_with("FILEAPPEND=") {
            Self::FileAppend(v["FILEAPPEND=".len()..].to_string())
        } else if up.starts_with("FILE=") {
            Self::File(v["FILE=".len()..].to_string())
        } else {
            Self::Stdout
        }
    }

    /// Open the destination: `None` for `STDOUT` (the caller's stream),
    /// the null device for `NONE`, else the file.
    pub fn open(&self, sys: &dyn Sys, cwd: &str) -> Result<Option<Fd>, SysError> {
        match self {
            Self::Stdout => Ok(None),
            Self::None => Ok(Some(crate::sys::stream::Null::fd())),
            Self::File(p) => {
                let flags = OpenFlags {
                    write: true,
                    create: true,
                    truncate: true,
                    ..OpenFlags::default()
                };
                sys.open(&crate::pattern::path::absolute(cwd, p), flags, 0o666)
                    .map(Some)
            }
            Self::FileAppend(p) => {
                let flags = OpenFlags {
                    write: true,
                    create: true,
                    append: true,
                    ..OpenFlags::default()
                };
                sys.open(&crate::pattern::path::absolute(cwd, p), flags, 0o666)
                    .map(Some)
            }
        }
    }
}

/// The kind of message the command ends with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgKind {
    /// A completion message.
    Completion,
    /// An escape message (the CL command fails).
    Escape,
}

/// The message `QSH CMD()` sends when the command ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CmdMessage {
    /// `QSH0005` (ended with a status), `QSH0006` (ended by a signal) or
    /// `QSH0007` (ended by an exception).
    pub id: &'static str,
    /// Completion or escape.
    pub kind: MsgKind,
    /// The exit status or signal number, the message's data.
    pub data: i32,
}

/// How the command ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// Exit status.
    Status(i32),
    /// Ended by a signal.
    Signal(i32),
    /// Ended by an exception (a function check in the shell's job).
    Exception,
}

/// The ending message: `QSH0005` is an escape only when
/// `QIBM_QSH_CMD_ESCAPE_MSG` is `Y` and the status is above zero;
/// `QSH0006` and `QSH0007` are always escapes.
pub fn ending_message(end: Ending, escape_msg: Option<&str>) -> CmdMessage {
    let escape = escape_msg.is_some_and(|v| v.trim().eq_ignore_ascii_case("Y"));
    match end {
        Ending::Status(n) => CmdMessage {
            id: "QSH0005",
            kind: if escape && n > 0 {
                MsgKind::Escape
            } else {
                MsgKind::Completion
            },
            data: n,
        },
        Ending::Signal(s) => CmdMessage {
            id: "QSH0006",
            kind: MsgKind::Escape,
            data: s,
        },
        Ending::Exception => CmdMessage {
            id: "QSH0007",
            kind: MsgKind::Escape,
            data: 0,
        },
    }
}

/// What a batch command produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchResult {
    /// The shell's exit status.
    pub status: i32,
    /// The message to send.
    pub message: CmdMessage,
}

/// Run `QSH CMD(cmd)`: the environment comes from `sys` (the job's
/// environment variables, `QIBM_*` included); standard output and error
/// go where `QIBM_QSH_CMD_OUTPUT` says, `stdout` being the job's own
/// stream for `STDOUT`.
pub fn run_cmd(sys: Arc<dyn Sys>, cmd: &str, stdin: Option<Fd>, stdout: Option<Fd>) -> BatchResult {
    let env = sys.environ();
    let get = |k: &str| {
        env.iter()
            .rev()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
    };
    let output = CmdOutput::parse(get("QIBM_QSH_CMD_OUTPUT").as_deref());
    let cwd = sys.initial_cwd();
    let out = match output.open(&*sys, &cwd) {
        Ok(Some(f)) => Some(f),
        Ok(None) => stdout,
        Err(_) => stdout,
    };
    let inv = super::Invocation::command(cmd);
    let status = super::run(sys, &inv, stdin, out.clone(), out);
    let end = if status > 128 && crate::signal::name(status - 128).is_some() {
        Ending::Signal(status - 128)
    } else {
        Ending::Status(status)
    };
    BatchResult {
        status,
        message: ending_message(end, get("QIBM_QSH_CMD_ESCAPE_MSG").as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_values() {
        assert_eq!(CmdOutput::parse(None), CmdOutput::Stdout);
        assert_eq!(CmdOutput::parse(Some("none")), CmdOutput::None);
        assert_eq!(
            CmdOutput::parse(Some("FILE=/tmp/a")),
            CmdOutput::File("/tmp/a".to_string())
        );
        assert_eq!(
            CmdOutput::parse(Some("fileappend=/tmp/a")),
            CmdOutput::FileAppend("/tmp/a".to_string())
        );
    }

    #[test]
    fn messages() {
        assert_eq!(
            ending_message(Ending::Status(1), None).kind,
            MsgKind::Completion
        );
        assert_eq!(
            ending_message(Ending::Status(1), Some("Y")).kind,
            MsgKind::Escape
        );
        assert_eq!(
            ending_message(Ending::Status(0), Some("Y")).kind,
            MsgKind::Completion
        );
        assert_eq!(ending_message(Ending::Signal(6), None).id, "QSH0006");
    }
}

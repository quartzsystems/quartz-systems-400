// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands (handoff steps 18-20): run a CL command as the signed-on
//! user (in the product's job, under the user's profile) or check it,
//! the results and their messages, a command's definition for the web
//! F4, the recent commands and the saved ones (My commands).

use qsweb::msg::Message;

/// How a command runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum How {
    /// In the user's connection, waiting for it.
    #[default]
    Interactive,
    /// Submitted to a job queue (`SBMJOB`).
    Batch,
}

impl How {
    /// `int` or `batch`, as the form sends it.
    pub fn code(self) -> &'static str {
        match self {
            How::Interactive => "int",
            How::Batch => "batch",
        }
    }

    /// From the form's value.
    pub fn from_code(s: &str) -> Self {
        if s == "batch" { How::Batch } else { How::Interactive }
    }

    /// The word My commands shows.
    pub fn label(self) -> &'static str {
        match self {
            How::Interactive => "Interactive",
            How::Batch => "Batch",
        }
    }
}

/// How a run ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    /// Ran and completed.
    #[default]
    Completed,
    /// Ended with an escape message.
    Failed,
    /// Cancel was asked while it ran.
    Cancelled,
    /// No such command.
    NotFound,
    /// Checked only: no errors.
    Checked,
    /// Checked only: errors.
    CheckFailed,
}

/// A message the command sent, as the job log has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogMessage {
    /// Severity.
    pub severity: u8,
    /// `CPF9801`; empty for an immediate message.
    pub id: String,
    /// First-level text.
    pub text: String,
    /// The sending program.
    pub program: String,
    /// Sent, milliseconds since 1970.
    pub sent_ms: u64,
}

/// One command run (or checked).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ran {
    /// The command as typed.
    pub command: String,
    /// The job it ran in, `number/user/name`.
    pub job: String,
    /// How it ended.
    pub outcome: Outcome,
    /// Its messages, in the order sent.
    pub messages: Vec<LogMessage>,
}

/// A parameter as the web F4 shows it, from the command definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Param {
    /// `SBS`.
    pub keyword: String,
    /// The prompt text.
    pub prompt: String,
    /// The choices text the 5250 prompter shows beside it.
    pub choices: String,
    /// `RSTD(*YES)` values (with the special and single values): a
    /// select; empty for a typed value.
    pub values: Vec<String>,
    /// `DFT`.
    pub default: Option<String>,
    /// `MAX`: above 1 a list.
    pub max: u16,
    /// The field's width.
    pub size: u32,
    /// `MIN` above 0.
    pub required: bool,
    /// Shown only under Additional parameters (F10).
    pub additional: bool,
}

/// A command's definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CmdDef {
    /// `WRKACTJOB`.
    pub name: String,
    /// The library it was found in.
    pub lib: String,
    /// The prompt text (`Work with Active Jobs`).
    pub text: String,
    /// The parameters the prompter shows, in its order.
    pub params: Vec<Param>,
}

/// A saved command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    /// Its name.
    pub name: String,
    /// The command string.
    pub command: String,
    /// Run as.
    pub how: How,
    /// Last run, seconds since 1970; 0 for never.
    pub last: u64,
    /// The group or `*PUBLIC` it is shared with; empty for none.
    pub share: String,
}

/// What the command pages need.
pub trait Cmd: Send + Sync {
    /// Run `command` as `user` and wait (or only check it): what it
    /// did, also kept with [`Cmd::results`].
    fn run(&self, user: &str, command: &str, check: bool) -> Ran;
    /// The runs kept for `user` since the last Clear, oldest first.
    fn results(&self, user: &str) -> Vec<Ran>;
    /// Forget `user`'s results.
    fn clear(&self, user: &str);
    /// Ask `user`'s running command to end; whether one was running.
    fn cancel(&self, user: &str) -> bool;
    /// The job queues `user` may submit to, `lib/name`.
    fn job_queues(&self, user: &str) -> Vec<String>;
    /// Command `name`'s definition (`lib/name` or a bare name found in
    /// the library list), as `user` may see it.
    fn describe(&self, user: &str, name: &str) -> Result<CmdDef, Message>;
    /// `user`'s recent commands, newest first.
    fn recent(&self, user: &str) -> Vec<String>;
    /// Replace `user`'s recent commands.
    fn put_recent(&self, user: &str, commands: &[String]) -> Result<(), Message>;
    /// `user`'s saved commands.
    fn saved(&self, user: &str) -> Vec<Saved>;
    /// Replace `user`'s saved commands.
    fn put_saved(&self, user: &str, saved: &[Saved]) -> Result<(), Message>;
    /// The commands other users shared with any of `to` (groups,
    /// `*PUBLIC`): owner and command.
    fn shared_with(&self, user: &str, to: &[String]) -> Vec<(String, Saved)>;
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shell's diagnostics. Each line is `qsh: <id> <text>`: the
//! identifier is the system's Qshell message number (an interface;
//! numbers not yet confirmed on the oracle are listed in docs/QSH.md),
//! the text is Quartz-written.

use std::fmt;

/// A diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Msg {
    /// A command name found nowhere.
    NotFound(String),
    /// A command found but not runnable: name, reason.
    CannotRun(String, String),
    /// A syntax error: line, detail.
    Syntax(u32, String),
    /// A redirection's file could not be opened: path, reason.
    Open(String, String),
    /// A descriptor that is not open.
    BadFd(String),
    /// `set -u` and an unset parameter.
    Unset(String),
    /// `${name?word}`: name, the word or our default text.
    ParamError(String, String),
    /// Assigning a read-only variable.
    ReadOnly(String),
    /// `${...}` that does not parse at run time.
    BadSubst(String),
    /// An arithmetic error.
    Arith(String),
    /// A `cd` target that cannot be entered: path, reason.
    Cd(String, String),
    /// A builtin's usage error: builtin, detail.
    Usage(String, String),
    /// A builtin's operand that should be a number: builtin, operand.
    NotNumber(String, String),
    /// No such job: builtin, job.
    NoJob(String, String),
    /// A builtin failed for a reason the system gave: builtin, detail.
    Failed(String, String),
    /// A name that is not a valid variable or function name.
    BadName(String, String),
    /// A signal name or number that does not exist: builtin, operand.
    BadSignal(String, String),
}

impl Msg {
    /// The message identifier.
    pub fn id(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "001-0014",
            Self::CannotRun(..) => "001-0019",
            Self::Syntax(..) => "001-0050",
            Self::Open(..) => "001-0023",
            Self::BadFd(_) => "001-0024",
            Self::Unset(_) => "001-0021",
            Self::ParamError(..) => "001-0022",
            Self::ReadOnly(_) => "001-0029",
            Self::BadSubst(_) => "001-0051",
            Self::Arith(_) => "001-0067",
            Self::Cd(..) => "001-0002",
            Self::Usage(..) => "001-0062",
            Self::NotNumber(..) => "001-0068",
            Self::NoJob(..) => "001-0040",
            Self::Failed(..) => "001-0030",
            Self::BadName(..) => "001-0063",
            Self::BadSignal(..) => "001-0045",
        }
    }
}

impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.id())?;
        match self {
            Self::NotFound(c) => write!(f, "Cannot find command {c}."),
            Self::CannotRun(c, why) => write!(f, "Cannot run {c}. {why}"),
            Self::Syntax(line, d) => write!(f, "Line {line} has a syntax error: {d}."),
            Self::Open(p, why) => write!(f, "Cannot open {p}. {why}"),
            Self::BadFd(n) => write!(f, "Descriptor {n} is not open."),
            Self::Unset(n) => write!(f, "Parameter {n} has no value."),
            Self::ParamError(n, w) => write!(f, "{n}: {w}"),
            Self::ReadOnly(n) => write!(f, "Variable {n} is read-only."),
            Self::BadSubst(s) => write!(f, "Substitution {s} is not valid."),
            Self::Arith(d) => write!(f, "Arithmetic failed: {d}."),
            Self::Cd(p, why) => write!(f, "Cannot change to directory {p}. {why}"),
            Self::Usage(b, d) => write!(f, "{b}: {d}"),
            Self::NotNumber(b, v) => write!(f, "{b}: {v} is not a number."),
            Self::NoJob(b, j) => write!(f, "{b}: there is no job {j}."),
            Self::Failed(b, d) => write!(f, "{b}: {d}"),
            Self::BadName(b, n) => write!(f, "{b}: {n} is not a valid name."),
            Self::BadSignal(b, s) => write!(f, "{b}: {s} is not a signal."),
        }
    }
}

/// The line the shell writes to standard error.
pub fn line(m: &Msg) -> String {
    format!("qsh: {m}\n")
}

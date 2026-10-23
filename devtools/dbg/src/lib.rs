// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source debugger's displays and their logic: the debug command
//! language ([`command`]), the source view ([`view`]), breakpoints and
//! watches ([`bkpt`]), the session that applies a command to them and
//! says what the job must do ([`session`]), and the displays laid out
//! as IBM's ([`screen`]). No store and no 5250 I/O: the job's driver
//! (`work/qsjob/src/devtools/dbg.rs`) does those.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bkpt;
pub mod command;
pub mod cond;
pub mod screen;
pub mod session;
pub mod view;

pub use command::{Command, parse};
pub use session::{Effect, Session};
pub use view::SourceView;

/// Why a debug command cannot be done.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DbgError {
    /// Nothing typed.
    #[error("Type a debug command.")]
    Empty,
    /// Not a debug command.
    #[error("{0} is not a debug command.")]
    Unknown(String),
    /// An operand that cannot be read.
    #[error("Operand {0} is not valid here.")]
    Operand(String),
    /// A required operand left out.
    #[error("{0} needs an operand.")]
    Missing(&'static str),
    /// A line outside the view.
    #[error("Line {0} is not in the view.")]
    NoLine(usize),
    /// No statement on or after the line.
    #[error("No statement starts on or after line {0}.")]
    NoStatement(usize),
    /// A command that needs the program stopped.
    #[error("The program is not stopped; {0} cannot be done now.")]
    NotStopped(&'static str),
    /// The module has no source to search or show.
    #[error("The module's source is not available.")]
    NoSource,
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands: definitions compiled from command-definition source into
//! `*CMD` objects, the command analyzer that turns a command string
//! into a parameter block for the command processing program, the
//! parameter block reader those programs use, and the prompter.
//!
//! The statement keywords, parameter types, message identifiers and
//! the prompt display's geometry are IBM's and the compatibility
//! interface; every text is Quartz-written. `docs/CMD.md` describes it.

#![deny(missing_docs)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::struct_excessive_bools
)]

pub mod def;
pub mod error;
pub mod payload;
pub mod rules;
pub mod source;
pub mod token;

pub use def::{CmdDef, Parm, ParmType, Qual};
pub use error::{CmdError, Diag, Result};
pub use payload::CmdObject;

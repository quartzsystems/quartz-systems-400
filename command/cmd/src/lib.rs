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

// The front end (`front`): the tokenizer, the definition model, the
// definition source compiler and its rules, and `analysis`.
#[cfg(feature = "front")]
pub mod analysis;
pub mod def;
pub mod error;
pub mod rules;
pub mod source;
pub mod token;
pub mod value;

// The rest (`lower`): the analyzer, parameter blocks, the *CMD payload,
// the prompter and the store.
#[cfg(feature = "lower")]
pub mod analyze;
#[cfg(feature = "lower")]
pub mod block;
#[cfg(feature = "lower")]
pub mod payload;
#[cfg(feature = "lower")]
pub mod prompt;
#[cfg(feature = "lower")]
pub mod store;

#[cfg(feature = "lower")]
pub use analyze::{Bound, CmdSource, Env, MemSource, Resolved, Setting, analyze};
#[cfg(feature = "lower")]
pub use block::{Built, Parms, Segment};
pub use def::{CmdDef, Parm, ParmType, Qual};
pub use error::{CmdError, Diag, Result};
#[cfg(feature = "lower")]
pub use payload::CmdObject;
pub use token::{Sel, Tok};
pub use value::Value;

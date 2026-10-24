// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG compiler behind `CRTRPGMOD`, `CRTBNDRPG` and
//! `CRTSQLRPGI`: the front end's tree, checked and generated as IR
//! (`QIR.md`), with the listing. Declarations become symbols with
//! IBM's layouts (`decl`, `files`, `syms`, `types`); the control
//! options merge with the command's (`ctl`); one walk over each
//! procedure's calculations checks and generates them (`gen`); the
//! listing is built from what the compile saw (`listing`).

#![deny(missing_docs)]
#![allow(
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::match_same_arms,
    clippy::struct_excessive_bools,
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    clippy::cast_possible_truncation,
    clippy::format_push_string,
    clippy::single_match_else,
    clippy::uninlined_format_args,
    clippy::len_without_is_empty,
    clippy::question_mark,
    clippy::bool_to_int_with_if,
    clippy::needless_pass_by_value
)]

pub mod compile;
pub mod ctl;
pub mod decl;
pub mod extfiles;
pub mod diag;
pub mod dtfmt;
pub mod files;
pub mod codegen;
pub mod listing;
pub mod msgs;
pub mod names;
pub mod reader_class;
pub mod sql;
pub mod syms;
pub mod types;

pub use compile::{Compiled, compile};
pub use ctl::{Command, Options};
pub use files::{ExtFiles, NoExtFiles};
pub use rpgsqlpre as sqlpre;

/// The IBM releases QS/400 V1R0M0 answers for in conditional
/// compilation: every release up to the one it targets, V7R5M0.
pub const RELEASES: &[&str] = &[
    "V4R4M0", "V5R1M0", "V5R2M0", "V5R3M0", "V5R4M0", "V6R1M0", "V7R1M0", "V7R2M0", "V7R3M0", "V7R4M0",
    "V7R5M0",
];

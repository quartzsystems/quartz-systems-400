// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Qshell for QS/400: the command language written from scratch.
//!
//! - [`syntax`]: the lexer, parser and tree with spans.
//! - [`arith`], [`pattern`], [`expand`]: arithmetic, pattern matching
//!   and pathname expansion, word expansion.
//! - [`shell`]: the interpreter; [`builtins`]: the shell's builtins.
//! - [`sys`]: the [`sys::Sys`] seam to everything outside the language
//!   (files, pipes, utilities, child jobs, signals, time), with an
//!   in-memory [`sys::MemSys`] and a std-based [`sys::HostSys`].
//! - [`session`]: the entry points: `qsh -c`, scripts, the line-fed
//!   interactive session and the batch `QSH CMD()` path.
//!
//! `docs/QSH.md` is the reference.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Builtins share one signature; parsers use short names; listings build
// text with format!.
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::unnecessary_wraps,
    clippy::format_push_string,
    clippy::too_many_lines,
    clippy::match_same_arms
)]

pub mod arith;
pub mod builtins;
pub mod expand;
pub mod msg;
pub mod pattern;
pub mod session;
pub mod shell;
pub mod signal;
pub mod syntax;
pub mod sys;

pub use session::{Feed, Interactive, Invocation};
pub use shell::Shell;
pub use sys::{HostSys, MemSys, Sys};

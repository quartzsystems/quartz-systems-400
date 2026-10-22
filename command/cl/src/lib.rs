// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE CL compiler: `CLLE` source into the IR (`QIR.md`), with the
//! compile listing and the debug views. Statements are read by
//! `parse`, checked and typed by `sem`, and lowered by `gen`; every
//! command in the body is analyzed against its `*CMD` definition and
//! either called directly through its processing program with a
//! parameter block built at compile time, or built as a command
//! string at run time (`docs/CL.md`).

#![deny(missing_docs)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::struct_excessive_bools
)]

pub mod ast;
pub mod diag;
pub mod lex;
pub mod parse;

pub use diag::{Diag, Severity};

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG front end: source members in, a syntax tree out. The
//! reader expands `/COPY` and `/INCLUDE`, applies conditional
//! compilation and classifies every line (`reader`); fixed-form
//! specifications are cut with the shared column tables and free-form
//! statements tokenized (`lex`); the parser builds one tree from both
//! forms (`parse`, `ast`). Every problem is an `RNF` diagnostic
//! (`diag`) and parsing goes on after it, so a compile reports all it
//! can, as IBM's compiler does.

#![deny(missing_docs)]
#![allow(
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::match_same_arms,
    clippy::struct_excessive_bools
)]

pub mod ast;
pub mod diag;
pub mod lex;
pub mod parse;
pub mod reader;
pub mod source;

pub use ast::Module;
pub use diag::Diag;
pub use source::{Includes, MemIncludes, Member, NoIncludes};

/// Read and parse a member: the expanded lines, the tree and every
/// diagnostic, in source order.
pub fn front(main: Member, includes: &mut dyn Includes, opts: &reader::Options) -> (reader::Read, Module, Vec<Diag>) {
    let read = reader::read(main, includes, opts);
    let (module, mut diags) = parse::parse(&read);
    diags.extend(read.diags.iter().cloned());
    diags.sort();
    (read, module, diags)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `.qir` text form: a lexer, a parser and a printer. Printing a
//! parsed module and parsing it again gives the same module.

pub mod lex;
mod parse;
mod print;
mod print_ext;

pub use parse::parse;
pub use print::{print, print_type};

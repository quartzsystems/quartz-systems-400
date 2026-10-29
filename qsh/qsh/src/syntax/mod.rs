// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command language's syntax: the tree ([`ast`]), the parser
//! ([`parser`]) and its errors ([`error`]).

pub mod ast;
pub mod error;
pub mod parser;

pub use ast::*;
pub use error::{ParseError, ParseErrorKind};
pub use parser::{Parser, is_name, parse, parse_here_text};

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL for QS/400: the DB2 for i dialect subset parsed into an AST, then
//! planned and run over the database crate. Statement syntax, SQLCODEs,
//! SQLSTATEs and catalog column names are IBM's; every message is ours.

#![deny(missing_docs)]

mod error;

pub mod ast;
pub mod lex;
pub mod parse;

pub use ast::{Naming, Statement};
pub use error::{Result, SqlError};
pub use parse::{parse, parse_all};

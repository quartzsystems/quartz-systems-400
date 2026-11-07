// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL for QS/400: the Db2 for i SQL dialect, 7.5 level, parsed into an AST, then
//! planned and run over the database crate. Statement syntax, SQLCODEs,
//! SQLSTATEs and catalog column names are IBM's; every message is ours.

#![deny(missing_docs)]

// The front end (`front`): the lexer, the tree, the parser and
// `analysis`, over nothing but the decimal type.
mod error;

#[cfg(feature = "front")]
pub mod analysis;
pub mod ast;
pub mod lex;
pub mod parse;
pub mod words;

// The planner and executor over the database (`lower`).
#[cfg(feature = "lower")]
pub mod dbmon;
#[cfg(feature = "lower")]
pub mod embed;
#[cfg(feature = "lower")]
pub mod exec;
#[cfg(feature = "lower")]
pub mod listing;
#[cfg(feature = "lower")]
pub mod options;
#[cfg(feature = "lower")]
pub mod pl;
#[cfg(feature = "lower")]
pub mod prtsqlinf;
#[cfg(feature = "lower")]
pub mod rla;
#[cfg(feature = "lower")]
pub mod run;
#[cfg(feature = "lower")]
pub mod sqlca;
#[cfg(feature = "lower")]
pub mod value;

#[cfg(feature = "front")]
pub use analysis::analyze;
pub use ast::{Naming, Statement};
pub use error::{Result, SqlError};
#[cfg(feature = "lower")]
pub use exec::dynamic::prepare_dynamic;
#[cfg(feature = "lower")]
pub use exec::plan::stats::collect_due as collect_statistics;
#[cfg(feature = "lower")]
pub use exec::{ColumnDesc, Outcome, Prepared, ResultSet, SqlContext, prepare};
pub use parse::{parse, parse_all};
#[cfg(feature = "lower")]
pub use run::{CommitOpt, RunEntry, RunOpts, RunReport, runsqlstm, runsqlstm_lent};
#[cfg(feature = "lower")]
pub use sqlca::Sqlca;
#[cfg(feature = "lower")]
pub use value::Value;

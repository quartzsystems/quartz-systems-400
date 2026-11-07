// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL for QS/400: the Db2 for i SQL dialect, 7.5 level, parsed into an AST, then
//! planned and run over the database crate. Statement syntax, SQLCODEs,
//! SQLSTATEs and catalog column names are IBM's; every message is ours.

#![deny(missing_docs)]

mod error;

pub mod ast;
pub mod dbmon;
pub mod embed;
pub mod exec;
pub mod lex;
pub mod listing;
pub mod options;
pub mod parse;
pub mod pl;
pub mod prtsqlinf;
pub mod rla;
pub mod run;
pub mod sqlca;
pub mod value;

pub use ast::{Naming, Statement};
pub use error::{Result, SqlError};
pub use exec::dynamic::prepare_dynamic;
pub use exec::plan::stats::collect_due as collect_statistics;
pub use exec::{ColumnDesc, Outcome, Prepared, ResultSet, SqlContext, prepare};
pub use parse::{parse, parse_all};
pub use run::{CommitOpt, RunEntry, RunOpts, RunReport, runsqlstm, runsqlstm_lent};
pub use sqlca::Sqlca;
pub use value::Value;

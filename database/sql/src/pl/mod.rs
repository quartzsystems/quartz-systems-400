// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL PL compiler: a routine body to a QIR module whose entry
//! procedure is the routine's program. Control flow and variables are
//! the IR's; every SQL statement, and every expression a control
//! statement evaluates, runs as embedded SQL through the runtime's
//! host-variable calls (`_QRNX_SQLIN`, `_QRNX_SQLOUT`, `_QRNX_SQLRUN`),
//! the variables passed as markers.
//!
//! The program's parameters (PARAMETER STYLE SQL, as IBM's SQL
//! procedures): one per routine parameter in its type's storage, one
//! `SMALLINT` indicator per parameter, the `SQLSTATE` (`char(5)`), the
//! routine name (`varchar(517)`), the specific name (`varchar(128)`), the
//! message text (`varchar(70)`), then, when passed, a 136-byte SQLCA the
//! program fills with its outcome (Quartz's: the caller's runtime reads
//! the SQLCODE, the message and the return status, `SQLERRD(1)`, there).
//!
//! Callers other than `CREATE PROCEDURE` (SQL triggers) give their own
//! [`Signature`] and a [`Catalog`].

mod compile;
pub mod rewrite;
pub mod types;

use db::cmt::LckLvl;

pub use compile::compile;

use crate::ast::{Naming, ObjectName, ParmMode, TypeSpec};
use crate::error::Result;

/// What the program is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A procedure: `RETURN` gives the return status.
    Procedure,
    /// A trigger: no `RETURN` value.
    Trigger,
}

/// One parameter of the routine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigParm {
    /// Its name.
    pub name: String,
    /// Its mode.
    pub mode: ParmMode,
    /// Its type.
    pub ty: TypeSpec,
}

/// The routine a body belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// The program (and module) name: the scope of its statements.
    pub program: String,
    /// The routine's SQL name, a qualifier of its parameters.
    pub routine: String,
    /// The parameters.
    pub parms: Vec<SigParm>,
    /// Procedure or trigger.
    pub role: Role,
    /// The naming of its statements.
    pub naming: Naming,
    /// `SET OPTION COMMIT`: `None` for `*NONE`.
    pub commit: Option<LckLvl>,
    /// Parameters reached only qualified by a label (a trigger's
    /// transition variables, `N.COL`).
    pub groups: Vec<ParmGroup>,
}

/// Parameters named under a label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParmGroup {
    /// The label (a correlation name).
    pub label: String,
    /// The names and the parameters they stand for.
    pub names: Vec<(String, usize)>,
    /// They may not be set.
    pub read_only: bool,
}

/// The procedure a trigger's body compiles to; the program's entry
/// calls it.
pub const TRIGGER_BODY: &str = "TRGBODY";

/// What the compiler asks of the database while it compiles.
pub trait Catalog {
    /// The column names of a table or view; empty when it does not
    /// exist (yet).
    fn columns(&mut self, table: &ObjectName) -> Vec<String>;
    /// The result columns of a query (text with `?` markers for the
    /// routine's variables), for `FOR` loops.
    ///
    /// # Errors
    ///
    /// The query does not describe.
    fn describe(&mut self, query: &str) -> Result<Vec<(String, TypeSpec)>>;
}

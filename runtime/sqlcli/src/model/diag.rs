// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Diagnostic records: what the last function on a handle left for
//! `SQLGetDiagRec` and `SQLError`. Every function but those clears its
//! handle's records first.

use sql::Sqlca;

use crate::consts::{CLI_NATIVE, rc};

/// One record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    /// SQLSTATE.
    pub state: String,
    /// The native error: the SQLCODE, or `-99999` for what the CLI found.
    pub native: i32,
    /// The message text.
    pub text: String,
}

impl Diag {
    /// A condition the CLI found.
    #[must_use]
    pub fn cli(state: &str, text: &str) -> Self {
        Self { state: state.into(), native: CLI_NATIVE, text: text.into() }
    }

    /// The record a statement's SQLCA makes.
    #[must_use]
    pub fn of(ca: &Sqlca) -> Self {
        let text = if ca.sqlerrmc.is_empty() { format!("{} occurred.", ca.msgid()) } else { ca.sqlerrmc.clone() };
        Self { state: ca.sqlstate.clone(), native: ca.sqlcode, text }
    }
}

/// A handle's records and the header fields of the last function.
#[derive(Clone, Debug, Default)]
pub struct Diags {
    /// The records, first first.
    pub recs: Vec<Diag>,
    /// The last function's return code.
    pub rc: i32,
    /// The rows the last statement changed.
    pub row_count: i32,
}

impl Diags {
    /// Forget the last function's.
    pub fn clear(&mut self) {
        self.recs.clear();
        self.rc = rc::SUCCESS;
        self.row_count = 0;
    }

    /// Add a record.
    pub fn push(&mut self, d: Diag) {
        self.recs.push(d);
    }

    /// `SQLError`: the first record, taken.
    pub fn take(&mut self) -> Option<Diag> {
        if self.recs.is_empty() { None } else { Some(self.recs.remove(0)) }
    }
}

/// The failure of a function: its records, already on the handle.
pub type Failed = i32;

/// What a function comes to: a return code with records already put on
/// the handle.
pub type CliResult<T> = Result<T, Failed>;

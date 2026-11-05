// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Warnings with SQLCODE 0 a statement raises as it runs, deep in the
//! value arithmetic: `01506` (a date adjusted to the end of its month)
//! and `01004` (a string truncated). The first one noted stands; the
//! statement's runner takes it and puts it in the SQLCA.

use std::cell::Cell;

/// A date or timestamp's day was moved to the end of its month.
pub const DATE_ADJUSTED: &str = "01506";
/// A string's characters (other than blanks) were cut off.
pub const TRUNCATED: &str = "01004";

thread_local! {
    static NOTED: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Note a warning for the running statement; an earlier one stands.
pub fn note(state: &'static str) {
    NOTED.with(|n| {
        if n.get().is_none() {
            n.set(Some(state));
        }
    });
}

/// The warning noted since the last call, taken.
#[must_use]
pub fn take() -> Option<&'static str> {
    NOTED.with(Cell::take)
}

/// Put a noted warning in an SQLCA that has none: its SQLSTATE and the
/// `SQLWARN` flags (`SQLWARN1` for a truncation, `SQLWARN6` for a date).
pub fn apply(state: &'static str, ca: &mut crate::sqlca::Sqlca) {
    if ca.sqlcode != 0 || ca.sqlstate != "00000" {
        return;
    }
    ca.sqlstate = state.into();
    ca.sqlwarn[0] = 'W';
    match state {
        TRUNCATED => ca.sqlwarn[1] = 'W',
        DATE_ADJUSTED => ca.sqlwarn[6] = 'W',
        _ => {}
    }
}

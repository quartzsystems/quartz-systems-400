// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL communication area every statement fills: the fields a
//! program's SQLCA shows, with IBM's codes and states.

use crate::error::SqlError;
use crate::exec::Outcome;

/// The SQLCA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sqlca {
    /// `SQLCODE`: 0, +100 for no rows, negative for an error.
    pub sqlcode: i32,
    /// `SQLSTATE`.
    pub sqlstate: String,
    /// `SQLERRD`; `[2]` is the row count.
    pub sqlerrd: [i32; 6],
    /// `SQLWARN0` to `SQLWARNA`.
    pub sqlwarn: [char; 11],
    /// `SQLERRMC`: the message tokens, here the message text.
    pub sqlerrmc: String,
    /// `SQLERRP`: the product and module.
    pub sqlerrp: String,
}

impl Default for Sqlca {
    fn default() -> Self {
        Self {
            sqlcode: 0,
            sqlstate: "00000".into(),
            sqlerrd: [0; 6],
            sqlwarn: [' '; 11],
            sqlerrmc: String::new(),
            sqlerrp: "QSQ400".into(),
        }
    }
}

impl Sqlca {
    /// Success with `rows` affected or returned.
    #[must_use]
    pub fn rows(rows: u64) -> Self {
        let mut c = Self::default();
        c.sqlerrd[2] = i32::try_from(rows).unwrap_or(i32::MAX);
        c
    }

    /// `+100`: no row qualified.
    #[must_use]
    pub fn no_data() -> Self {
        Self {
            sqlcode: 100,
            sqlstate: "02000".into(),
            ..Self::default()
        }
    }

    /// From an error.
    #[must_use]
    pub fn from_error(e: &SqlError) -> Self {
        Self {
            sqlcode: e.sqlcode(),
            sqlstate: e.sqlstate().into(),
            sqlerrmc: e.to_string(),
            ..Self::default()
        }
    }

    /// From an outcome: a change of no rows and an empty result set are
    /// `+100`.
    #[must_use]
    pub fn from_outcome(o: &Outcome) -> Self {
        match o {
            Outcome::Count(0) => Self::no_data(),
            Outcome::Count(n) => Self::rows(*n),
            Outcome::Rows(rs) if rs.rows.is_empty() => Self::no_data(),
            Outcome::Rows(rs) => Self::rows(rs.rows.len() as u64),
            Outcome::Done => Self::default(),
        }
    }

    /// Whether the statement failed.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.sqlcode < 0
    }

    /// The message ID: `SQL` plus the code's four digits; `SQL0000` for
    /// success.
    #[must_use]
    pub fn msgid(&self) -> String {
        format!("SQL{:04}", self.sqlcode.unsigned_abs())
    }

    /// One line as a listing shows it.
    #[must_use]
    pub fn line(&self) -> String {
        if self.sqlcode == 0 {
            return format!("SQLCODE 0  rows {}", self.sqlerrd[2]);
        }
        format!(
            "{} SQLCODE {} SQLSTATE {}{}",
            self.msgid(),
            self.sqlcode,
            self.sqlstate,
            if self.sqlerrmc.is_empty() {
                String::new()
            } else {
                format!("  {}", self.sqlerrmc)
            }
        )
    }
}

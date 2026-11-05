// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GRANT` and `REVOKE`.

use super::{Ident, ObjectName};

/// One privilege as written, with its columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Privilege {
    /// `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `ALTER`, `INDEX`,
    /// `REFERENCES`, `EXECUTE`, `USAGE`, `READ`, `WRITE`, `CREATEIN`, ...
    pub name: String,
    /// `UPDATE (cols)`, `REFERENCES (cols)`.
    pub columns: Vec<Ident>,
}

/// What the privileges are on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrantObject {
    /// `[TABLE] name, ...`: tables and views.
    Tables(Vec<ObjectName>),
    /// Another kind: `SCHEMA`, `PROCEDURE`, `FUNCTION`, `SPECIFIC
    /// PROCEDURE`, `SPECIFIC FUNCTION`, `SEQUENCE`, `VARIABLE`, `TYPE`,
    /// `PACKAGE`, with the names.
    Other {
        /// The kind as written.
        kind: String,
        /// The names.
        names: Vec<ObjectName>,
    },
}

/// A grantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Grantee {
    /// `PUBLIC`.
    Public,
    /// A user profile (`USER x` or `x`).
    User(Ident),
}

/// `GRANT ... TO` or `REVOKE ... FROM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// `ALL [PRIVILEGES]`.
    pub all: bool,
    /// The privileges named, empty for `ALL`.
    pub privileges: Vec<Privilege>,
    /// On what.
    pub object: GrantObject,
    /// To or from whom.
    pub grantees: Vec<Grantee>,
    /// `WITH GRANT OPTION` (`GRANT` only).
    pub grant_option: bool,
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the parser and the executor share: the special registers and
//! built-in global variables the parser knows by name, the clauses a
//! JSON call keeps, and a few names and limits. They live here so the
//! parser builds without the executor (the `front` feature); the
//! executor's modules re-export them where they were.

use crate::error::SqlError;

/// The hidden column of a derived table: each row's place in its order.
pub const ORDER_COLUMN: &str = "*ORD";

/// The largest cardinality of an array type declared `ARRAY[]`.
pub const UNBOUNDED: u64 = 2_147_483_647;

/// The registers `CURRENT x` names beyond the parser's own, by the word
/// after `CURRENT` (or the underscore spelling).
pub const CURRENT: &[(&str, &str)] = &[
    ("DEGREE", "CURRENT DEGREE"),
    ("CLIENT_ACCTNG", "CURRENT CLIENT_ACCTNG"),
    ("CLIENT_APPLNAME", "CURRENT CLIENT_APPLNAME"),
    ("CLIENT_PROGRAMID", "CURRENT CLIENT_PROGRAMID"),
    ("CLIENT_USERID", "CURRENT CLIENT_USERID"),
    ("CLIENT_WRKSTNNAME", "CURRENT CLIENT_WRKSTNNAME"),
];

/// Built-in global variables by schema and name.
pub const GLOBALS: &[(&str, &str, &str)] = &[
    ("QSYS2", "JOB_NAME", "QSYS2.JOB_NAME"),
    (
        "QSYS2",
        "SERVER_MODE_JOB_NAME",
        "QSYS2.SERVER_MODE_JOB_NAME",
    ),
    ("QSYS2", "PROCESS_ID", "QSYS2.PROCESS_ID"),
    ("QSYS2", "THREAD_ID", "QSYS2.THREAD_ID"),
    ("SYSIBM", "CLIENT_HOST", "SYSIBM.CLIENT_HOST"),
    ("SYSIBM", "CLIENT_IPADDR", "SYSIBM.CLIENT_IPADDR"),
    ("SYSIBM", "CLIENT_PORT", "SYSIBM.CLIENT_PORT"),
    ("SYSIBM", "PACKAGE_NAME", "SYSIBM.PACKAGE_NAME"),
    ("SYSIBM", "PACKAGE_SCHEMA", "SYSIBM.PACKAGE_SCHEMA"),
    ("SYSIBM", "PACKAGE_VERSION", "SYSIBM.PACKAGE_VERSION"),
    ("SYSIBM", "ROUTINE_SCHEMA", "SYSIBM.ROUTINE_SCHEMA"),
    (
        "SYSIBM",
        "ROUTINE_SPECIFIC_NAME",
        "SYSIBM.ROUTINE_SPECIFIC_NAME",
    ),
    ("SYSIBM", "ROUTINE_TYPE", "SYSIBM.ROUTINE_TYPE"),
];

/// The canonical name of a built-in global variable.
#[must_use]
pub fn global(schema: Option<&str>, name: &str) -> Option<&'static str> {
    GLOBALS
        .iter()
        .find(|(s, n, _)| *n == name && schema.is_none_or(|x| x == *s))
        .map(|(_, _, c)| *c)
}

/// `SQL0604` for `FOR MIXED DATA`: CCSID 37 has no mixed form.
pub(crate) fn no_mixed() -> SqlError {
    SqlError::Coded {
        code: -604,
        state: "42611",
        what: "FOR MIXED DATA needs a mixed CCSID; the job's CCSID 37 has none".into(),
    }
}

/// The clauses of a JSON call: `name=value` pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opts(Vec<(String, String)>);

impl Opts {
    /// A clause's value.
    #[must_use]
    pub fn get(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }

    /// A clause's value, or `default`.
    #[must_use]
    pub fn or<'a>(&'a self, k: &str, default: &'a str) -> &'a str {
        self.get(k).unwrap_or(default)
    }

    /// Set a clause.
    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        self.0.retain(|(n, _)| n != k);
        self.0.push((k.into(), v.into()));
    }

    /// The text kept in the call.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.0 {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push(';');
        }
        s
    }

    /// From that text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self(
            text.split(';')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL precompiler behind `CRTSQLRPGI`. Each `EXEC SQL` statement
//! of a member is read for its kind, its host variables and the text
//! the SQL engine runs (input host variables as `?`, `INTO` lists
//! taken out), checked (syntax, host variables, tables) and handed to
//! the RPG compiler as a [`Stmt`]: the compiler resolves the host
//! variables against its symbols and calls the runtime with the
//! statement's [`sql::embed::Desc`]. `SET OPTION`, `WHENEVER`,
//! `DECLARE CURSOR` and `INCLUDE` act at compile time.
//!
//! The precompiler runs inside the compiler rather than writing a
//! second source member: the compiler already knows every host
//! variable's type, the source keeps its own line numbers, and the
//! listing numbers the lines IBM's generated source would add
//! ([`lines`]) so diagnostics land where the oracle puts them.

#![deny(missing_docs)]
#![allow(clippy::too_many_lines, clippy::struct_excessive_bools, clippy::many_single_char_names)]

pub mod lines;
pub mod listing;
pub mod msgs;
pub mod scan;
mod stmt;

use std::collections::HashMap;

pub use sql::embed::{Desc, Op};

/// One `EXEC SQL` statement of the source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The line it starts on.
    pub line: u32,
    /// The text after `EXEC SQL`, one `\n` per line break.
    pub text: String,
    /// The scope: 0 for the main source, a procedure's otherwise.
    pub scope: usize,
}

/// What the compiler knows of a host variable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostInfo {
    /// Subfields, for a structure; 0 for a scalar.
    pub fields: u32,
    /// Elements, for an array; 0 otherwise.
    pub dim: u32,
}

/// The compiler's side of a precompile.
pub trait Env {
    /// A host variable (`NAME` or `DS.SUB`) in a scope.
    fn host(&self, scope: usize, name: &str) -> Option<HostInfo>;
    /// Whether a table can be found (`schema` none: the library list).
    fn table(&mut self, schema: Option<&str>, name: &str) -> bool;
}

/// The command's SQL options and those `SET OPTION` gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `COMMIT`.
    pub commit: String,
    /// `NAMING`: `*SYS` or `*SQL`.
    pub naming: String,
    /// `DATFMT`.
    pub datfmt: String,
    /// `DATSEP`.
    pub datsep: String,
    /// `TIMFMT`.
    pub timfmt: String,
    /// `TIMSEP`.
    pub timsep: String,
    /// `CLOSQLCSR`.
    pub closqlcsr: String,
    /// `GENLVL`.
    pub genlvl: u8,
    /// Other options, as given.
    pub other: Vec<(String, String)>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            commit: "*CHG".into(),
            naming: "*SYS".into(),
            datfmt: "*JOB".into(),
            datsep: "*JOB".into(),
            timfmt: "*HMS".into(),
            timsep: "*JOB".into(),
            closqlcsr: "*ENDACTGRP".into(),
            genlvl: 10,
            other: Vec::new(),
        }
    }
}

impl Options {
    /// `*SQL` naming.
    #[must_use]
    pub fn sql_naming(&self) -> bool {
        self.naming == "*SQL"
    }
}

/// A host variable a statement uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostRef {
    /// The variable, upper case (`NAME`, `DS.SUB`).
    pub name: String,
    /// Its indicator variable.
    pub ind: Option<String>,
    /// The line it is on.
    pub line: u32,
}

/// `FOR n ROWS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rows {
    /// A number.
    Lit(i64),
    /// A host variable.
    Host(HostRef),
}

/// The `WHENEVER` actions in effect at a statement: the label to go
/// to, none for `CONTINUE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Whenever {
    /// `SQLERROR`.
    pub error: Option<String>,
    /// `SQLWARNING`.
    pub warning: Option<String>,
    /// `NOT FOUND`.
    pub not_found: Option<String>,
}

/// A statement, precompiled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stmt {
    /// Its first line.
    pub line: u32,
    /// Its last line.
    pub last_line: u32,
    /// The scope it is in.
    pub scope: usize,
    /// The statement's verb as the listing shows it (`SELECT`, `FETCH`).
    pub verb: String,
    /// What the runtime runs.
    pub desc: Desc,
    /// Input host variables, in order.
    pub inputs: Vec<HostRef>,
    /// Output host variables, in order.
    pub outputs: Vec<HostRef>,
    /// `FOR n ROWS`.
    pub rows: Option<Rows>,
    /// `WHENEVER` in effect after it runs.
    pub whenever: Whenever,
    /// IBM's generated source declares a structure for it.
    pub ds: bool,
    /// Input host variables and indicators, structures counted by
    /// their subfields.
    pub n_in: u32,
    /// Output host variables and indicators, counted the same way.
    pub n_out: u32,
}

/// A precompile diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diag {
    /// The source line (record).
    pub line: u32,
    /// `SQLnnnn`.
    pub msgid: &'static str,
    /// Severity.
    pub sev: u8,
    /// The message values.
    pub args: Vec<String>,
}

/// What a precompile produced.
#[derive(Clone, Debug, Default)]
pub struct Precompiled {
    /// Every statement, in source order.
    pub stmts: Vec<Stmt>,
    /// The diagnostics, by line.
    pub diags: Vec<Diag>,
    /// The options in force.
    pub options: Options,
    /// The highest severity.
    pub max_severity: u8,
    /// Whether the compile goes on.
    pub ok: bool,
}

/// A declared cursor.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cursor {
    pub text: String,
    pub inputs: Vec<HostRef>,
    pub hold: bool,
    pub scroll: bool,
    pub stmt: String,
}

/// Precompile a member's statements.
pub fn precompile(srcs: &[Source], env: &mut dyn Env, opts: &Options) -> Precompiled {
    let mut st = stmt::State {
        env,
        opts: opts.clone(),
        cursors: HashMap::new(),
        whenever: Whenever::default(),
        diags: Vec::new(),
        first: true,
    };
    let mut stmts = Vec::new();
    for s in srcs {
        if let Some(mut x) = st.statement(s) {
            x.n_in = count(st.env, x.scope, &x.inputs);
            x.n_out = count(st.env, x.scope, &x.outputs);
            stmts.push(x);
        }
        st.first = false;
    }
    let mut diags = std::mem::take(&mut st.diags);
    diags.sort();
    diags.dedup();
    let max_severity = diags.iter().map(|d| d.sev).max().unwrap_or(0);
    Precompiled {
        stmts,
        diags,
        ok: max_severity <= st.opts.genlvl,
        options: st.opts,
        max_severity,
    }
}

/// Host variables as IBM's generated structure holds them: one field
/// per scalar, per subfield of a structure, per indicator.
fn count(env: &dyn Env, scope: usize, hosts: &[HostRef]) -> u32 {
    hosts
        .iter()
        .map(|h| {
            let v = env.host(scope, &h.name).map_or(1, |i| i.fields.max(1));
            let i = h.ind.as_ref().map_or(0, |n| env.host(scope, n).map_or(1, |i| i.dim.max(1)));
            v + i
        })
        .sum()
}

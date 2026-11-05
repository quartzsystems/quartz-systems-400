// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The handles of a job: one environment, its connections and their
//! statements, numbered from 1 in one sequence, a freed number given
//! out again first (the oracle's order: environment 1, connection 2,
//! statement 3).

use std::collections::BTreeMap;

use super::diag::Diags;
use crate::data::{ColDesc, ParamDesc};
use crate::server::Runner;

/// A pointer argument as the program passed it; `None` for null.
pub type Ptr = Option<u128>;

/// A column bound with `SQLBindCol`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColBind {
    /// The C type of the buffer.
    pub ctype: i16,
    /// The buffer.
    pub data: Ptr,
    /// Its length (for decimals, precision * 256 + scale).
    pub max: i32,
    /// The length and indicator word.
    pub ind: Ptr,
}

/// A parameter bound with `SQLBindParam` or `SQLBindParameter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParamBind {
    /// Input, output or both.
    pub io: i16,
    /// The C type of the buffer.
    pub ctype: i16,
    /// The SQL type the program named.
    pub stype: i16,
    /// The SQL type's length or precision.
    pub len: i32,
    /// Its scale.
    pub scale: i16,
    /// The buffer.
    pub data: Ptr,
    /// The buffer's length (`SQLBindParameter`), 0 when not given.
    pub max: i32,
    /// The length and indicator word.
    pub ind: Ptr,
}

/// The environment.
#[derive(Debug)]
pub struct Env {
    /// Character output ends with a NUL.
    pub output_nts: bool,
    /// Server mode.
    pub server_mode: bool,
    /// System naming for new connections.
    pub sys_naming: bool,
    /// The default library for new connections.
    pub default_lib: String,
    /// How many times `SQLAllocEnv` gave the handle out.
    pub count: i32,
    /// Its diagnostics.
    pub diags: Diags,
}

impl Default for Env {
    fn default() -> Self {
        Self { output_nts: true, server_mode: false, sys_naming: false, default_lib: String::new(), count: 1, diags: Diags::default() }
    }
}

/// A connection.
pub struct Dbc {
    /// Commit after each statement.
    pub autocommit: bool,
    /// The commitment control level (`SQL_TXN_*`).
    pub commit: i32,
    /// System naming.
    pub sys_naming: bool,
    /// The default library for unqualified names; blank for the
    /// naming's own.
    pub default_lib: String,
    /// Connected.
    pub connected: bool,
    /// Where its statements run in server mode.
    pub server: Option<Box<dyn Runner>>,
    /// The scope its statements are prepared in.
    pub scope: String,
    /// Its diagnostics.
    pub diags: Diags,
}

impl std::fmt::Debug for Dbc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dbc").field("connected", &self.connected).field("scope", &self.scope).finish_non_exhaustive()
    }
}

impl Dbc {
    /// A connection not yet connected, with the environment's naming.
    #[must_use]
    pub fn new(env: &Env, number: i32) -> Self {
        Self {
            autocommit: false,
            commit: crate::consts::txn::READ_UNCOMMITTED,
            sys_naming: env.sys_naming,
            default_lib: env.default_lib.clone(),
            connected: false,
            server: None,
            scope: format!("CLI:{number}"),
            diags: Diags::default(),
        }
    }
}

/// Where a statement is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StmtState {
    /// Nothing prepared.
    #[default]
    Allocated,
    /// Prepared (or run without a cursor).
    Prepared,
    /// A cursor is open.
    Open,
}

/// A statement.
#[derive(Debug, Default)]
pub struct Stmt {
    /// Its connection.
    pub dbc: i32,
    /// Where it is.
    pub state: StmtState,
    /// Its result columns.
    pub cols: Vec<ColDesc>,
    /// Its parameter markers.
    pub params: Vec<ParamDesc>,
    /// Columns bound, by number.
    pub col_binds: BTreeMap<u16, ColBind>,
    /// Parameters bound, by number.
    pub param_binds: BTreeMap<u16, ParamBind>,
    /// The cursor's name in the connection's scope.
    pub cursor: String,
    /// The current row, for `SQLGetData`.
    pub row: Option<Vec<sql::Value>>,
    /// The columns `SQLGetData` already took from the row.
    pub got: Vec<u16>,
    /// The rows the last statement changed; -1 after a query.
    pub row_count: i32,
    /// A scrollable cursor.
    pub scrollable: bool,
    /// `WITH HOLD`.
    pub hold: bool,
    /// `FOR FETCH ONLY`.
    pub fetch_only: bool,
    /// Its diagnostics.
    pub diags: Diags,
}

/// A handle's object.
#[derive(Debug)]
pub enum Handle {
    /// The environment.
    Env(Env),
    /// A connection.
    Dbc(Dbc),
    /// A statement.
    Stmt(Stmt),
}

/// Every handle of a job.
#[derive(Debug, Default)]
pub struct Handles {
    map: BTreeMap<i32, Handle>,
}

impl Handles {
    /// The lowest number not in use.
    fn next(&self) -> i32 {
        let mut n = 1;
        while self.map.contains_key(&n) {
            n += 1;
        }
        n
    }

    /// Give `h` a number.
    pub fn add(&mut self, h: Handle) -> i32 {
        let n = self.next();
        self.map.insert(n, h);
        n
    }

    /// The number the next [`Handles::add`] gives.
    #[must_use]
    pub fn peek(&self) -> i32 {
        self.next()
    }

    /// Forget a handle.
    pub fn remove(&mut self, n: i32) -> Option<Handle> {
        self.map.remove(&n)
    }

    /// The environment's number, if there is one.
    #[must_use]
    pub fn env_number(&self) -> Option<i32> {
        self.map.iter().find(|(_, h)| matches!(h, Handle::Env(_))).map(|(n, _)| *n)
    }

    /// The environment.
    pub fn env(&mut self, n: i32) -> Option<&mut Env> {
        match self.map.get_mut(&n) {
            Some(Handle::Env(e)) => Some(e),
            _ => None,
        }
    }

    /// A connection.
    pub fn dbc(&mut self, n: i32) -> Option<&mut Dbc> {
        match self.map.get_mut(&n) {
            Some(Handle::Dbc(d)) => Some(d),
            _ => None,
        }
    }

    /// A statement.
    pub fn stmt(&mut self, n: i32) -> Option<&mut Stmt> {
        match self.map.get_mut(&n) {
            Some(Handle::Stmt(s)) => Some(s),
            _ => None,
        }
    }

    /// Whether `n` is a handle of type `t` (`htype`).
    #[must_use]
    pub fn is(&self, n: i32, t: i16) -> bool {
        use crate::consts::htype;
        matches!(
            (self.map.get(&n), t),
            (Some(Handle::Env(_)), htype::ENV) | (Some(Handle::Dbc(_)), htype::DBC) | (Some(Handle::Stmt(_)), htype::STMT)
        )
    }

    /// The connections.
    pub fn dbcs(&self) -> impl Iterator<Item = i32> + '_ {
        self.map.iter().filter(|(_, h)| matches!(h, Handle::Dbc(_))).map(|(n, _)| *n)
    }

    /// A connection's statements.
    pub fn stmts_of(&self, dbc: i32) -> Vec<i32> {
        self.map
            .iter()
            .filter(|(_, h)| matches!(h, Handle::Stmt(s) if s.dbc == dbc))
            .map(|(n, _)| *n)
            .collect()
    }

    /// A handle's diagnostics.
    pub fn diags(&mut self, n: i32) -> Option<&mut Diags> {
        match self.map.get_mut(&n)? {
            Handle::Env(e) => Some(&mut e.diags),
            Handle::Dbc(d) => Some(&mut d.diags),
            Handle::Stmt(s) => Some(&mut s.diags),
        }
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CLI's functions over a job's handles. Each takes the handle
//! numbers and pointers as the program passed them and answers the
//! return code, its diagnostics left on the handle. Statements run
//! through the connection's [`Runner`](crate::server::Runner) in server
//! mode, else in the job ([`Host::run`]), as dynamic SQL: `PREPARE`,
//! `DESCRIBE`, `EXECUTE` or `OPEN`, `FETCH`, `CLOSE` in the
//! connection's scope (`CLI:n`), the statement prepared as `Sn`, its
//! cursor `CLICURn`.

use sql::Value;
use sql::embed::{Desc, Reply, Request};
use db::cmt::LckLvl;
use sql::Sqlca;

use crate::consts::{rc, txn};
use crate::model::diag::Diag;
use crate::model::handles::Handles;

pub mod catalog;
pub mod conn;
pub mod diag;
pub mod env;
pub mod host;
pub mod stmt;

pub use host::Host;

/// A job's CLI.
#[derive(Debug, Default)]
pub struct Cli {
    /// Its handles.
    pub handles: Handles,
}

/// The return code a statement's SQLCA comes to, with the record it
/// leaves (none for success and for no data).
#[must_use]
pub fn outcome(ca: &Sqlca) -> (i32, Option<Diag>) {
    match ca.sqlcode {
        0 => (rc::SUCCESS, None),
        100 => (rc::NO_DATA, None),
        n if n < 0 => (rc::ERROR, Some(Diag::of(ca))),
        _ => (rc::SUCCESS_WITH_INFO, Some(Diag::of(ca))),
    }
}

impl Cli {
    /// Start a function on handle `h`: its records cleared. False when
    /// it is not a handle.
    pub(crate) fn begin(&mut self, h: i32) -> bool {
        match self.handles.diags(h) {
            Some(d) => {
                d.clear();
                true
            }
            None => false,
        }
    }

    /// Leave a record on `h` and answer `code`.
    pub(crate) fn note(&mut self, h: i32, code: i32, d: Diag) -> i32 {
        if let Some(x) = self.handles.diags(h) {
            x.push(d);
            x.rc = code;
        }
        code
    }

    /// Fail a function on `h`.
    pub(crate) fn fail(&mut self, h: i32, state: &str, text: &str) -> i32 {
        self.note(h, rc::ERROR, Diag::cli(state, text))
    }

    /// The return code of a reply, its record left on `h`.
    pub(crate) fn answer(&mut self, h: i32, ca: &Sqlca) -> i32 {
        match outcome(ca) {
            (code, Some(d)) => self.note(h, code, d),
            (code, None) => code,
        }
    }

    /// A statement's description for connection `dbc`: its naming and
    /// commitment control.
    pub(crate) fn desc(&mut self, dbc: i32, op: sql::embed::Op) -> Desc {
        let Some(d) = self.handles.dbc(dbc) else { return Desc { op, ..Desc::default() } };
        Desc { op, sql_naming: !d.sys_naming, commit: level(d.commit), calls: true, ..Desc::default() }
    }

    /// Run a statement on connection `dbc`.
    pub(crate) fn run(&mut self, host: &mut dyn Host, dbc: i32, desc: Desc, inputs: Vec<Value>) -> (Reply, Vec<(usize, Value)>) {
        let Some(d) = self.handles.dbc(dbc) else { return (Reply::default(), Vec::new()) };
        let req = Request { scope: d.scope.clone(), desc, inputs };
        match d.server.as_mut() {
            Some(r) => r.run(&req),
            None => host.run(&req),
        }
    }

    /// `COMMIT` after a statement when the connection commits each one.
    pub(crate) fn autocommit(&mut self, host: &mut dyn Host, dbc: i32) -> Option<Sqlca> {
        let d = self.handles.dbc(dbc)?;
        if !d.autocommit || d.commit == txn::NO_COMMIT {
            return None;
        }
        let mut desc = self.desc(dbc, sql::embed::Op::Exec);
        desc.text = "COMMIT".into();
        let (r, _) = self.run(host, dbc, desc, Vec::new());
        r.sqlca.is_error().then_some(r.sqlca)
    }
}

/// The lock level of a `SQL_TXN_*` level; `None` for no commitment control.
#[must_use]
pub fn level(commit: i32) -> Option<LckLvl> {
    match commit {
        txn::READ_UNCOMMITTED => Some(LckLvl::Chg),
        txn::READ_COMMITTED => Some(LckLvl::Cs),
        txn::REPEATABLE_READ | txn::SERIALIZABLE => Some(LckLvl::All),
        _ => None,
    }
}

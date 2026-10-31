// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operating system's objects: what QS00SS1 `*BASE` puts in `QSYS`
//! (and the shipped work management objects of `QGPL` and `QUSRSYS`).
//! One function makes them, so the first IPL and `build lp` (which
//! saves them as the product load) make the same objects. User
//! profiles, system values, network attributes and the TCP/IP
//! configuration are the system's own and stay with the first IPL.

#![deny(missing_docs)]

pub mod display;
pub mod qstrup;
pub mod ui;

use store::obj::ObjectId;
use store::txn::Txn;
use thiserror::Error;

/// A piece that could not be made.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{what}: {why}")]
pub struct Error {
    /// Which piece.
    pub what: &'static str,
    /// Why.
    pub why: String,
}

fn piece<T, E: std::fmt::Display>(what: &'static str, r: Result<T, E>) -> Result<T, Error> {
    r.map_err(|e| Error {
        what,
        why: e.to_string(),
    })
}

/// What [`create`] made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Objects created.
    pub made: usize,
    /// Commands whose shipped definition replaced an older one.
    pub commands_replaced: usize,
    /// Commands that did not install: name and why.
    pub failed_commands: Vec<(String, String)>,
    /// Pieces that failed without stopping the rest.
    pub problems: Vec<String>,
}

/// Every operating system object missing from the store: message
/// files, the work management objects, display files, panel groups
/// and menus, the registration facility, the commands and `QSTRUP`.
/// Objects that exist are left alone (a changed command definition is
/// replaced, as at every IPL).
pub fn create(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<Report, Error> {
    let mut r = Report::default();
    let mctx = msgf::store::Ctx {
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    };
    r.made += piece(
        "message files",
        msgf::store::install_builtin(txn, &mctx, qsys),
    )?
    .len();
    r.made += piece(
        "work management objects",
        wm::obj::shipped::install(txn, qsys, stamp),
    )?;
    r.made += piece("display files", display::install_shipped(txn, qsys, stamp))?;
    r.made += piece(
        "panel groups and menus",
        ui::install_shipped(txn, qsys, stamp),
    )?;
    r.made += piece(
        "registration facility",
        regfac::store::install(txn, qsys, stamp),
    )?;
    let cctx = cmd::store::Ctx {
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    };
    let done = piece("commands", sys_cpps::install::install(txn, &cctx, qsys))?;
    r.made += done.created.len();
    r.commands_replaced = done.replaced.len();
    r.failed_commands = done.failed;
    match qstrup::install(txn, qsys, stamp) {
        Ok(made) => r.made += usize::from(made),
        Err(e) => r.problems.push(format!("QSYS/QSTRUP: {e}")),
    }
    Ok(r)
}

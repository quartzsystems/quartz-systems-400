// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ILE in the storage service: the commands that compile, bind and
//! display, and the sources a job's programs read objects, files, data
//! areas and messages through. A request lends the store, the
//! temporary ASP and the system values to the sources and takes them
//! back when it is answered. Programs run in the job processes
//! (`qsjob`), not here.

pub mod commands;
mod provider;
mod sources;

use std::cell::RefCell;
use std::rc::Rc;

use qsname::Name;
use store::obj::ObjectId;
use store::obj::ops::{self, NameContext};
use store::txn::Txn;
use sysval::SysVals;

use crate::ipl::storage::{Storage, Temp};
use crate::ipl::system::System;

pub use provider::StoreProvider;
pub use sources::{MainStore, StoreData, StoreMsgs, StoreObjects, StoreUserSpaces};

/// What a request lends the sources.
pub struct Lent {
    /// ASP 1 and its store.
    pub storage: Storage,
    /// The temporary ASP.
    pub temp: Option<Temp>,
    /// The system values.
    pub sysvals: Option<SysVals>,
}

/// The lent resources, shared with the sources.
pub type Shared = Rc<RefCell<Option<Lent>>>;

/// The help lines.
pub const HELP: &str = "\
qir asm LIB/SRCPF MBR MODULE(LIB/M) | qir asm NAME MODULE(LIB/M)   assemble a member or an xfer buffer
qir check NAME | qir dis LIB/MODULE                                verify a buffer; print a module's IR
crtpgm LIB/PGM MODULE(LIB/M ...) [BNDDIR(LIB/B ...) ACTGRP(*NEW|*CALLER|NAME) ENTMOD(M) OPTION(*UNRSLVREF) TEXT('..')]
crtsrvpgm LIB/S MODULE(...) [EXPORT(*ALL|*SRCFILE) SRCFILE(LIB/F) SRCMBR(M) BNDDIR(...) ACTGRP(...) TEXT('..')]
updpgm LIB/PGM MODULE(LIB/M ...) | crtbnddir LIB/B | addbnddire LIB/B OBJ((LIB/N *MODULE|*SRVPGM) ...)
call LIB/PGM ['text' 123 x'C1C2' ...]      run a program; parameters as CL types them
dsppgm | dspsrvpgm | dspmod | dsppgmref | dspbnddir LIB/NAME
dspactgrp | rclactgrp NAME|*ELIGIBLE | dspcallstk | dspmsgq [*PGM|*EXT|QSYSOPR] | rpy KEY REPLY | dmp
wrkmsgd LIB/MSGF | dspmsgd ID [MSGF(LIB/F)]
strdbg LIB/PGM | addbkp N [PGM(LIB/PGM)] | rmvbkp N|*ALL | dspbkp | enddbg
   at a stop: step [over|into], go, eval VAR[:x|:c], dspvar, dspcallstk, addbkp N, rmvbkp N, dmp
";

pub use wm::ctl::JOB_COMMANDS;

/// Run `f` in a transaction on the lent main store.
pub fn with_main<T>(
    shared: &Shared,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let mut b = shared.borrow_mut();
    let lent = b
        .as_mut()
        .ok_or("the object store is not available to the job")?;
    let qsys = lent.storage.qsys.ok_or("object store not initialized")?;
    let mut txn = lent.storage.store.begin().map_err(|e| e.to_string())?;
    match f(&mut txn, qsys) {
        Ok(v) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

/// Run `f` in a transaction on the lent temporary store.
pub fn with_temp<T>(
    shared: &Shared,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let mut b = shared.borrow_mut();
    let lent = b
        .as_mut()
        .ok_or("the object store is not available to the job")?;
    let temp = lent.temp.as_mut().ok_or("QTEMP is not available")?;
    let qtemp = temp.qtemp;
    let mut txn = temp.store.begin().map_err(|e| e.to_string())?;
    match f(&mut txn, qtemp) {
        Ok(v) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

thread_local! {
    /// The library list of the job whose request is being answered.
    /// The service answers one request at a time, so one slot does.
    static LIBL: RefCell<Option<wm::job::LibList>> = const { RefCell::new(None) };
}

/// Name resolution follows `libl` until the next call; `None` goes back
/// to `QSYS` then `QGPL`.
pub fn set_libl(libl: Option<wm::job::LibList>) {
    LIBL.with(|c| *c.borrow_mut() = libl);
}

/// The library list name resolution uses now, if a job's is set.
pub fn current_libl() -> Option<wm::job::LibList> {
    LIBL.with(|c| c.borrow().clone())
}

/// The library list as the store resolves names: the requesting job's
/// list in search order (`QTEMP` and libraries that do not exist left
/// out), its current library or, for `*CRTDFT`, `QGPL`. Without a job,
/// `QSYS` then `QGPL`.
pub fn name_context(txn: &mut Txn<'_>, qsys: ObjectId) -> NameContext {
    let mut find = |n: &str| -> Option<ObjectId> {
        match n {
            "QSYS" => Some(qsys),
            "QTEMP" => None,
            _ => ops::find_library(txn, qsys, Name::parse(n).ok()?)
                .map(|f| f.id)
                .ok(),
        }
    };
    let qgpl = find("QGPL");
    match current_libl() {
        Some(l) => NameContext {
            curlib: l.cur.as_deref().and_then(&mut find).or(qgpl),
            libl: l.search().iter().filter_map(|n| find(n)).collect(),
        },
        None => NameContext {
            curlib: qgpl,
            libl: std::iter::once(qsys).chain(qgpl).collect(),
        },
    }
}

/// Lend the store, the temporary ASP and the system values to the
/// sources.
pub fn lend(system: &mut System) -> Result<(), String> {
    let storage = system.storage.take().ok_or("object store not open")?;
    *system.lent.borrow_mut() = Some(Lent {
        storage,
        temp: system.temp.take(),
        sysvals: system.sysvals.take(),
    });
    Ok(())
}

/// Take them back.
pub fn restore(system: &mut System) {
    if let Some(l) = system.lent.borrow_mut().take() {
        system.storage = Some(l.storage);
        system.temp = l.temp;
        system.sysvals = l.sysvals;
    }
}

/// A console command of this module, or none.
pub fn command(system: &mut System, cmd: &str, args: &[&str], line: &str) -> Option<String> {
    commands::command(system, cmd, args, line)
}

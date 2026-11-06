// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The time zone descriptions the service answers with: every `*TIMZON`
//! in QSYS, installed into `sysval::tz` at start and after each change
//! or restore, so `QTIMZON`, `QUTCOFFSET` and local time follow at once.
//! The first start of V1R1M0 on a V1R0M0 store also moves `QTIMZON`
//! from its V1R0M0 name to IBM's (`sysval::tz::v1r0m0_name`).

use std::sync::atomic::{AtomicBool, Ordering};

use store::obj::library::Library;
use store::obj::types;

use super::spool::with_store;
use crate::ipl::system::System;

/// Set before the shipped objects are made: QSYS had commands but no
/// descriptions, a store V1R0M0 made.
static FROM_V1R0M0: AtomicBool = AtomicBool::new(false);

/// How many objects of `ty` QSYS holds.
fn count(system: &mut System, ty: u16) -> usize {
    with_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        Ok(lib.list(txn, Some(ty)).map_err(|e| e.to_string())?.len())
    })
    .unwrap_or(0)
}

/// Before the shipped objects are made: note a V1R0M0 store.
pub fn before_install(system: &mut System) {
    let old = count(system, types::TIMZON) == 0 && count(system, types::CMD) > 0;
    FROM_V1R0M0.store(old, Ordering::Relaxed);
}

/// Once the system values are here: SQL's local time hooked to
/// `QTIMZON`, `QTIMZON` moved from its V1R0M0
/// name on such a store, then the descriptions installed.
pub fn after_install(system: &mut System) {
    sql::exec::registers::set_local_offset(sysval::tz::current_offset);
    if FROM_V1R0M0.swap(false, Ordering::Relaxed) {
        migrate(system);
    }
    reload(system);
}

fn migrate(system: &mut System) {
    let old = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QTIMZON").ok())
        .map(|v| v.text().trim().to_string())
        .unwrap_or_default();
    let Some(new) = sysval::tz::v1r0m0_name(&old).filter(|n| *n != old) else {
        return;
    };
    // The shipped set is installed before the change is checked.
    sysval::tz::install(sysval::tz::shipped().to_vec());
    let set = system.sysvals.as_mut().is_some_and(|v| v.set_text("QTIMZON", new).is_ok());
    if set {
        match crate::ipl::sysvals::save(system) {
            Ok(()) => eprintln!("qsstg: QTIMZON {old} (V1R0M0) is now {new}"),
            Err(e) => eprintln!("qsstg: QTIMZON {old} -> {new} not saved: {e}"),
        }
    }
}

/// Install every description in QSYS; the shipped set when the store
/// cannot be read.
pub fn reload(system: &mut System) {
    match with_store(system, osobjects::timzon::load_all) {
        Ok(zones) if !zones.is_empty() => sysval::tz::install(zones),
        Ok(_) => sysval::tz::install(sysval::tz::shipped().to_vec()),
        Err(e) => eprintln!("qsstg: time zone descriptions: {e}"),
    }
}

/// Whether a command can have changed the descriptions in QSYS.
pub fn touches(cmd: &str) -> bool {
    matches!(cmd, "RSTOBJ" | "RSTLIB" | "RST" | "RSTLICPGM" | "RCLSTG" | "APYPTF" | "RMVPTF" | "DLTOBJ" | "RNMOBJ")
}

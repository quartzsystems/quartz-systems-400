// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel groups and menus in the store: `*PNLGRP` objects hold a
//! compiled UIM payload, `*MENU` objects their attributes. The shipped
//! panel groups are made by `osobjects`; a job opens them through the
//! storage service (`pnlgrp_open`, `menu_open`).

pub use osobjects::ui::{compile, create_menu, create_pnlgrp, menu_type, pnlgrp_type};
use store::obj::ObjectId;
use store::txn::Txn;

use crate::ipl::system::System;

fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
    type_code: u16,
) -> Result<(ObjectId, String), String> {
    let names = crate::svc::ile::name_context(txn, qsys);
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    let id = wm::obj::find(txn, qsys, &names, lib, name, type_code)
        .map_err(|_| format!("CPF9801 object {name} in library {lib} not found"))?;
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, real))
}

/// A panel group's payload and real library.
pub fn open_pnlgrp(system: &mut System, name: &str, lib: &str) -> Result<(Vec<u8>, String), String> {
    let ty = pnlgrp_type();
    crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (id, real) = find(txn, qsys, lib, name, ty)?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Ok((b, real))
    })
}

/// A menu's attributes (encoded) and real library.
pub fn open_menu(system: &mut System, name: &str, lib: &str) -> Result<(Vec<u8>, String), String> {
    let ty = menu_type();
    crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (id, real) = find(txn, qsys, lib, name, ty)?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Ok((b, real))
    })
}


// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel groups and menus in the store: `*PNLGRP` objects hold a
//! compiled UIM payload, `*MENU` objects their attributes. The shipped
//! panel groups (the command menus and the task menus) are compiled
//! and created in `QSYS` when missing; a job opens them through the
//! storage service (`pnlgrp_open`, `menu_open`).

use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::System;

/// The object type code of `*PNLGRP`.
pub fn pnlgrp_type() -> u16 {
    types::type_by_name("*PNLGRP").map_or(0x0012, |t| t.code)
}

/// The object type code of `*MENU`.
pub fn menu_type() -> u16 {
    types::type_by_name("*MENU").map_or(0x0011, |t| t.code)
}

/// Compile UIM text into a panel group payload, or the listing.
pub fn compile(text: &str, name: &str, lib: &str) -> Result<(Vec<u8>, String), String> {
    let opts = uim::CompileOpts {
        name: Some(name.to_string()),
        library: Some(lib.to_string()),
        ..uim::CompileOpts::default()
    };
    let c = uim::compile(text, &uim::NoImbeds, &uim::NoImports, &opts);
    match c.group {
        Some(group) if c.ok() => {
            let p = uim::Payload {
                name: name.to_string(),
                library: lib.to_string(),
                group,
                source: c.source.lines().to_vec(),
            };
            Ok((uim::payload::encode(&p), c.listing))
        }
        _ => Err(c.listing),
    }
}

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

/// Create (or replace) a panel group object with `payload`.
pub fn create_pnlgrp(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    text: &str,
    stamp: &wm::obj::Stamp,
    payload: &[u8],
    replace: bool,
) -> Result<ObjectId, String> {
    replace_existing(txn, lib, name, pnlgrp_type(), replace)?;
    wm::obj::create_attr(txn, lib, (name, pnlgrp_type(), "UIM"), text, stamp, payload)
        .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9801")))
}

/// Create (or replace) a menu object.
pub fn create_menu(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    attrs: &menus::MenuAttrs,
    stamp: &wm::obj::Stamp,
    replace: bool,
) -> Result<ObjectId, String> {
    replace_existing(txn, lib, name, menu_type(), replace)?;
    wm::obj::create_attr(
        txn,
        lib,
        (name, menu_type(), attrs.kind.text().trim_start_matches('*')),
        &attrs.text,
        stamp,
        &attrs.encode(),
    )
    .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9801")))
}

/// Delete an object of the type in `lib` when it exists; refuse when
/// it exists and `replace` is off.
fn replace_existing(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    name: Name,
    type_code: u16,
    replace: bool,
) -> Result<(), String> {
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    if let Some(id) = l.lookup(txn, type_code, name).map_err(|e| e.to_string())? {
        if !replace {
            return Err(format!("CPF2112 object {name} already exists"));
        }
        ops::delete(txn, store::obj::JobHandle(0), id).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The shipped panel groups and menus missing from `QSYS`, compiled
/// from their UIM and created (a `*MENU` of the same name over each);
/// how many objects were made.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    // The system panel groups behind the displays.
    for (name, text) in uim::shipped::FILES {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        if lib
            .lookup(txn, pnlgrp_type(), n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let (payload, _) = compile(text, name, "QSYS")
            .map_err(|l| format!("shipped panel group {name} does not compile:\n{l}"))?;
        create_pnlgrp(txn, qsys, n, &format!("Shipped panel group {name}"), stamp, &payload, false)?;
        made += 1;
    }
    // The menus: a panel group and a *MENU each.
    for (name, text) in menus::shipped::FILES {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        let have_group = lib
            .lookup(txn, pnlgrp_type(), n)
            .map_err(|e| e.to_string())?
            .is_some();
        if !have_group {
            let (payload, _) = compile(text, name, "QSYS").map_err(|l| {
                format!("shipped panel group {name} does not compile:\n{l}")
            })?;
            create_pnlgrp(txn, qsys, n, &format!("Shipped menu {name}"), stamp, &payload, false)?;
            made += 1;
        }
        let have_menu = lib
            .lookup(txn, menu_type(), n)
            .map_err(|e| e.to_string())?
            .is_some();
        if !have_menu {
            let attrs =
                menus::MenuAttrs::uim(menus::Qualified::qsys(name), &format!("Shipped menu {name}"));
            create_menu(txn, qsys, n, &attrs, stamp, false)?;
            made += 1;
        }
    }
    Ok(made)
}

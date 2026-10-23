// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DFU's storage requests (360-379): a file's key fields, and DFU
//! programs, `*PGM` objects of attribute `DFU` whose payload is the
//! program's definition (`dfu::Program`). RLU reads and writes its
//! member through PDM's requests.

use store::obj::{JobHandle, ops, types};

use crate::ipl::system::System;
use crate::svc::db::{db, db_err, file_id, now, object_name, typed_id, with_txn};

/// The extended attribute of a DFU program.
const ATTR: &str = "DFU";

/// `LIB/NAME`, or the name alone for the library list.
fn qual(lib: &str, name: &str) -> String {
    if lib.is_empty() || lib.eq_ignore_ascii_case("*LIBL") {
        name.to_string()
    } else {
        format!("{lib}/{name}")
    }
}

/// Each record format's key fields, in key order.
pub fn keys(system: &mut System, lib: &str, file: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual(lib, file))?;
        let d = db::file::describe(txn, id).map_err(|e| db_err(&e))?;
        Ok(d.formats
            .iter()
            .map(|f| {
                (
                    f.format.name.to_string(),
                    f.format.keys.iter().map(|k| k.name.to_string()).collect(),
                )
            })
            .collect())
    })
}

/// A DFU program's definition and its library; `None` when there is no
/// such program.
///
/// # Errors
/// A program that is not a DFU program.
pub fn get(system: &mut System, lib: &str, name: &str) -> Result<Option<(String, String)>, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let Ok(id) = typed_id(txn, ctx, &qual(lib, name), types::PGM) else {
            return Ok(None);
        };
        let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != ATTR {
            return Err(format!("CPF9898 Program {name} is not a DFU program."));
        }
        let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
        let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        Ok(Some((real, String::from_utf8_lossy(&b).into_owned())))
    })
}

/// Create a DFU program, replacing a DFU program of the name.
pub fn put(
    system: &mut System,
    lib: &str,
    name: &str,
    def: &str,
    text: &str,
) -> Result<String, String> {
    let oname = object_name(&qual(lib, name))?;
    let owner = sec::guard::current()
        .map_or_else(|| "QSYS".to_string(), |s| s.borrow().principal.user.clone());
    let stamp = wm::obj::Stamp {
        owner,
        system: system.system_name(),
        now: now(),
    };
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let l = ctx.library_for(txn, &oname).map_err(|e| e.to_string())?;
        let n = qsname::Name::parse(name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        let lb = store::obj::library::Library::load(txn, l).map_err(|e| e.to_string())?;
        if let Some(old) = lb.lookup(txn, types::PGM, n).map_err(|e| e.to_string())? {
            let h = ops::describe(txn, old).map_err(|e| e.to_string())?;
            if ccsid::decode_trimmed(&h.attribute) != ATTR {
                return Err(format!(
                    "CPF9898 Program {name} exists and is not a DFU program."
                ));
            }
            ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
        }
        let id = wm::obj::create_attr(txn, l, (n, types::PGM, ATTR), text, &stamp, def.as_bytes())
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF9898")))?;
        ile::store::library_name(txn, id).map_err(|e| e.to_string())
    })
}

/// Delete a DFU program.
pub fn delete(system: &mut System, lib: &str, name: &str) -> Result<(), String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = typed_id(txn, ctx, &qual(lib, name), types::PGM)
            .map_err(|_| format!("CPF2105 Program {name} in {lib} not found."))?;
        let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != ATTR {
            return Err(format!("CPF9898 Program {name} is not a DFU program."));
        }
        ops::delete(txn, JobHandle(0), id).map_err(|e| e.to_string())
    })
}

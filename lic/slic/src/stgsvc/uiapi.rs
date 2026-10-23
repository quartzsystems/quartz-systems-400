// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Search indexes for UIM help: a `*SCHIDX` holds topics (title, root
//! words, panel group and help module) that `ADDSCHIDXE` copies from a
//! panel group's `:ISCH.` tags; F11 in help reads them back.

use qsname::Name;
use stg::proto::SchidxEntry;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};

/// The object type code of `*SCHIDX`.
fn schidx_type() -> u16 {
    types::type_by_name("*SCHIDX").map_or(0x0023, |t| t.code)
}

/// The object type code of `*PNLGRP`.
fn pnlgrp_type() -> u16 {
    crate::svc::ui::pnlgrp_type()
}

/// Run `f` in a transaction with the job's names.
fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &ops::NameContext) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let names = crate::svc::ile::name_context(txn, qsys);
        f(txn, qsys, &names)
    })
}

/// An object by `lib` (`*LIBL` when blank) and name.
fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    (name, lib): (&str, &str),
    ty: u16,
) -> Result<(ObjectId, String), String> {
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    let id = wm::obj::find(txn, qsys, names, lib, name, ty)
        .map_err(|_| format!("CPF9801 {name} in {lib}"))?;
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, real))
}

/// The entries of a search index.
fn entries(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<SchidxEntry>, String> {
    let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    if b.is_empty() {
        return Ok(Vec::new());
    }
    ipc::decode(&b).map_err(|e| format!("CPF9898 search index damaged: {e}"))
}

/// A search index's topics.
pub fn schidx_entries(
    system: &mut System,
    name: &str,
    lib: &str,
) -> Result<Vec<SchidxEntry>, String> {
    in_store(system, |txn, qsys, names| {
        let (id, _) = find(txn, qsys, names, (name, lib), schidx_type())?;
        entries(txn, id)
    })
}

/// `CRTSCHIDX`: an empty search index.
pub fn schidx_create(
    system: &mut System,
    (name, lib): (&str, &str),
    text: &str,
    replace: bool,
    owner: &str,
) -> Result<String, String> {
    let sysname = system.system_name();
    in_store(system, |txn, qsys, names| {
        let l = match lib {
            "*CURLIB" | "" => names
                .curlib
                .ok_or_else(|| format!("CPF6E11 {name}: no current library"))?,
            l => wm::obj::library(txn, qsys, l).map_err(|e| format!("CPF6E11 {e}"))?,
        };
        let n = Name::parse(name).map_err(|e| format!("CPD0078 SCHIDX({name}): {e}"))?;
        let lib_obj = store::obj::library::Library::load(txn, l).map_err(|e| e.to_string())?;
        if let Some(old) = lib_obj
            .lookup(txn, schidx_type(), n)
            .map_err(|e| e.to_string())?
        {
            if !replace {
                return Err(format!("CPF6E11 search index {name} exists"));
            }
            ops::delete(txn, store::obj::JobHandle(0), old).map_err(|e| e.to_string())?;
        }
        let stamp = wm::obj::Stamp {
            owner: if owner.is_empty() {
                "QSYS".into()
            } else {
                owner.to_string()
            },
            system: sysname,
            now: now_unix(),
        };
        let empty: Vec<SchidxEntry> = Vec::new();
        wm::obj::create_attr(
            txn,
            l,
            (n, schidx_type(), ""),
            text,
            &stamp,
            &ipc::encode(&empty),
        )
        .map_err(|e| format!("CPF6E11 {e}"))?;
        Ok(format!("Search index {name} created.\n"))
    })
}

/// `ADDSCHIDXE`: a panel group's topics into a search index.
pub fn schidx_add(
    system: &mut System,
    (name, lib): (&str, &str),
    (pnlgrp, pnlgrp_lib): (&str, &str),
) -> Result<String, String> {
    in_store(system, |txn, qsys, names| {
        let (id, _) = find(txn, qsys, names, (name, lib), schidx_type())
            .map_err(|e| format!("CPF6E12 {e}"))?;
        let (gid, real) = find(txn, qsys, names, (pnlgrp, pnlgrp_lib), pnlgrp_type())
            .map_err(|e| format!("CPF6E12 {e}"))?;
        let payload = wm::obj::read(txn, gid).map_err(|e| e.to_string())?;
        let group = uim::payload::decode(&payload)
            .map_err(|e| format!("CPF6E12 panel group {pnlgrp}: {e}"))?
            .group;
        let qualified = format!("{real}/{}", pnlgrp.to_ascii_uppercase());
        let mut all = entries(txn, id)?;
        if all.iter().any(|e| e.2 == qualified) {
            return Err(format!("CPF6E62 {pnlgrp} is in search index {name}"));
        }
        let added: Vec<SchidxEntry> = uim::engine::IndexEntry::of_group(&group, &qualified)
            .into_iter()
            .map(|e| (e.title, e.roots, e.pnlgrp, e.module))
            .collect();
        let n = added.len();
        all.extend(added);
        wm::obj::write(txn, id, &ipc::encode(&all)).map_err(|e| e.to_string())?;
        Ok(format!(
            "{n} topics of panel group {pnlgrp} added to search index {name}.\n"
        ))
    })
}

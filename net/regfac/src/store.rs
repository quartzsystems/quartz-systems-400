// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility repository in the object store:
//! `QUSRSYS/QUSEXRGOBJ`, type `*EXITRG`, its payload [`crate::payload`].

use qsname::Name;
use store::obj::ObjectId;
use store::obj::library::Library;
use store::txn::Txn;

use crate::error::{RegFail, RegResult};
use crate::payload;
use crate::registry::Registry;

/// The repository's library.
pub const LIB: &str = "QUSRSYS";
/// The repository's name.
pub const NAME: &str = "QUSEXRGOBJ";
/// Its text.
pub const TEXT: &str = "Registration facility exit points";

/// The `*EXITRG` type code.
#[must_use]
pub fn type_code() -> u16 {
    store::obj::types::EXITRG
}

fn fail(e: impl std::fmt::Display) -> RegFail {
    RegFail::unavailable(&format!("registration facility repository: {e}"))
}

/// The repository object, if it exists.
pub fn find(txn: &mut Txn<'_>, qsys: ObjectId) -> RegResult<Option<ObjectId>> {
    let Ok(lib) = wm::obj::library(txn, qsys, LIB) else {
        return Ok(None);
    };
    let l = Library::load(txn, lib).map_err(fail)?;
    l.lookup(txn, type_code(), Name::parse(NAME).expect("valid"))
        .map_err(fail)
}

/// The registry the store holds; `CPF3CDA` when there is none.
pub fn load(txn: &mut Txn<'_>, qsys: ObjectId) -> RegResult<Registry> {
    let id = find(txn, qsys)?
        .ok_or_else(|| RegFail::unavailable("there is no registration facility repository"))?;
    let b = wm::obj::read(txn, id).map_err(fail)?;
    payload::decode(&b)
}

/// Write `reg` back.
pub fn save(txn: &mut Txn<'_>, qsys: ObjectId, reg: &Registry) -> RegResult<()> {
    let id = find(txn, qsys)?
        .ok_or_else(|| RegFail::unavailable("there is no registration facility repository"))?;
    wm::obj::write(txn, id, &payload::encode(reg)).map_err(fail)
}

/// Make the repository with the shipped points when it is missing, or
/// add the shipped points it lacks; how many points were added.
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> RegResult<usize> {
    if find(txn, qsys)?.is_some() {
        let mut reg = load(txn, qsys)?;
        let added = reg.merge_shipped();
        if added > 0 {
            save(txn, qsys, &reg)?;
        }
        return Ok(added);
    }
    let lib = wm::obj::library(txn, qsys, LIB).map_err(fail)?;
    let reg = Registry::shipped();
    wm::obj::create(
        txn,
        lib,
        Name::parse(NAME).expect("valid"),
        type_code(),
        TEXT,
        stamp,
        &payload::encode(&reg),
    )
    .map_err(fail)?;
    Ok(reg.points.len())
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped time zone descriptions (`sysval::tz::shipped`): IBM's
//! `QSYS/Q...` `*TIMZON` objects, their rules from tzdata.

use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, types};
use store::txn::Txn;
use sysval::tz::Zone;

/// The shipped descriptions missing from `QSYS`; how many were made.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    for z in sysval::tz::shipped() {
        let n = Name::parse(&z.name).map_err(|e| format!("{}: {e}", z.name))?;
        if lib
            .lookup(txn, types::TIMZON, n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        wm::obj::create(
            txn,
            qsys,
            n,
            types::TIMZON,
            &z.text,
            stamp,
            &wm::obj::timzon::encode(z),
        )
        .map_err(|e| format!("{}: {e}", z.name))?;
        made += 1;
    }
    Ok(made)
}

/// Every `*TIMZON` in `QSYS`, in `WRKTIMZON` order; one that does not
/// read is left out.
pub fn load_all(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<Zone>, String> {
    let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (k, id) in lib
        .list(txn, Some(types::TIMZON))
        .map_err(|e| e.to_string())?
    {
        let name = k.name.to_string();
        if let Ok(z) = wm::obj::read(txn, id)
            .map_err(|e| e.to_string())
            .and_then(|b| wm::obj::timzon::decode(&name, &b).map_err(|e| e.to_string()))
        {
            out.push(z);
        }
    }
    out.sort_by(sysval::tz::list_order);
    Ok(out)
}

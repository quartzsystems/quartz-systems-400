// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped workstation customizing objects (`print::wscst::shipped`):
//! IBM's `QSYS/QWP...` names with Quartz content.

use qsname::Name;
use store::obj::{ObjectId, types};
use store::txn::Txn;

/// The shipped `*WSCST` objects missing from `QSYS`; how many were made.
pub fn install_shipped(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, String> {
    let lib = store::obj::library::Library::load(txn, qsys).map_err(|e| e.to_string())?;
    let mut made = 0;
    for (name, class, payload) in print::wscst::shipped::objects() {
        let n = Name::parse(name).map_err(|e| format!("{name}: {e}"))?;
        if lib
            .lookup(txn, types::WSCST, n)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        wm::obj::create_attr(txn, qsys, (n, types::WSCST, class), "", stamp, &payload)
            .map_err(|e| format!("{name}: {e}"))?;
        made += 1;
    }
    Ok(made)
}

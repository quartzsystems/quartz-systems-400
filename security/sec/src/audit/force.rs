// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAUDENDACN(*PWRDWNSYS)`: the data area `QSYS/QAUDFORCE` holds `1`
//! from the power down an unwritable audit journal caused until the
//! next IPL has set `QAUDCTL(*NONE)`.

use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::ops::CreateParams;
use store::obj::{ObjectId, types};
use store::txn::Txn;

use crate::Result;

/// The data area.
pub const AREA: &str = "QAUDFORCE";

fn find(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Option<ObjectId>> {
    let lib = store::obj::library::Library::load(txn, qsys)?;
    Ok(lib.lookup(txn, types::DTAARA, Name::parse(AREA).expect("valid"))?)
}

/// Mark the next IPL.
pub fn set(txn: &mut Txn<'_>, qsys: ObjectId, now: u64) -> Result<()> {
    let one = DataAreaValue::Char("1".into());
    match find(txn, qsys)? {
        Some(id) => dtaara::write(txn, id, &one)?,
        None => {
            dtaara::create(
                txn,
                &CreateParams {
                    library: qsys,
                    name: Name::parse(AREA).expect("valid"),
                    type_code: types::DTAARA,
                    attribute: String::new(),
                    text: "Audit journal full: QAUDCTL off at the next IPL".into(),
                    owner: "QSYS".into(),
                    system: "QS400".into(),
                    now,
                    by_system: true,
                },
                DataAreaType::Char { len: 1 },
                &one,
            )?;
        }
    }
    Ok(())
}

/// Whether the mark is set; it is cleared.
pub fn take(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<bool> {
    let Some(id) = find(txn, qsys)? else {
        return Ok(false);
    };
    let on = matches!(dtaara::read(txn, id)?, DataAreaValue::Char(s) if s.trim() == "1");
    if on {
        dtaara::write(txn, id, &DataAreaValue::Char("0".into()))?;
    }
    Ok(on)
}

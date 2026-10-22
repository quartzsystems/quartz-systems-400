// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSECURITY` changes take effect at an IPL. The level the last IPL
//! ran at is kept in the data area `QSYS/QSECLVL`; when the new level is
//! 30 or above and the old one 20 or below, every profile whose class
//! is not `*SECOFR` loses `*ALLOBJ`, as IBM does.

use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::ops::CreateParams;
use store::obj::{ObjectId, types};
use store::txn::Txn;

use crate::Result;
use crate::profile::objects;
use crate::profile::spcaut::{SpecialAuth, UserClass};

/// The data area that remembers the level.
pub const AREA: &str = "QSECLVL";

/// What the IPL found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    /// The level the last IPL ran at, 0 the first time.
    pub from: u8,
    /// The level now.
    pub to: u8,
    /// Profiles that lost `*ALLOBJ`.
    pub revoked: usize,
}

/// Note this IPL's level; adjust the profiles when it rose past 20.
pub fn ipl(txn: &mut Txn<'_>, qsys: ObjectId, level: u8, now: u64) -> Result<Change> {
    let name = Name::parse(AREA).expect("valid");
    let lib = store::obj::library::Library::load(txn, qsys)?;
    let found = lib.lookup(txn, types::DTAARA, name)?;
    let value = DataAreaValue::Char(format!("{level:02}"));
    let from = if let Some(id) = found {
        let old = match dtaara::read(txn, id)? {
            DataAreaValue::Char(s) => s.trim().parse().unwrap_or(0),
            _ => 0,
        };
        dtaara::write(txn, id, &value)?;
        old
    } else {
        dtaara::create(
            txn,
            &CreateParams {
                library: qsys,
                name,
                type_code: types::DTAARA,
                attribute: String::new(),
                text: "Security level of the last IPL".into(),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now,
                by_system: true,
            },
            DataAreaType::Char { len: 2 },
            &value,
        )?;
        0
    };
    let mut revoked = 0;
    if from != 0 && from <= 20 && level >= 30 {
        for n in objects::list(txn, qsys)? {
            let (id, mut p) = objects::read(txn, qsys, &n)?;
            if p.usrcls != UserClass::SecOfr && p.spcaut.has(SpecialAuth::ALLOBJ) {
                p.spcaut = p.spcaut.without(SpecialAuth::ALLOBJ);
                objects::write(txn, id, &p)?;
                revoked += 1;
            }
        }
        crate::auth::cache::bump_epoch();
    }
    Ok(Change {
        from,
        to: level,
        revoked,
    })
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects of the shipped products other than `QS00SS1`: what
//! each one's code load lists. The service makes them when it seeds the
//! catalog; `build lp` makes them for the product's load.

use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types, usridx};
use store::txn::Txn;

use crate::{Error, piece};

/// Products with objects of their own, and their libraries.
pub const PRODUCTS: &[(&str, &[&str])] =
    &[("QS00DG1", &["QHTTPSVR"]), ("QS00RPG", &["QRPG"]), ("QS00AFW", &["QUSRSYS"])];

/// Access for Web's stores in `QUSRSYS` (`net/accessweb`, docs/ACCESSWEB.md):
/// name, text. The system owns them; the product changes them through
/// the storage service (`web_store`).
pub const AFW_STORES: &[(&str, &str)] = &[
    ("QZXWPOL", "Access for Web policies"),
    ("QZXWPRF", "Access for Web preferences"),
];

/// The stores' key: a profile name (10) and a function or preference
/// key (20).
pub const AFW_KEY: u16 = 30;

/// The stores' longest entry: the key and its value.
pub const AFW_ENTRY: u16 = 128;

/// The libraries a product's objects live in (none for a product
/// without objects of its own).
#[must_use]
pub fn libraries(product: &str) -> &'static [&'static str] {
    PRODUCTS
        .iter()
        .find(|(p, _)| *p == product)
        .map_or(&[], |(_, l)| l)
}

/// Access for Web's stores missing from `QUSRSYS`; how many were made.
fn afw_stores(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<usize, Error> {
    let what = "Access for Web stores";
    let lib = piece(what, ops::find_library(txn, qsys, Name::parse("QUSRSYS").expect("valid")))?.id;
    let have = piece(what, ops::list(txn, lib, Some(types::USRIDX)))?;
    let mut made = 0;
    for (name, text) in AFW_STORES {
        let name = Name::parse(name).expect("valid");
        if have.iter().any(|(k, _)| k.name == name) {
            continue;
        }
        let params = CreateParams {
            library: lib,
            name,
            type_code: types::USRIDX,
            attribute: String::new(),
            text: (*text).into(),
            owner: stamp.owner.clone(),
            system: stamp.system.clone(),
            now: stamp.now,
            by_system: true,
        };
        piece(what, usridx::create(txn, &params, false, AFW_KEY, AFW_ENTRY))?;
        made += 1;
    }
    Ok(made)
}

/// A product's objects missing from the store, with their libraries;
/// how many were made.
pub fn create(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
    product: &str,
) -> Result<usize, Error> {
    let ctx = msgf::store::Ctx {
        now: stamp.now,
        owner: stamp.owner.clone(),
        system: stamp.system.clone(),
    };
    let mut made = 0;
    for lib in libraries(product) {
        made += piece(
            "product message files",
            msgf::store::install_product_files(txn, &ctx, qsys, lib),
        )?
        .len();
    }
    if product == "QS00AFW" {
        made += afw_stores(txn, qsys, stamp)?;
    }
    Ok(made)
}

/// Every product's objects missing from the store.
pub fn create_all(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<usize, Error> {
    let mut made = 0;
    for (p, _) in PRODUCTS {
        made += create(txn, qsys, stamp, p)?;
    }
    Ok(made)
}

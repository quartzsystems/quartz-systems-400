// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects of the shipped products other than `QS00SS1`: what
//! each one's code load lists. The service makes them when it seeds the
//! catalog; `build lp` makes them for the product's load.

use store::obj::ObjectId;
use store::txn::Txn;

use crate::{Error, piece};

/// Products with objects of their own, and their libraries.
pub const PRODUCTS: &[(&str, &[&str])] = &[("QS00DG1", &["QHTTPSVR"]), ("QS00RPG", &["QRPG"])];

/// The libraries a product's objects live in (none for a product
/// without objects of its own).
#[must_use]
pub fn libraries(product: &str) -> &'static [&'static str] {
    PRODUCTS
        .iter()
        .find(|(p, _)| *p == product)
        .map_or(&[], |(_, l)| l)
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

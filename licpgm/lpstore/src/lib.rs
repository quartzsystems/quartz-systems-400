// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Licensed programs over the object store (`docs/LICPGM.md` §9): the
//! `*PRDDFN` and `*PRDLOD` objects, the license and agreement records
//! in `QUSRSYS`, and product save streams for SAVLICPGM and RSTLICPGM.

#![deny(missing_docs)]
#![allow(clippy::similar_names, clippy::single_match_else)]

pub mod objstore;
pub mod savfile;

use std::cell::RefCell;

pub use objstore::{Cell, Products};
pub use savfile::{SaveRestorer, SavfMedia, save_product};

use store::Store;
use store::obj::ObjectId;
use wm::obj::Stamp;

/// The system's stamp for objects the product functions create.
#[must_use]
pub fn stamp(now: u64) -> Stamp {
    Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now,
    }
}

/// Every IPL: the catalog's products created when missing
/// (`licpgm::catalog::seed`). Returns whether anything was made.
pub fn seed(store: &mut Store, qsys: ObjectId, now: u64) -> Result<bool, licpgm::StoreError> {
    let cell = RefCell::new(store);
    let mut p = Products::new(&cell, qsys, stamp(now));
    let before = licpgm::ProductStore::definitions(&p).len();
    licpgm::catalog::seed(&mut p, now)?;
    Ok(licpgm::ProductStore::definitions(&p).len() != before)
}

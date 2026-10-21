// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every on-disk layout of this crate, for the spec test against
//! `docs/DB_FORMAT.md` (run from the `db` crate).

pub use store::spec::{Field, Layout, field};

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::journal::LAYOUT,
        &crate::journal::DIR_LAYOUT,
        &crate::journal::OBJ_LAYOUT,
        &crate::rcv::LAYOUT,
        &crate::entry::LAYOUT,
    ]
}

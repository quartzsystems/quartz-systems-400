// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every fixed layout of this crate, for the spec test against
//! `docs/ILE.md`.

pub use store::spec::{Field, Layout, field};

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::object::module::HDR_LAYOUT,
        &crate::object::module::SECTION_LAYOUT,
        &crate::object::program::HDR_LAYOUT,
        &crate::object::program::SRVPGM_REF_LAYOUT,
        &crate::object::bnddir::HDR_LAYOUT,
        &crate::object::bnddir::ENTRY_LAYOUT,
    ]
}

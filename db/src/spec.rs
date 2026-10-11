// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every on-disk layout of this crate, for the spec test against
//! `docs/DB_FORMAT.md`.

pub use store::spec::{Field, Layout, field};

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::file::attr::LAYOUT,
        &crate::file::members::LAYOUT,
        &crate::file::dependents::LAYOUT,
        &crate::format::sections::SECTION_LAYOUT,
        &crate::format::sections::FORMAT_LAYOUT,
        &crate::format::sections::FIELD_LAYOUT,
        &crate::format::sections::KEY_LAYOUT,
        &crate::format::sections::BASEDON_LAYOUT,
        &crate::format::sections::SOURCE_LAYOUT,
        &crate::format::sections::GROUP_LAYOUT,
        &crate::format::sections::CONDITION_LAYOUT,
        &crate::format::sections::VALUE_LAYOUT,
        &crate::format::sections::JOIN_LAYOUT,
        &crate::member::attr::LAYOUT,
        &crate::member::slots::LAYOUT,
        &crate::path::LAYOUT,
    ]
}

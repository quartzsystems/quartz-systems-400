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
        &crate::payload::HDR_LAYOUT,
        &crate::payload::ENTRY_LAYOUT,
        &crate::payload::VAR_LAYOUT,
    ]
}

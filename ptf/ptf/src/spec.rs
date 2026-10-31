// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PTF layouts as data, for the test against `docs/PTF.md`.

pub use savfmt::spec::{Field, Layout, field};

/// Every layout.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::header::HEADER_LAYOUT,
        &crate::header::REF_LAYOUT,
        &crate::header::EXIT_LAYOUT,
    ]
}

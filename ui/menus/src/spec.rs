// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The payload's layout as data, so `tests/spec.rs` can compare it
//! with the table in `docs/MENUS.md`.

pub use uim::spec::{Field, Layout, field};

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![&crate::payload::LAYOUT]
}

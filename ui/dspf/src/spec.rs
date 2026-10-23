// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every fixed layout of this crate, for the spec test against
//! `docs/DSPF.md`.

pub use db::spec::{Field, Layout, field};

/// The layouts.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![&crate::payload::LAYOUT, &crate::payload::SOURCE_LAYOUT]
}

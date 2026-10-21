// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every fixed layout of this crate, for the spec test against
//! `docs/WORKMGMT.md`.

pub use crate::rec::{Field, Layout};

/// The layouts.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::obj::jobd::LAYOUT,
        &crate::obj::cls::LAYOUT,
        &crate::obj::msgq::LAYOUT,
        &crate::obj::msgq::ENTRY_LAYOUT,
    ]
}

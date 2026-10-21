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
        &crate::obj::jobq::LAYOUT,
        &crate::obj::jobq::ENTRY_LAYOUT,
        &crate::obj::msgq::LAYOUT,
        &crate::obj::msgq::ENTRY_LAYOUT,
        &crate::obj::rpyl::LAYOUT,
        &crate::obj::sbsd::LAYOUT,
        &crate::obj::sbsd::POOL_LAYOUT,
        &crate::obj::sbsd::RTGE_LAYOUT,
        &crate::obj::sbsd::WSE_LAYOUT,
        &crate::obj::sbsd::JOBQE_LAYOUT,
        &crate::obj::sbsd::AJE_LAYOUT,
        &crate::obj::sbsd::PJE_LAYOUT,
        &crate::obj::sbsd::CMNE_LAYOUT,
    ]
}

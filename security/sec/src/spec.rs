// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every fixed layout of this crate, for the spec test against
//! `docs/SECURITY.md`.

pub use crate::rec::{Field, Layout};

/// The layouts.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    let mut v = vec![&crate::profile::payload::LAYOUT];
    v.extend(crate::audit::layout::all());
    v
}

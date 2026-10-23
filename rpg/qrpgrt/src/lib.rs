// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG runtime library: what generated RPG code needs beyond
//! the IR's own instructions, as bound procedures named `_QRNX_...`
//! that the binder resolves as system imports and [`install`] puts into
//! a job (`docs/RPG.md`, the runtime interface).

#![deny(missing_docs)]
#![allow(clippy::too_many_arguments, clippy::many_single_char_names, clippy::trivially_copy_pass_by_ref, clippy::unreadable_literal)]

pub mod fmt;
pub mod mov;
pub mod natives;
pub mod value;

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    vec![
        ("_QRNX_CHARNUM", natives::charnum as *const () as usize),
        ("_QRNX_EDITC", natives::editc as *const () as usize),
        ("_QRNX_EDITW", natives::editw as *const () as usize),
        ("_QRNX_MOVE", natives::move_ as *const () as usize),
        ("_QRNX_POW", natives::pow as *const () as usize),
        ("_QRNX_SQRT", natives::sqrt as *const () as usize),
    ]
}

/// Put the runtime library into a job.
pub fn install(job: &mut mi::Job) {
    for (name, addr) in procedures() {
        job.act.system.insert(name.to_string(), addr);
    }
}

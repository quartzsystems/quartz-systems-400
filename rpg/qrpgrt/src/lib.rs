// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG runtime library: what generated RPG code needs beyond
//! the IR's own instructions, as bound procedures named `_QRNX_...`
//! that the binder resolves as system imports and [`install`] puts into
//! a job (`docs/RPG.md`, the runtime interface).

#![deny(missing_docs)]
#![allow(clippy::too_many_arguments, clippy::many_single_char_names, clippy::trivially_copy_pass_by_ref, clippy::unreadable_literal)]

pub mod cvt;
pub mod exc;
pub mod arrays;
pub mod dates;
pub mod dtnat;
pub mod fmt;
pub mod mov;
pub mod natives;
pub mod ops;
pub mod strops;
pub mod value;

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    vec![
        ("_QRNX_CHARNUM", natives::charnum as *const () as usize),
        ("_QRNX_DTADD", dtnat::add as *const () as usize),
        ("_QRNX_DTCMP", dtnat::cmp as *const () as usize),
        ("_QRNX_DTCVT", dtnat::cvt as *const () as usize),
        ("_QRNX_DTDIFF", dtnat::diff as *const () as usize),
        ("_QRNX_DTNOW", dtnat::now as *const () as usize),
        ("_QRNX_DTSUB", dtnat::sub as *const () as usize),
        ("_QRNX_DTTEST", dtnat::test as *const () as usize),
        ("_QRNX_EDITC", natives::editc as *const () as usize),
        ("_QRNX_EDITW", natives::editw as *const () as usize),
        ("_QRNX_MOVE", natives::move_ as *const () as usize),
        ("_QRNX_POW", natives::pow as *const () as usize),
        ("_QRNX_SQRT", natives::sqrt as *const () as usize),
        ("_QRNX_PSDSINIT", exc::psds_init as *const () as usize),
        ("_QRNX_PSDSERR", exc::psds_err as *const () as usize),
        ("_QRNX_RERAISE", exc::reraise as *const () as usize),
        ("_QRNX_CHARTONUM", cvt::char_to_num as *const () as usize),
        ("_QRNX_MOVEA", ops::movea as *const () as usize),
        ("_QRNX_ZONE", ops::zone as *const () as usize),
        ("_QRNX_CAT", ops::cat as *const () as usize),
        ("_QRNX_SCAN", ops::scan as *const () as usize),
        ("_QRNX_CHECK", ops::check as *const () as usize),
        ("_QRNX_XLATE", ops::xlate as *const () as usize),
        ("_QRNX_SUBST", ops::subst as *const () as usize),
        ("_QRNX_TESTN", ops::testn as *const () as usize),
        ("_QRNX_TESTZ", ops::testz as *const () as usize),
        ("_QRNX_BITS", ops::bits as *const () as usize),
        ("_QRNX_LOOKUP", ops::lookup as *const () as usize),
        ("_QRNX_SORTA", ops::sorta as *const () as usize),
        ("_QRNX_SCANR", ops::scanr as *const () as usize),
        ("_QRNX_SCANRPL", ops::scanrpl as *const () as usize),
        ("_QRNX_SPLIT", ops::split as *const () as usize),
        ("_QRNX_EXTREME", ops::extreme as *const () as usize),
        ("_QRNX_STRGET", ops::strget as *const () as usize),
        ("_QRNX_STRSET", ops::strset as *const () as usize),
    ]
}

/// Put the runtime library into a job.
pub fn install(job: &mut mi::Job) {
    for (name, addr) in procedures() {
        job.act.system.insert(name.to_string(), addr);
    }
}

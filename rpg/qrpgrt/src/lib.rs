// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG runtime library: what generated RPG code needs beyond
//! the IR's own instructions, as bound procedures named `_QRNX_...`
//! that the binder resolves as system imports and [`install`] puts into
//! a job (`docs/RPG.md`, the runtime interface).

#![deny(missing_docs)]
#![allow(clippy::too_many_arguments, clippy::many_single_char_names, clippy::trivially_copy_pass_by_ref, clippy::unreadable_literal)]

pub mod ctl;
pub mod cvt;
pub mod exc;
pub mod arrays;
pub mod dates;
pub mod dtnat;
pub mod fmt;
pub mod mov;
pub mod natives;
pub mod ops;
pub mod sql;
pub mod strops;
pub mod value;
pub mod xml;

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
        ("_QRNX_EDITFLT", natives::editflt as *const () as usize),
        ("_QRNX_MOVE", natives::move_ as *const () as usize),
        ("_QRNX_POW", natives::pow as *const () as usize),
        ("_QRNX_FLTMIX", natives::flt_mix as *const () as usize),
        ("_QRNX_SQRT", natives::sqrt as *const () as usize),
        ("_QRNX_PSDSINIT", exc::psds_init as *const () as usize),
        ("_QRNX_USER", exc::user as *const () as usize),
        ("_QRNX_PSDSERR", exc::psds_err as *const () as usize),
        ("_QRNX_RERAISE", exc::reraise as *const () as usize),
        ("_QRNX_FILEERR", exc::file_err as *const () as usize),
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
        ("_QRNX_SQLIN", sql::sql_in as *const () as usize),
        ("_QRNX_SQLOUT", sql::sql_out as *const () as usize),
        ("_QRNX_SQLRUN", sql::sql_run as *const () as usize),
        ("_QRNX_SQLDA", sql::sql_da as *const () as usize),
        ("_QRNX_COMMIT", ctl::commit as *const () as usize),
        ("_QRNX_DUMP", ctl::dump as *const () as usize),
        ("_QRNX_ALLOC", ctl::alloc as *const () as usize),
        ("_QRNX_NOHANDLER", ctl::no_handler as *const () as usize),
        ("_QRNX_ALTSEQ", ctl::altseq as *const () as usize),
        ("_QRNX_FIXZONED", ctl::fix_zoned as *const () as usize),
        ("_QRNX_SPECIAL", ctl::special as *const () as usize),
        ("_QRNX_SIGNAL", ctl::signal_status as *const () as usize),
        ("_QRNX_DATAPGM", ctl::data_pgm as *const () as usize),
        ("_QRNX_DTALCK", ctl::dta_locked as *const () as usize),
        ("_QRNX_REALLOC", ctl::realloc as *const () as usize),
        ("_QRNX_XMLINTO", xml::xml_into as *const () as usize),
        ("_QRNX_XMLSAX", xml::xml_sax as *const () as usize),
    ]
}

/// Put the runtime library into a job.
pub fn install(job: &mut mi::Job) {
    for (name, addr) in procedures() {
        job.act.system.insert(name.to_string(), addr);
    }
}

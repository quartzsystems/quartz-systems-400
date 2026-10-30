// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Qshell utilities for QS/400, written from scratch: every one is a
//! [`qsh::sys::Utility`] over the [`qsh::sys::Sys`] seam alone, so the
//! same code runs over the in-memory system in tests and over the IFS
//! client on the system.
//!
//! - [`file`]: `cat chmod chgrp chown cp df du file find ln ls mkdir
//!   mkfifo mv pwd rm rmdir touch`.
//! - [`text`]: `cmp cut expand grep head iconv od paste pr sed sort
//!   split tail tee tr uniq wc` (and `egrep`, `fgrep`), `gencat` and
//!   `dspmsg`.
//! - [`proc`]: `basename date dirname env expr id locale logname nohup
//!   printenv ps pwdx sleep test true false uname xargs`.
//! - [`archive`]: `pax tar compress uncompress zcat ajar`.
//! - [`net`]: `hostname nslookup host dig rexec`.
//! - [`ibmi`]: `system db2 liblist sysval datarea dataq catsplf
//!   getjobid attr setccsid Rfile ipcs ipcrm clrtmp logger`.
//! - [`regex`]: the POSIX basic and extended regular expressions behind
//!   `grep` and `sed`.
//! - [`common`]: option parsing, buffered output, time, modes, running
//!   programs and walking trees.
//!
//! Behavior follows the POSIX specification and Qshell's documented
//! utilities; `docs/QSH.md` lists each utility and the formats still to
//! settle on the system.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Utilities share one signature and build text with format!; counts
// and sizes move between integer widths at the output edge.
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::unnecessary_wraps,
    clippy::format_push_string,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::struct_excessive_bools
)]

pub mod archive;
pub mod common;
pub mod file;
pub mod ibmi;
pub mod net;
pub mod proc;
pub mod regex;
pub mod text;

use std::sync::Arc;

use qsh::sys::{Registry, UtilCtx, Utility};

/// One utility: its name and entry point.
pub type Entry = (&'static str, fn(&mut UtilCtx<'_>) -> i32);

/// Every utility this crate ships, by name.
pub const UTILITIES: &[Entry] = &[
    ("ajar", archive::ajar::run),
    ("attr", ibmi::attr::run_attr),
    ("basename", proc::basename::run),
    ("cat", file::cat::run),
    ("catsplf", ibmi::catsplf::run),
    ("chgrp", file::chown::run_chgrp),
    ("chmod", file::chmod::run),
    ("chown", file::chown::run_chown),
    ("clrtmp", ibmi::clrtmp::run),
    ("cmp", text::cmp::run),
    ("compress", archive::compress::run_compress),
    ("cp", file::cp::run),
    ("cut", text::cut::run),
    ("dataq", ibmi::dataq::run),
    ("datarea", ibmi::datarea::run),
    ("date", proc::date::run),
    ("db2", ibmi::db2::run),
    ("df", file::df::run),
    ("dig", net::dig::run),
    ("dirname", proc::basename::run_dirname),
    ("dspmsg", text::catalog::run_dspmsg),
    ("du", file::du::run),
    ("echo", proc::echo::run),
    ("egrep", text::grep::run_egrep),
    ("env", proc::env::run),
    ("expand", text::expand::run),
    ("expr", proc::expr::run),
    ("false", proc::truefalse::run_false),
    ("fgrep", text::grep::run_fgrep),
    ("file", file::filetype::run),
    ("find", file::find::run),
    ("gencat", text::catalog::run_gencat),
    ("getjobid", ibmi::getjobid::run),
    ("grep", text::grep::run),
    ("head", text::head::run),
    ("host", net::host::run),
    ("hostname", net::hostname::run),
    ("iconv", text::iconv::run),
    ("id", proc::id::run_id),
    ("ipcrm", ibmi::ipcs::run_ipcrm),
    ("ipcs", ibmi::ipcs::run_ipcs),
    ("liblist", ibmi::liblist::run),
    ("ln", file::ln::run),
    ("locale", proc::locale::run),
    ("logger", ibmi::logger::run),
    ("logname", proc::id::run_logname),
    ("ls", file::ls::run),
    ("mkdir", file::mkdir::run),
    ("mkfifo", file::mkfifo::run),
    ("mv", file::mv::run),
    ("nohup", proc::nohup::run),
    ("nslookup", net::nslookup::run),
    ("od", text::od::run),
    ("paste", text::paste::run),
    ("pax", archive::pax::run),
    ("pr", text::pr::run),
    ("printenv", proc::env::run_printenv),
    ("ps", proc::ps::run),
    ("pwd", file::pwd::run),
    ("pwdx", proc::pwdx::run),
    ("rexec", net::rexec::run),
    ("Rfile", ibmi::rfile::run),
    ("rm", file::rm::run),
    ("rmdir", file::rmdir::run),
    ("sed", text::sed::run),
    ("setccsid", ibmi::attr::run_setccsid),
    ("sleep", proc::sleep::run),
    ("sort", text::sort::run),
    ("split", text::split::run),
    ("system", ibmi::system::run),
    ("sysval", ibmi::sysval::run),
    ("tail", text::tail::run),
    ("tar", archive::tar::run),
    ("tee", text::tee::run),
    ("test", proc::test::run),
    ("touch", file::touch::run),
    ("tr", text::tr::run),
    ("true", proc::truefalse::run_true),
    ("uname", proc::uname::run),
    ("uncompress", archive::compress::run_uncompress),
    ("uniq", text::uniq::run),
    ("wc", text::wc::run),
    ("xargs", proc::xargs::run),
    ("zcat", archive::compress::run_zcat),
];

/// The directory the utilities live in.
pub const BIN: &str = "/usr/bin";

/// Register every utility under `/usr/bin`.
pub fn register(reg: &mut Registry) {
    for (name, f) in UTILITIES {
        reg.add(&format!("{BIN}/{name}"), Arc::new(*f));
    }
}

/// The utility of a name, if there is one.
pub fn lookup(name: &str) -> Option<Arc<dyn Utility>> {
    UTILITIES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| Arc::new(*f) as Arc<dyn Utility>)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Qshell utilities for QS/400, written from scratch: every one is a
//! [`qsh::sys::Utility`] over the [`qsh::sys::Sys`] seam alone, so the
//! same code runs over the in-memory system in tests and over the IFS
//! client on the system.
//!
//! - [`file`]: `cat chmod chgrp chown cp df du find ln ls mkdir mv rm
//!   rmdir touch`.
//! - [`text`]: `cut expand grep head iconv od paste sed sort split tail
//!   tee tr uniq wc` (and `egrep`, `fgrep`).
//! - [`proc`]: `basename date dirname env ps sleep true false uname
//!   xargs`.
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

pub mod common;
pub mod file;
pub mod proc;
pub mod regex;
pub mod text;

use std::sync::Arc;

use qsh::sys::{Registry, UtilCtx, Utility};

/// One utility: its name and entry point.
pub type Entry = (&'static str, fn(&mut UtilCtx<'_>) -> i32);

/// Every utility this crate ships, by name.
pub const UTILITIES: &[Entry] = &[
    ("basename", proc::basename::run),
    ("cat", file::cat::run),
    ("chgrp", file::chown::run_chgrp),
    ("chmod", file::chmod::run),
    ("chown", file::chown::run_chown),
    ("cp", file::cp::run),
    ("cut", text::cut::run),
    ("date", proc::date::run),
    ("df", file::df::run),
    ("dirname", proc::basename::run_dirname),
    ("echo", proc::echo::run),
    ("du", file::du::run),
    ("egrep", text::grep::run_egrep),
    ("env", proc::env::run),
    ("expand", text::expand::run),
    ("false", proc::truefalse::run_false),
    ("fgrep", text::grep::run_fgrep),
    ("find", file::find::run),
    ("grep", text::grep::run),
    ("head", text::head::run),
    ("iconv", text::iconv::run),
    ("ln", file::ln::run),
    ("ls", file::ls::run),
    ("mkdir", file::mkdir::run),
    ("mv", file::mv::run),
    ("od", text::od::run),
    ("paste", text::paste::run),
    ("ps", proc::ps::run),
    ("rm", file::rm::run),
    ("rmdir", file::rmdir::run),
    ("sed", text::sed::run),
    ("sleep", proc::sleep::run),
    ("sort", text::sort::run),
    ("split", text::split::run),
    ("tail", text::tail::run),
    ("tee", text::tee::run),
    ("touch", file::touch::run),
    ("tr", text::tr::run),
    ("true", proc::truefalse::run_true),
    ("uname", proc::uname::run),
    ("uniq", text::uniq::run),
    ("wc", text::wc::run),
    ("xargs", proc::xargs::run),
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

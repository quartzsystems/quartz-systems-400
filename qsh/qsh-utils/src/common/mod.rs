// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What every utility shares: option parsing ([`args`]), reading and
//! writing through the descriptors ([`io`]), civil time ([`time`]),
//! permission modes ([`mode`]), running other programs ([`exec`]) and
//! walking directory trees ([`walk`]).

pub mod args;
pub mod exec;
pub mod io;
pub mod mode;
pub mod time;
pub mod walk;

use qsh::sys::{SysError, UtilCtx};

/// The exit status of a usage error.
pub const USAGE: i32 = 2;

/// Write `name: text` to standard error.
pub fn complain(cx: &UtilCtx<'_>, name: &str, text: &str) {
    let _ = cx.err(&format!("{name}: {text}\n"));
}

/// Write `name: what: error text` to standard error.
pub fn complain_sys(cx: &UtilCtx<'_>, name: &str, what: &str, e: SysError) {
    let _ = cx.err(&format!("{name}: {what}: {}\n", e.errno.text()));
}

/// The system error behind an I/O error.
pub fn sys_error(e: &std::io::Error) -> SysError {
    match e.get_ref().and_then(|r| r.downcast_ref::<SysError>()) {
        Some(s) => *s,
        None => SysError::from(std::io::Error::new(e.kind(), "")),
    }
}

/// A walked path as the user named it: `abs` (the root's absolute
/// path) replaced by `op` (the operand).
pub fn shown(op: &str, abs: &str, path: &str) -> String {
    match path.strip_prefix(abs) {
        Some(rest) => format!("{op}{rest}"),
        None => path.to_string(),
    }
}

/// Report a usage error and give the status to return.
pub fn usage(cx: &UtilCtx<'_>, name: &str, text: &str) -> i32 {
    complain(cx, name, text);
    USAGE
}

/// The name the utility was called as, without a directory.
pub fn argv0(cx: &UtilCtx<'_>) -> String {
    cx.argv
        .first()
        .map(|a| a.rsplit('/').next().unwrap_or(a).to_string())
        .unwrap_or_default()
}

/// A non-negative decimal number, or `None`.
pub fn number(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

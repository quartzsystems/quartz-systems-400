// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `uname [-amnrsv]`: `OS400`, the system name, the release and
//! version numbers from the release identity, and the machine.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::usage;

/// The operating system name `uname -s` prints.
pub const SYSNAME: &str = "OS400";

/// Run `uname`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "uname";
    let p = match getopt(&cx.argv[1..], "amnrsv") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if !p.operands.is_empty() {
        return usage(cx, name, "No operands are taken.");
    }
    let all = p.has('a');
    let mut parts: Vec<String> = Vec::new();
    let rel = release::RELEASE;
    if all || p.has('s') || p.opts.is_empty() {
        parts.push(SYSNAME.to_string());
    }
    if all || p.has('n') {
        parts.push(cx.sys.nodename());
    }
    if all || p.has('r') {
        parts.push(rel.r.to_string());
    }
    if all || p.has('v') {
        parts.push(rel.v.to_string());
    }
    if all || p.has('m') {
        parts.push(cx.sys.machine());
    }
    let mut out = Out::stdout(cx);
    out.text(&parts.join(" "));
    out.text("\n");
    out.finish(cx, name, 0)
}

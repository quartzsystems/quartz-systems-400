// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `hostname [-s]`: the system's TCP/IP host name, qualified by its
//! domain (`-s`: up to the first dot). Without a resolver the system
//! name stands in. Setting the name is not done here (`CHGTCPDMN`).

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::usage;

/// Run `hostname`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "hostname";
    let p = match getopt(&cx.argv[1..], "s") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if !p.operands.is_empty() {
        return usage(
            cx,
            name,
            "The host name cannot be set here; use the CHGTCPDMN command.",
        );
    }
    let full = cx
        .sys
        .resolver()
        .and_then(qsh::sys::Resolver::host_name)
        .unwrap_or_else(|_| cx.sys.nodename());
    let shown = if p.has('s') {
        full.split('.').next().unwrap_or_default().to_string()
    } else {
        full
    };
    let mut out = Out::stdout(cx);
    out.text(&shown);
    out.text("\n");
    out.finish(cx, name, 0)
}

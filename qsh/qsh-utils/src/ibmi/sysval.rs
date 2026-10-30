// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sysval [-p] name ...` and `sysval [-p] -n name ...`: show system
//! values (or with `-n` network attributes), one value a line; `-p`
//! puts the name before each value.

use qsh::sys::{Errno, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "sysval";

/// Run `sysval`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "np") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, NAME, "A system value name is needed.");
    }
    let net = p.has('n');
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for n in &p.operands {
        let up = n.to_ascii_uppercase();
        let r = if net {
            cx.sys.network_attribute(&up)
        } else {
            cx.sys.system_value(&up)
        };
        match r {
            Ok(v) => {
                if p.has('p') {
                    out.text(&format!("{up} {v}\n"));
                } else {
                    out.text(&format!("{v}\n"));
                }
            }
            Err(e) => {
                out.flush();
                let why = match (e.errno, net) {
                    (Errno::ENOENT, false) => "The system value is not known.",
                    (Errno::ENOENT, true) => "The network attribute is not known.",
                    _ => e.errno.text(),
                };
                complain(cx, NAME, &format!("{n}: {why}"));
                status = 1;
            }
        }
    }
    out.finish(cx, NAME, status)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pwdx pid ...`: each process's working directory, as `pid: path`.

use qsh::sys::{Errno, UtilCtx};

use crate::common::io::Out;
use crate::common::{complain, usage};

/// Run `pwdx`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "pwdx";
    let ops = &cx.argv[1..];
    if ops.is_empty() {
        return usage(cx, name, "A process ID is needed.");
    }
    if let Some(o) = ops.iter().find(|o| o.starts_with('-') && *o != "--") {
        return usage(cx, name, &format!("Option {o} is not valid."));
    }
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for a in ops.iter().filter(|o| *o != "--") {
        let Ok(pid) = a.parse::<i32>() else {
            out.flush();
            complain(cx, name, &format!("{a}: The process ID is not valid."));
            status = 1;
            continue;
        };
        match cx.sys.process_cwd(pid) {
            Ok(d) => {
                out.text(&format!("{pid}: {d}\n"));
            }
            Err(e) => {
                out.flush();
                let why = if e.errno == Errno::ESRCH {
                    "No such process."
                } else {
                    e.errno.text()
                };
                complain(cx, name, &format!("{pid}: {why}"));
                status = 1;
            }
        }
    }
    out.finish(cx, name, status)
}

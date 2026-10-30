// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `getjobid [-s] [pid ...]`: the qualified job name of each process
//! (the shell's when none is given). `-s` prints `name user number`,
//! the form the shell's `read` takes apart.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "getjobid";

/// Run `getjobid`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "s") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let mut pids = Vec::new();
    for a in &p.operands {
        match a.parse::<i32>() {
            Ok(n) if n > 0 => pids.push(n),
            _ => return usage(cx, NAME, &format!("{a} is not a process ID.")),
        }
    }
    if pids.is_empty() {
        pids.push(cx.sys.pid());
    }
    let procs = match cx.sys.processes() {
        Ok(v) => v,
        Err(e) => {
            complain(cx, NAME, &format!("The processes cannot be listed: {}", e.errno.text()));
            return 1;
        }
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for pid in pids {
        let Some(pr) = procs.iter().find(|p| p.pid == pid) else {
            out.flush();
            complain(cx, NAME, &format!("{pid}: No such process."));
            status = 1;
            continue;
        };
        if p.has('s') {
            let f: Vec<&str> = pr.job.split('/').collect();
            if let [num, user, name] = f.as_slice() {
                out.text(&format!("{name} {user} {num}\n"));
            } else {
                out.text(&format!("{}\n", pr.job));
            }
        } else {
            out.text(&format!("Process {pid} runs in job {}.\n", pr.job));
        }
    }
    out.finish(cx, NAME, status)
}

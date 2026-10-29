// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rmdir [-p] dir...`: remove empty directories, with `-p` each
//! parent named in the path too.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::{complain_sys, usage};

/// Run `rmdir`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "rmdir";
    let p = match getopt(&cx.argv[1..], "p") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, name, "A directory name is needed.");
    }
    let mut status = 0;
    for d in &p.operands {
        let mut path = d.trim_end_matches('/').to_string();
        if path.is_empty() {
            path = "/".to_string();
        }
        loop {
            if let Err(e) = cx.sys.rmdir(&cx.path(&path)) {
                complain_sys(cx, name, &path, e);
                status = 1;
                break;
            }
            if !p.has('p') {
                break;
            }
            match path.rfind('/') {
                Some(0) | None => break,
                Some(i) => path.truncate(i),
            }
            if path.is_empty() {
                break;
            }
        }
    }
    status
}

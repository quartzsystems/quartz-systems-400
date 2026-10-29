// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ln [-fs] source target`, `ln [-fs] source... directory`: hard or
//! symbolic links.

use qsh::sys::{FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::{complain, complain_sys, usage};
use crate::file::{basename, join};

/// Run `ln`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "ln";
    let p = match getopt(&cx.argv[1..], "fs") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if p.operands.len() < 2 {
        return usage(cx, name, "A source and a target are needed.");
    }
    let force = p.has('f');
    let symbolic = p.has('s');
    let target = p.operands.last().cloned().unwrap_or_default();
    let target_abs = cx.path(&target);
    let is_dir = cx
        .sys
        .stat(&target_abs)
        .is_ok_and(|st| st.ftype == FileType::Dir);
    let sources = &p.operands[..p.operands.len() - 1];
    if sources.len() > 1 && !is_dir {
        complain(cx, name, &format!("{target}: The path is not a directory."));
        return 1;
    }
    let mut status = 0;
    for src in sources {
        let dest = if is_dir {
            join(&target, basename(src))
        } else {
            target.clone()
        };
        let dest_abs = cx.path(&dest);
        if cx.sys.lstat(&dest_abs).is_ok() {
            if !force {
                complain(cx, name, &format!("{dest}: The file already exists."));
                status = 1;
                continue;
            }
            if let Err(e) = cx.sys.unlink(&dest_abs) {
                complain_sys(cx, name, &dest, e);
                status = 1;
                continue;
            }
        }
        if !symbolic
            && let Err(e) = cx.sys.stat(&cx.path(src))
        {
            complain_sys(cx, name, src, e);
            status = 1;
            continue;
        }
        let r = if symbolic {
            cx.sys.symlink(src, &dest_abs)
        } else {
            cx.sys.link(&cx.path(src), &dest_abs)
        };
        if let Err(e) = r {
            complain_sys(cx, name, &dest, e);
            status = 1;
        }
    }
    status
}

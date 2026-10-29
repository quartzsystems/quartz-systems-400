// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `mv [-fi] source target`, `mv [-fi] source... directory`: rename,
//! or copy and remove across file systems.

use qsh::sys::{Errno, FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::confirm;
use crate::common::{complain, complain_sys, usage};
use crate::file::cp::{Opts, copy_path, is_dir};
use crate::file::rm::remove;
use crate::file::{basename, join};

fn move_one(cx: &UtilCtx<'_>, name: &str, src: &str, dst: &str, force: bool, ask: bool) -> bool {
    let src_abs = cx.path(src);
    let dst_abs = cx.path(dst);
    let st = match cx.sys.lstat(&src_abs) {
        Ok(st) => st,
        Err(e) => {
            complain_sys(cx, name, src, e);
            return false;
        }
    };
    if let Ok(d) = cx.sys.lstat(&dst_abs) {
        if d.dev == st.dev && d.ino == st.ino {
            complain(cx, name, &format!("{src} and {dst} are the same file."));
            return false;
        }
        if ask && !confirm(cx, &format!("{name}: replace {dst}? ")) {
            return true;
        }
        if d.ftype == FileType::Dir && st.ftype != FileType::Dir {
            complain(cx, name, &format!("{dst}: The target is a directory."));
            return false;
        }
    }
    match cx.sys.rename(&src_abs, &dst_abs) {
        Ok(()) => true,
        Err(e) if e.errno == Errno::EXDEV => {
            let o = Opts {
                force,
                preserve: true,
                recursive: true,
                ..Opts::default()
            };
            if !copy_path(cx, name, src, dst, o, true) {
                return false;
            }
            remove(cx, name, src, &src_abs, true, true, false)
        }
        Err(e) => {
            complain_sys(cx, name, src, e);
            false
        }
    }
}

/// Run `mv`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "mv";
    let p = match getopt(&cx.argv[1..], "fi") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let force = p.last_of('f', 'i') == Some('f');
    let ask = p.last_of('f', 'i') == Some('i');
    if p.operands.len() < 2 {
        return usage(cx, name, "A source and a target are needed.");
    }
    let target = p.operands.last().cloned().unwrap_or_default();
    let sources = &p.operands[..p.operands.len() - 1];
    let target_is_dir = is_dir(cx, &target);
    if sources.len() > 1 && !target_is_dir {
        complain(cx, name, &format!("{target}: The path is not a directory."));
        return 1;
    }
    let mut status = 0;
    for src in sources {
        let dst = if target_is_dir {
            join(&target, basename(src))
        } else {
            target.clone()
        };
        if !move_one(cx, name, src, &dst, force, ask) {
            status = 1;
        }
    }
    status
}

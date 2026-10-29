// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rm [-fiRr] file...`: remove files, and trees with `-r`.

use qsh::sys::{FileType, Stat, Sys, SysError, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::confirm;
use crate::common::{complain, complain_sys, usage};

/// Remove one path, recursing into directories when `recursive`.
/// Returns `false` on any failure (already reported).
pub fn remove(
    cx: &UtilCtx<'_>,
    name: &str,
    shown: &str,
    abs: &str,
    recursive: bool,
    force: bool,
    interactive: bool,
) -> bool {
    let sys: &dyn Sys = cx.sys;
    let st: Stat = match sys.lstat(abs) {
        Ok(st) => st,
        Err(e) => {
            if !force {
                complain_sys(cx, name, shown, e);
            }
            return force;
        }
    };
    if st.ftype == FileType::Dir {
        if !recursive {
            complain(cx, name, &format!("{shown}: The target is a directory."));
            return false;
        }
        let mut ok = true;
        match sys.read_dir(abs) {
            Ok(mut names) => {
                names.sort();
                for n in names {
                    let child_shown = crate::file::join(shown, &n);
                    let child_abs = crate::file::join(abs, &n);
                    ok &= remove(cx, name, &child_shown, &child_abs, true, force, interactive);
                }
            }
            Err(e) => {
                complain_sys(cx, name, shown, e);
                return false;
            }
        }
        if !ok {
            return false;
        }
        if interactive && !confirm(cx, &format!("{name}: remove the directory {shown}? ")) {
            return true;
        }
        return report(cx, name, shown, sys.rmdir(abs));
    }
    if interactive && !confirm(cx, &format!("{name}: remove {shown}? ")) {
        return true;
    }
    report(cx, name, shown, sys.unlink(abs))
}

fn report(cx: &UtilCtx<'_>, name: &str, shown: &str, r: Result<(), SysError>) -> bool {
    match r {
        Ok(()) => true,
        Err(e) => {
            complain_sys(cx, name, shown, e);
            false
        }
    }
}

/// Run `rm`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "rm";
    let p = match getopt(&cx.argv[1..], "fiRr") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let force = p.last_of('f', 'i') == Some('f');
    let interactive = p.last_of('f', 'i') == Some('i');
    let recursive = p.has('r') || p.has('R');
    if p.operands.is_empty() {
        if force {
            return 0;
        }
        return usage(cx, name, "A file name is needed.");
    }
    let mut status = 0;
    for f in &p.operands {
        let base = crate::file::basename(f);
        if base == "." || base == ".." || f == "/" {
            complain(cx, name, &format!("{f}: This path cannot be removed."));
            status = 1;
            continue;
        }
        if !remove(cx, name, f, &cx.path(f), recursive, force, interactive) {
            status = 1;
        }
    }
    status
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pwd [-L | -P]`: the working directory. `-L` (the default) prints
//! `PWD` when it is absolute, has no `.` or `..` and names the working
//! directory; otherwise, and with `-P`, the path with links resolved.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain_sys, usage};

const NAME: &str = "pwd";

fn logical(cx: &UtilCtx<'_>) -> Option<String> {
    let pwd = cx.getenv("PWD")?;
    if !pwd.starts_with('/') || pwd.split('/').any(|c| c == "." || c == "..") {
        return None;
    }
    let a = cx.sys.stat(pwd).ok()?;
    let b = cx.sys.stat(&cx.cwd).ok()?;
    (a.dev == b.dev && a.ino == b.ino).then(|| pwd.to_string())
}

/// Run `pwd`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "LP") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if !p.operands.is_empty() {
        return usage(cx, NAME, "No operands are taken.");
    }
    let phys = p.last_of('L', 'P') == Some('P');
    let path = match (phys, logical(cx)) {
        (false, Some(l)) => l,
        _ => match cx.sys.realpath(&cx.cwd) {
            Ok(r) => r,
            Err(e) => {
                complain_sys(cx, NAME, ".", e);
                return 1;
            }
        },
    };
    let mut out = Out::stdout(cx);
    out.text(&path);
    out.text("\n");
    out.finish(cx, NAME, 0)
}

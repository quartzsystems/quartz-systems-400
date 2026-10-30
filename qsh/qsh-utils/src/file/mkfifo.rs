// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `mkfifo [-m mode] file...`: make FIFOs, mode `0666` less the umask
//! or exactly `-m` (octal or symbolic from `a=rw`).

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::{complain_sys, mode, usage};

const NAME: &str = "mkfifo";

/// Run `mkfifo`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "m:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, NAME, "A file name is needed.");
    }
    let bits = match p.value('m') {
        Some(m) => match mode::parse(m, 0o666, false, cx.umask) {
            Some(b) => b,
            None => return usage(cx, NAME, &format!("{m} is not a valid mode.")),
        },
        None => 0o666 & !cx.umask,
    };
    let mut status = 0;
    for f in &p.operands {
        let abs = cx.path(f);
        let r = cx.sys.mkfifo(&abs, bits).and_then(|()| {
            if p.has('m') {
                cx.sys.chmod(&abs, bits).or(Ok(()))
            } else {
                Ok(())
            }
        });
        if let Err(e) = r {
            complain_sys(cx, NAME, f, e);
            status = 1;
        }
    }
    status
}

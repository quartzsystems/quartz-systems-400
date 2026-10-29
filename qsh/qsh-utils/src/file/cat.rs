// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cat [-u] [file...]`: copy each input to standard output. A member
//! of a `/QSYS.LIB` file reads as its records, one a line, through
//! the system.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Out, input, inputs};
use crate::common::{complain_sys, usage};

/// Run `cat`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "cat";
    let p = match getopt(&cx.argv[1..], "u") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    let mut buf = [0u8; 16384];
    for f in inputs(&p.operands) {
        let fd = match input(cx, &f) {
            Ok(fd) => fd,
            Err(e) => {
                complain_sys(cx, name, &f, e);
                status = 1;
                continue;
            }
        };
        loop {
            match fd.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if !out.write(&buf[..n]) {
                        return out.finish(cx, name, status);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    complain_sys(cx, name, &f, e.into());
                    status = 1;
                    break;
                }
            }
        }
    }
    out.finish(cx, name, status)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tee [-ai] [file...]`: copy standard input to standard output and
//! to each file.

use qsh::sys::stream::write_all;
use qsh::sys::{Fd, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::{open_write, stdin};
use crate::common::{complain_sys, usage};

/// Run `tee`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "tee";
    let p = match getopt(&cx.argv[1..], "ai") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let append = p.has('a');
    let mut status = 0;
    let mut sinks: Vec<(String, Fd)> = Vec::new();
    if let Some(fd) = cx.fd(1) {
        sinks.push(("standard output".to_string(), fd.clone()));
    }
    for f in &p.operands {
        match open_write(cx, f, append) {
            Ok(fd) => sinks.push((f.clone(), fd)),
            Err(e) => {
                complain_sys(cx, name, f, e);
                status = 1;
            }
        }
    }
    let input = stdin(cx);
    let mut buf = [0u8; 16384];
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                complain_sys(cx, name, "standard input", e.into());
                status = 1;
                break;
            }
        };
        let mut failed = Vec::new();
        for (i, (label, fd)) in sinks.iter().enumerate() {
            if let Err(e) = write_all(&**fd, &buf[..n]) {
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    complain_sys(cx, name, label, e.into());
                }
                status = 1;
                failed.push(i);
            }
        }
        for i in failed.into_iter().rev() {
            sinks.remove(i);
        }
        if sinks.is_empty() {
            break;
        }
    }
    status
}

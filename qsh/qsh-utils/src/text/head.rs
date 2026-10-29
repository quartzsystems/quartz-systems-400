// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `head [-n number | -c number] [file...]`: the first lines (or
//! bytes) of each input, with `==> name <==` headers for several.

use qsh::sys::UtilCtx;

use crate::common::args::getopt_from;
use crate::common::io::{Lines, Out, input, inputs};
use crate::common::{complain_sys, number, usage};

/// Run `head`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "head";
    let p = match getopt_from(&cx.argv[1..], "n:c:", &|a| {
        a.len() > 1 && a.starts_with('-') && a[1..].bytes().all(|b| b.is_ascii_digit())
    }) {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut lines: u64 = 10;
    let mut bytes: Option<u64> = None;
    for (o, v) in &p.opts {
        let v = v.as_deref().unwrap_or("");
        match o {
            '\u{0}' => lines = v[1..].parse().unwrap_or(10),
            'n' => match number(v) {
                Some(n) => {
                    lines = n;
                    bytes = None;
                }
                None => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            'c' => match number(v) {
                Some(n) => bytes = Some(n),
                None => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            _ => {}
        }
    }
    let files = inputs(&p.operands);
    let many = files.len() > 1;
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for (i, f) in files.iter().enumerate() {
        let fd = match input(cx, f) {
            Ok(fd) => fd,
            Err(e) => {
                complain_sys(cx, name, f, e);
                status = 1;
                continue;
            }
        };
        if many {
            if i > 0 {
                out.text("\n");
            }
            out.text(&format!("==> {f} <==\n"));
        }
        let mut rd = Lines::new(fd);
        if let Some(n) = bytes {
            let mut left = n;
            while left > 0 {
                match rd.next_byte() {
                    Ok(Some(b)) => {
                        if !out.write(&[b]) {
                            return out.finish(cx, name, status);
                        }
                        left -= 1;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        complain_sys(cx, name, f, e.into());
                        status = 1;
                        break;
                    }
                }
            }
        } else {
            let mut left = lines;
            while left > 0 {
                match rd.next_line() {
                    Ok(Some(l)) => {
                        if !out.write(&l) {
                            return out.finish(cx, name, status);
                        }
                        left -= 1;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        complain_sys(cx, name, f, e.into());
                        status = 1;
                        break;
                    }
                }
            }
        }
    }
    out.finish(cx, name, status)
}

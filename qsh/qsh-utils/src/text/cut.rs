// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cut -b list [-n] | -c list | -f list [-d delim] [-s] [file...]`:
//! selected bytes, characters or fields of each line.

use qsh::sys::UtilCtx;

use super::Positions;
use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input, inputs};
use crate::common::{complain_sys, usage};

enum Mode {
    Bytes,
    Chars,
    Fields(u8, bool),
}

/// Run `cut`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "cut";
    let p = match getopt(&cx.argv[1..], "b:c:f:d:ns") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let given: Vec<char> = ['b', 'c', 'f'].into_iter().filter(|c| p.has(*c)).collect();
    if given.len() != 1 {
        return usage(cx, name, "One of -b, -c or -f is needed.");
    }
    let which = given[0];
    let list = p.value(which).unwrap_or("");
    let Some(pos) = Positions::parse(list) else {
        return usage(cx, name, &format!("The list {list} is not valid."));
    };
    let mode = match which {
        'b' => Mode::Bytes,
        'c' => Mode::Chars,
        _ => {
            let d = p.value('d').unwrap_or("\t");
            let db = d.as_bytes();
            if db.len() != 1 {
                return usage(cx, name, "The delimiter must be one character.");
            }
            Mode::Fields(db[0], p.has('s'))
        }
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for f in inputs(&p.operands) {
        let fd = match input(cx, &f) {
            Ok(fd) => fd,
            Err(e) => {
                complain_sys(cx, name, &f, e);
                status = 1;
                continue;
            }
        };
        let mut rd = Lines::new(fd);
        loop {
            let (line, _) = match rd.next_text() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    complain_sys(cx, name, &f, e.into());
                    status = 1;
                    break;
                }
            };
            let mut o = Vec::new();
            match &mode {
                Mode::Bytes => {
                    for (i, b) in line.iter().enumerate() {
                        if pos.has(i + 1) {
                            o.push(*b);
                        }
                    }
                }
                Mode::Chars => {
                    let text = String::from_utf8_lossy(&line);
                    let mut s = String::new();
                    for (i, c) in text.chars().enumerate() {
                        if pos.has(i + 1) {
                            s.push(c);
                        }
                    }
                    o = s.into_bytes();
                }
                Mode::Fields(d, only) => {
                    if line.contains(d) {
                        let mut first = true;
                        for (i, fld) in line.split(|b| b == d).enumerate() {
                            if pos.has(i + 1) {
                                if !first {
                                    o.push(*d);
                                }
                                first = false;
                                o.extend_from_slice(fld);
                            }
                        }
                    } else {
                        if *only {
                            continue;
                        }
                        o.clone_from(&line);
                    }
                }
            }
            o.push(b'\n');
            if !out.write(&o) {
                return out.finish(cx, name, status);
            }
        }
    }
    out.finish(cx, name, status)
}

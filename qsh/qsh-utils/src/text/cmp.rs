// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cmp [-l | -s] file1 file2 [skip1 [skip2]]`: compare two files byte
//! by byte. The first difference is reported in POSIX's form
//! (`a b differ: char N, line M`); `-l` lists every difference as
//! `N o1 o2`; `-s` only sets the status. Status 0 same, 1 different,
//! 2 trouble.

use std::io;

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "cmp";

/// A skip operand: decimal, `0x` hex or `0` octal.
fn skip(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok();
    }
    if s.len() > 1 && s.starts_with('0') {
        return u64::from_str_radix(&s[1..], 8).ok();
    }
    crate::common::number(s)
}

fn skip_bytes(l: &mut Lines, n: u64) -> io::Result<()> {
    for _ in 0..n {
        if l.next_byte()?.is_none() {
            break;
        }
    }
    Ok(())
}

/// Run `cmp`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "ls") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.has('l') && p.has('s') {
        return usage(cx, NAME, "Options -l and -s cannot be used together.");
    }
    let ops = &p.operands;
    if ops.len() < 2 || ops.len() > 4 {
        return usage(cx, NAME, "Two files are needed, then up to two skip counts.");
    }
    if ops[0] == "-" && ops[1] == "-" {
        return usage(cx, NAME, "Only one file can be standard input.");
    }
    let mut skips = [0u64; 2];
    for (i, s) in ops[2..].iter().enumerate() {
        match skip(s) {
            Some(n) => skips[i] = n,
            None => return usage(cx, NAME, &format!("{s} is not a valid skip count.")),
        }
    }
    let silent = p.has('s');
    let mut files = Vec::new();
    for f in &ops[..2] {
        match input(cx, f) {
            Ok(fd) => files.push(Lines::new(fd)),
            Err(e) => {
                if !silent {
                    complain_sys(cx, NAME, f, e);
                }
                return 2;
            }
        }
    }
    let shown = |i: usize| {
        if ops[i] == "-" {
            "standard input".to_string()
        } else {
            ops[i].clone()
        }
    };
    let (Some(mut b), Some(mut a)) = (files.pop(), files.pop()) else {
        return 2;
    };
    if let Err(e) = skip_bytes(&mut a, skips[0]).and_then(|()| skip_bytes(&mut b, skips[1])) {
        if !silent {
            complain_sys(cx, NAME, &ops[0], e.into());
        }
        return 2;
    }
    let mut out = Out::stdout(cx);
    let mut pos: u64 = 0;
    let mut line: u64 = 1;
    let mut differ = false;
    loop {
        let (x, y) = match (a.next_byte(), b.next_byte()) {
            (Ok(x), Ok(y)) => (x, y),
            (Err(e), _) => {
                complain_sys(cx, NAME, &ops[0], e.into());
                return 2;
            }
            (_, Err(e)) => {
                complain_sys(cx, NAME, &ops[1], e.into());
                return 2;
            }
        };
        pos += 1;
        match (x, y) {
            (None, None) => break,
            (Some(_), None) | (None, Some(_)) => {
                if !silent {
                    let short = usize::from(x.is_some());
                    out.flush();
                    complain(cx, NAME, &format!("End of file on {}.", shown(short)));
                }
                return out.finish(cx, NAME, 1);
            }
            (Some(x), Some(y)) => {
                if x != y {
                    differ = true;
                    if silent {
                        return 1;
                    }
                    if p.has('l') {
                        out.text(&format!("{pos} {x:o} {y:o}\n"));
                    } else {
                        out.text(&format!(
                            "{} {} differ: char {pos}, line {line}\n",
                            shown(0),
                            shown(1)
                        ));
                        return out.finish(cx, NAME, 1);
                    }
                }
                if x == b'\n' {
                    line += 1;
                }
            }
        }
    }
    out.finish(cx, NAME, i32::from(differ))
}

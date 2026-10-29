// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `echo [-n] [string...]` as a program, for `xargs` and `find -exec`:
//! the shell's builtin answers at the command line.

use qsh::sys::UtilCtx;

use crate::common::io::Out;

/// Expand the escapes the shell's `echo` knows.
fn unescape(s: &str, out: &mut String) -> bool {
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] != '\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'c' => return true,
            'f' => out.push('\u{c}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'v' => out.push('\u{b}'),
            '\\' => out.push('\\'),
            '0' => {
                let mut v = 0u32;
                let mut n = 0;
                while n < 3 && i + 1 < b.len() && b[i + 1].is_digit(8) {
                    v = v * 8 + b[i + 1].to_digit(8).unwrap_or(0);
                    i += 1;
                    n += 1;
                }
                out.push(char::from_u32(v).unwrap_or('\0'));
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
        i += 1;
    }
    false
}

/// Run `echo`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let mut args: &[String] = &cx.argv[1..];
    let mut newline = true;
    if args.first().is_some_and(|a| a == "-n") {
        newline = false;
        args = &args[1..];
    }
    let mut s = String::new();
    let mut stop = false;
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        if unescape(a, &mut s) {
            stop = true;
            break;
        }
    }
    if newline && !stop {
        s.push('\n');
    }
    let mut out = Out::stdout(cx);
    out.text(&s);
    out.finish(cx, "echo", 0)
}

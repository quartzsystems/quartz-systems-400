// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `uniq [-c | -d | -u] [-f fields] [-s chars] [input [output]]`:
//! collapse repeated adjacent lines. The old `-n` and `+n` forms are
//! taken too.

use qsh::sys::UtilCtx;

use crate::common::args::getopt_from;
use crate::common::io::{Lines, Out, input, open_write};
use crate::common::{complain_sys, number, usage};

/// The part of a line compared: after `fields` fields and `chars`
/// bytes.
fn key(line: &[u8], fields: usize, chars: usize) -> &[u8] {
    let mut i = 0;
    for _ in 0..fields {
        while i < line.len() && line[i].is_ascii_whitespace() {
            i += 1;
        }
        while i < line.len() && !line[i].is_ascii_whitespace() {
            i += 1;
        }
    }
    &line[(i + chars).min(line.len())..]
}

/// Run `uniq`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "uniq";
    let p = match getopt_from(&cx.argv[1..], "cduf:s:", &|a| {
        a.len() > 1
            && (a.starts_with('-') || a.starts_with('+'))
            && a[1..].bytes().all(|b| b.is_ascii_digit())
    }) {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let (mut fields, mut chars) = (0usize, 0usize);
    for (o, v) in &p.opts {
        let v = v.as_deref().unwrap_or("");
        match o {
            '\u{0}' => {
                let n = v[1..].parse().unwrap_or(0);
                if v.starts_with('-') {
                    fields = n;
                } else {
                    chars = n;
                }
            }
            'f' => match number(v) {
                Some(n) => fields = n as usize,
                None => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            's' => match number(v) {
                Some(n) => chars = n as usize,
                None => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            _ => {}
        }
    }
    let count = p.has('c');
    let only_dup = p.has('d');
    let only_uniq = p.has('u');
    if p.operands.len() > 2 {
        return usage(cx, name, "Too many operands.");
    }
    let src = p.operands.first().map_or("-", String::as_str);
    let fd = match input(cx, src) {
        Ok(fd) => fd,
        Err(e) => {
            complain_sys(cx, name, src, e);
            return 1;
        }
    };
    let mut out = match p.operands.get(1) {
        None => Out::stdout(cx),
        Some(dst) => match open_write(cx, dst, false) {
            Ok(fd) => Out::on(Some(fd)),
            Err(e) => {
                complain_sys(cx, name, dst, e);
                return 1;
            }
        },
    };
    let mut rd = Lines::new(fd);
    let mut cur: Option<Vec<u8>> = None;
    let mut n = 0u64;
    let emit = |out: &mut Out, line: &[u8], n: u64| -> bool {
        if (only_dup && n < 2) || (only_uniq && n > 1) {
            return true;
        }
        if count {
            out.text(&format!("{n:>4} "));
        }
        out.write(line)
    };
    loop {
        let line = match rd.next_line() {
            Ok(Some(l)) => Some(l),
            Ok(None) => None,
            Err(e) => {
                complain_sys(cx, name, src, e.into());
                return 1;
            }
        };
        match (&cur, &line) {
            (Some(c), Some(l))
                if key(c.strip_suffix(b"\n").unwrap_or(c), fields, chars)
                    == key(l.strip_suffix(b"\n").unwrap_or(l), fields, chars) =>
            {
                n += 1;
                continue;
            }
            _ => {}
        }
        if let Some(c) = &cur {
            let mut c = c.clone();
            if c.last() != Some(&b'\n') {
                c.push(b'\n');
            }
            if !emit(&mut out, &c, n) {
                break;
            }
        }
        match line {
            Some(l) => {
                cur = Some(l);
                n = 1;
            }
            None => break,
        }
    }
    out.finish(cx, name, 0)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tail [-f] [-c number | -n number] [file...]`: the last lines (or
//! bytes), or from the `+number`th on. `-f` is accepted; the input
//! ends at end of file.

use qsh::sys::UtilCtx;

use crate::common::args::getopt_from;
use crate::common::io::{Out, input, inputs, read_input, split_lines};
use crate::common::{complain_sys, usage};

#[derive(Clone, Copy)]
enum Count {
    Last(u64),
    From(u64),
}

fn count(s: &str) -> Option<Count> {
    let (from, digits) = match s.as_bytes().first() {
        Some(b'+') => (true, &s[1..]),
        Some(b'-') => (false, &s[1..]),
        _ => (false, s),
    };
    let n: u64 = digits.parse().ok()?;
    Some(if from { Count::From(n) } else { Count::Last(n) })
}

/// Run `tail`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "tail";
    let p = match getopt_from(&cx.argv[1..], "fn:c:", &|a| {
        a.len() > 1
            && (a.starts_with('-') || a.starts_with('+'))
            && a[1..].bytes().all(|b| b.is_ascii_digit())
    }) {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut lines = Count::Last(10);
    let mut bytes: Option<Count> = None;
    for (o, v) in &p.opts {
        let v = v.as_deref().unwrap_or("");
        match o {
            '\u{0}' => lines = count(v).unwrap_or(Count::Last(10)),
            'n' => match count(v) {
                Some(c) => {
                    lines = c;
                    bytes = None;
                }
                None => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            'c' => match count(v) {
                Some(c) => bytes = Some(c),
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
        let data = match input(cx, f).and_then(|_| read_input(cx, f)) {
            Ok(d) => d,
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
        let piece: &[u8] = match bytes {
            Some(Count::Last(n)) => &data[data.len().saturating_sub(n as usize)..],
            Some(Count::From(n)) => &data[(n.max(1) as usize - 1).min(data.len())..],
            None => {
                let ls = split_lines(&data);
                let start = match lines {
                    Count::Last(n) => ls.len().saturating_sub(n as usize),
                    Count::From(n) => (n.max(1) as usize - 1).min(ls.len()),
                };
                let off: usize = ls[..start].iter().map(|l| l.len()).sum();
                &data[off..]
            }
        };
        if !out.write(piece) {
            break;
        }
    }
    out.finish(cx, name, status)
}

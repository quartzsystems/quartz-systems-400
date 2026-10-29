// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `wc [-c | -m] [-lw] [file...]`: line, word and byte counts, each in
//! an eight-column field, with a `total` line for several files.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input, inputs};
use crate::common::{complain_sys, usage};

#[derive(Default, Clone, Copy)]
struct Counts {
    lines: u64,
    words: u64,
    bytes: u64,
    chars: u64,
}

fn count(rd: &mut Lines) -> std::io::Result<Counts> {
    let mut c = Counts::default();
    let mut in_word = false;
    while let Some(b) = rd.next_byte()? {
        c.bytes += 1;
        if b & 0xC0 != 0x80 {
            c.chars += 1;
        }
        if b == b'\n' {
            c.lines += 1;
        }
        if b.is_ascii_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            c.words += 1;
        }
    }
    Ok(c)
}

/// Run `wc`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "wc";
    let p = match getopt(&cx.argv[1..], "cmlw") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let (mut l, mut w, mut c, mut m) = (p.has('l'), p.has('w'), p.has('c'), p.has('m'));
    if p.last_of('c', 'm') == Some('m') {
        c = false;
    } else if c {
        m = false;
    }
    if !(l || w || c || m) {
        l = true;
        w = true;
        c = true;
    }
    let files = inputs(&p.operands);
    let mut out = Out::stdout(cx);
    let mut status = 0;
    let mut total = Counts::default();
    let line = |cnt: &Counts, label: &str| {
        let mut s = String::new();
        if l {
            s.push_str(&format!("{:>8}", cnt.lines));
        }
        if w {
            s.push_str(&format!("{:>8}", cnt.words));
        }
        if c {
            s.push_str(&format!("{:>8}", cnt.bytes));
        }
        if m {
            s.push_str(&format!("{:>8}", cnt.chars));
        }
        if !label.is_empty() {
            s.push(' ');
            s.push_str(label);
        }
        s.push('\n');
        s
    };
    for f in &files {
        let cnt = match input(cx, f).and_then(|fd| count(&mut Lines::new(fd)).map_err(Into::into)) {
            Ok(c) => c,
            Err(e) => {
                complain_sys(cx, name, f, e);
                status = 1;
                continue;
            }
        };
        total.lines += cnt.lines;
        total.words += cnt.words;
        total.bytes += cnt.bytes;
        total.chars += cnt.chars;
        out.text(&line(&cnt, if f == "-" && p.operands.is_empty() { "" } else { f }));
    }
    if files.len() > 1 {
        out.text(&line(&total, "total"));
    }
    out.finish(cx, name, status)
}

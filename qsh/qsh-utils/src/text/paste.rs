// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `paste [-s] [-d list] file...`: lines of each file side by side, or
//! each file's lines joined on one line with `-s`.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input, inputs};
use crate::common::{complain_sys, usage};

/// The delimiter list with its escapes: `\n`, `\t`, `\\`, `\0` (none).
fn delimiters(s: &str) -> Vec<Option<u8>> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            i += 1;
            out.push(match b[i] {
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'0' => None,
                other => Some(other),
            });
        } else {
            out.push(Some(b[i]));
        }
        i += 1;
    }
    if out.is_empty() {
        out.push(Some(b'\t'));
    }
    out
}

/// Run `paste`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "paste";
    let p = match getopt(&cx.argv[1..], "sd:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let delims = delimiters(p.value('d').unwrap_or("\t"));
    let serial = p.has('s');
    let files = inputs(&p.operands);
    let mut out = Out::stdout(cx);
    let mut status = 0;
    // Each operand's reader; every `-` shares one, so `paste - -` takes
    // standard input's lines in turn.
    let mut sources: Vec<Lines> = Vec::new();
    let mut slots: Vec<Option<usize>> = Vec::new();
    let mut stdin_slot: Option<usize> = None;
    for f in &files {
        if f == "-"
            && let Some(i) = stdin_slot
        {
            slots.push(Some(i));
            continue;
        }
        match input(cx, f) {
            Ok(fd) => {
                sources.push(Lines::new(fd));
                slots.push(Some(sources.len() - 1));
                if f == "-" {
                    stdin_slot = Some(sources.len() - 1);
                }
            }
            Err(e) => {
                complain_sys(cx, name, f, e);
                status = 1;
                slots.push(None);
            }
        }
    }
    if serial {
        for rd in &mut sources {
            let mut line = Vec::new();
            let mut k = 0;
            let mut first = true;
            while let Ok(Some((l, _))) = rd.next_text() {
                if !first {
                    if let Some(d) = delims[k % delims.len()] {
                        line.push(d);
                    }
                    k += 1;
                }
                first = false;
                line.extend(l);
            }
            line.push(b'\n');
            if !out.write(&line) {
                break;
            }
        }
        return out.finish(cx, name, status);
    }
    loop {
        let mut line = Vec::new();
        let mut any = false;
        let n = slots.len();
        for (i, slot) in slots.iter().enumerate() {
            let next = slot.and_then(|s| sources[s].next_text().ok().flatten());
            let l = match next {
                Some((l, _)) => {
                    any = true;
                    l
                }
                None => Vec::new(),
            };
            line.extend(l);
            if i + 1 < n
                && let Some(d) = delims[i % delims.len()]
            {
                line.push(d);
            }
        }
        if !any {
            break;
        }
        line.push(b'\n');
        if !out.write(&line) {
            break;
        }
    }
    out.finish(cx, name, status)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `grep [-E | -F] [-c | -l | -q] [-hHinsvx] [-e pattern]... [-f file]...
//! [pattern] [file...]`, with `egrep` and `fgrep` as `-E` and `-F`.
//! Status 0 when a line was selected, 1 when none, 2 on an error.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Lines, Out, input, inputs, read_input, split_lines};
use crate::common::{complain, complain_sys, usage};
use crate::regex::{Regex, Syntax};

/// Run `grep`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    grep(cx, "grep", None)
}

/// Run `egrep`.
pub fn run_egrep(cx: &mut UtilCtx<'_>) -> i32 {
    grep(cx, "egrep", Some('E'))
}

/// Run `fgrep`.
pub fn run_fgrep(cx: &mut UtilCtx<'_>) -> i32 {
    grep(cx, "fgrep", Some('F'))
}

fn grep(cx: &mut UtilCtx<'_>, name: &str, forced: Option<char>) -> i32 {
    let p = match getopt(&cx.argv[1..], "EFclqhHinsvxe:f:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mode = forced.or_else(|| p.last_of('E', 'F')).unwrap_or('G');
    let icase = p.has('i');
    let invert = p.has('v');
    let whole = p.has('x');
    let count = p.has('c');
    let names_only = p.has('l');
    let quiet = p.has('q');
    let silent = p.has('s');
    let numbers = p.has('n');
    let mut patterns: Vec<Vec<u8>> = Vec::new();
    for e in p.values('e') {
        patterns.push(e.as_bytes().to_vec());
    }
    for f in p.values('f') {
        match read_input(cx, f) {
            Ok(d) => {
                for l in split_lines(&d) {
                    patterns.push(l.strip_suffix(b"\n").unwrap_or(l).to_vec());
                }
            }
            Err(e) => {
                complain_sys(cx, name, f, e);
                return 2;
            }
        }
    }
    let mut operands = p.operands.clone();
    if patterns.is_empty() {
        if operands.is_empty() {
            return usage(cx, name, "A pattern is needed.");
        }
        patterns.push(operands.remove(0).into_bytes());
    }
    let mut regexes = Vec::new();
    for pat in &patterns {
        let r = match mode {
            'F' => Ok(Regex::literal(pat, icase)),
            'E' => Regex::new(pat, Syntax::Extended, icase),
            _ => Regex::new(pat, Syntax::Basic, icase),
        };
        match r {
            Ok(r) => regexes.push(r),
            Err(e) => {
                complain(cx, name, &e.to_string());
                return 2;
            }
        }
    }
    let files = inputs(&operands);
    let with_names = match p.last_of('h', 'H') {
        Some('h') => false,
        Some(_) => true,
        None => files.len() > 1,
    };
    let selected = |line: &[u8]| -> bool {
        let hit = regexes.iter().any(|r| {
            if whole {
                r.find_at(line, 0)
                    .is_some_and(|m| m.start == 0 && m.end == line.len())
            } else {
                r.is_match(line)
            }
        });
        hit != invert
    };
    let mut out = Out::stdout(cx);
    let mut any = false;
    let mut error = false;
    'files: for f in &files {
        let fd = match input(cx, f) {
            Ok(fd) => fd,
            Err(e) => {
                if !silent {
                    complain_sys(cx, name, f, e);
                }
                error = true;
                continue;
            }
        };
        let label = if f == "-" { "(standard input)" } else { f };
        let mut rd = Lines::new(fd);
        let mut n = 0u64;
        let mut hits = 0u64;
        loop {
            let (line, _) = match rd.next_text() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    if !silent {
                        complain_sys(cx, name, f, e.into());
                    }
                    error = true;
                    break;
                }
            };
            n += 1;
            if !selected(&line) {
                continue;
            }
            hits += 1;
            any = true;
            if quiet {
                break 'files;
            }
            if count || names_only {
                if names_only {
                    break;
                }
                continue;
            }
            let mut o = Vec::with_capacity(line.len() + 16);
            if with_names {
                o.extend_from_slice(label.as_bytes());
                o.push(b':');
            }
            if numbers {
                o.extend_from_slice(n.to_string().as_bytes());
                o.push(b':');
            }
            o.extend_from_slice(&line);
            o.push(b'\n');
            if !out.write(&o) {
                break 'files;
            }
        }
        if count {
            if with_names {
                out.text(&format!("{label}:{hits}\n"));
            } else {
                out.text(&format!("{hits}\n"));
            }
        } else if names_only && hits > 0 {
            out.text(&format!("{label}\n"));
        }
    }
    let status = if any {
        0
    } else if error {
        2
    } else {
        1
    };
    let status = if error && !quiet { 2 } else { status };
    out.finish(cx, name, status)
}

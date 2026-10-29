// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `xargs [-ptx] [-E eofstr] [-I replstr | -L number | -n number]
//! [-s size] [utility [argument...]]`: build and run command lines
//! from standard input.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::exec;
use crate::common::io::{Lines, confirm, stdin};
use crate::common::{complain, number, usage};

/// Read arguments: blanks and newlines separate, quotes and
/// backslashes protect. Returns each argument and whether it ended a
/// line.
fn words(rd: &mut Lines, eof: Option<&str>) -> Result<Vec<(String, bool)>, String> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut have = false;
    let mut quote: Option<u8> = None;
    loop {
        let b = rd.next_byte().map_err(|e| e.to_string())?;
        match (b, quote) {
            (None, Some(_)) => return Err("A quote is not closed.".to_string()),
            (None, None) => {
                if have {
                    out.push((String::from_utf8_lossy(&cur).into_owned(), true));
                }
                break;
            }
            (Some(b'\n'), Some(_)) => return Err("A quote is not closed.".to_string()),
            (Some(c), Some(q)) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            (Some(b'\\'), None) => {
                if let Some(n) = rd.next_byte().map_err(|e| e.to_string())? {
                    cur.push(n);
                    have = true;
                }
            }
            (Some(c @ (b'\'' | b'"')), None) => {
                quote = Some(c);
                have = true;
            }
            (Some(c), None) if c == b' ' || c == b'\t' || c == b'\n' => {
                if have {
                    out.push((String::from_utf8_lossy(&cur).into_owned(), c == b'\n'));
                    cur.clear();
                    have = false;
                } else if c == b'\n'
                    && let Some(last) = out.last_mut()
                {
                    last.1 = true;
                }
            }
            (Some(c), None) => {
                cur.push(c);
                have = true;
            }
        }
    }
    if let Some(e) = eof
        && let Some(i) = out.iter().position(|(w, _)| w == e)
    {
        out.truncate(i);
    }
    Ok(out)
}

/// Run `xargs`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "xargs";
    let p = match getopt(&cx.argv[1..], "ptxE:I:L:n:s:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let trace = p.has('t');
    let prompt = p.has('p');
    let exact = p.has('x');
    let eof = p.value('E');
    let replace = p.value('I').map(ToString::to_string);
    let per_lines = match p.value('L') {
        None => None,
        Some(s) => match number(s) {
            Some(n) if n > 0 => Some(n as usize),
            _ => return usage(cx, name, &format!("{s} is not a valid number.")),
        },
    };
    let per_args = match p.value('n') {
        None => None,
        Some(s) => match number(s) {
            Some(n) if n > 0 => Some(n as usize),
            _ => return usage(cx, name, &format!("{s} is not a valid number.")),
        },
    };
    let size = match p.value('s') {
        None => 4096,
        Some(s) => match number(s) {
            Some(n) if n > 0 => n as usize,
            _ => return usage(cx, name, &format!("{s} is not a valid size.")),
        },
    };
    let base: Vec<String> = if p.operands.is_empty() {
        vec!["echo".to_string()]
    } else {
        p.operands.clone()
    };
    let mut rd = Lines::new(stdin(cx));
    // Group into invocations.
    let mut groups: Vec<Vec<String>> = Vec::new();
    if replace.is_some() {
        // Insert mode: each line less its leading blanks is one argument.
        while let Ok(Some((l, _))) = rd.next_text() {
            let l = String::from_utf8_lossy(&l);
            let l = l.trim_start_matches([' ', '\t']);
            if eof == Some(l) {
                break;
            }
            if !l.is_empty() {
                groups.push(vec![l.to_string()]);
            }
        }
    }
    let items = if replace.is_some() {
        Vec::new()
    } else {
        match words(&mut rd, eof) {
            Ok(w) => w,
            Err(e) => {
                complain(cx, name, &e);
                return 1;
            }
        }
    };
    if replace.is_some() {
        // Grouped above.
    } else if let Some(n) = per_lines {
        let mut cur: Vec<String> = Vec::new();
        let mut lines = 0;
        for (w, end) in &items {
            cur.push(w.clone());
            if *end {
                lines += 1;
                if lines == n {
                    groups.push(std::mem::take(&mut cur));
                    lines = 0;
                }
            }
        }
        if !cur.is_empty() {
            groups.push(cur);
        }
    } else {
        let base_len: usize = base.iter().map(|a| a.len() + 1).sum();
        let mut cur: Vec<String> = Vec::new();
        let mut len = base_len;
        for (w, _) in &items {
            let full = per_args.is_some_and(|n| cur.len() >= n)
                || (!cur.is_empty() && len + w.len() + 1 > size);
            if full {
                groups.push(std::mem::take(&mut cur));
                len = base_len;
            }
            if base_len + w.len() + 1 > size {
                complain(cx, name, "An argument is longer than the command line size.");
                return 1;
            }
            len += w.len() + 1;
            cur.push(w.clone());
        }
        if !cur.is_empty() || items.is_empty() {
            groups.push(cur);
        }
        if exact && per_args.is_some_and(|n| groups.last().is_some_and(|g| g.len() < n)) && groups.len() > 1 {
            complain(cx, name, "The arguments do not fit the command line size.");
            return 1;
        }
    }
    let mut status = 0;
    for g in groups {
        let argv: Vec<String> = match &replace {
            Some(r) => {
                let line = g.first().cloned().unwrap_or_default();
                base.iter().map(|a| a.replace(r.as_str(), &line)).collect()
            }
            None => base.iter().cloned().chain(g).collect(),
        };
        if trace || prompt {
            let _ = cx.err(&format!("{}{}", argv.join(" "), if prompt { "?..." } else { "\n" }));
            if prompt && !confirm(cx, "") {
                continue;
            }
        }
        match exec::run(cx, &argv, None, None) {
            Ok(0) => {}
            Ok(255) => {
                complain(cx, name, &format!("{}: ended with status 255.", argv[0]));
                return 124;
            }
            Ok(n) if n > 128 => return 125,
            Ok(_) => status = 123,
            Err(e) => {
                complain(cx, name, &format!("{}: {}", argv[0], e.text()));
                return e.status();
            }
        }
    }
    status
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `echo`, `print`, `read`.

use super::{fail, opts_or_fail};
use crate::expand::{Field, Q, S};
use crate::msg::Msg;
use crate::shell::{Res, Shell};

/// Expand `echo`-style backslash escapes into `out`. Returns false at
/// `\c` (stop all output).
pub(crate) fn echo_escapes(s: &str, out: &mut String) -> bool {
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('a') => out.push('\x07'),
            Some('b') => out.push('\x08'),
            Some('c') => return false,
            Some('f') => out.push('\x0c'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\x0b'),
            Some('\\') => out.push('\\'),
            Some('0') => {
                let mut v = 0u32;
                for _ in 0..3 {
                    match it.peek().and_then(|d| d.to_digit(8)) {
                        Some(d) => {
                            v = v * 8 + d;
                            it.next();
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(v).unwrap_or('\0'));
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    true
}

/// `echo [-n] [arg...]`: arguments with escapes, a newline unless `-n`.
pub(crate) fn echo(sh: &mut Shell, args: &[String]) -> Res {
    let mut i = 1;
    let mut newline = true;
    while args.get(i).is_some_and(|a| a == "-n") {
        newline = false;
        i += 1;
    }
    let mut out = String::new();
    let mut go = true;
    for (k, a) in args[i..].iter().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        if !echo_escapes(a, &mut out) {
            go = false;
            break;
        }
    }
    if go && newline {
        out.push('\n');
    }
    sh.out(&out)?;
    Ok(0)
}

/// `print [-nrR] [-u n] [arg...]`.
pub(crate) fn print(sh: &mut Shell, args: &[String]) -> Res {
    let mut i = 1;
    let (mut newline, mut raw, mut fd) = (true, false, 1u32);
    while let Some(a) = args.get(i) {
        if a == "--" || a == "-" {
            i += 1;
            break;
        }
        if !a.starts_with('-') || a.len() < 2 {
            break;
        }
        let mut stop = false;
        let chars: Vec<char> = a[1..].chars().collect();
        let mut j = 0;
        while j < chars.len() {
            match chars[j] {
                'n' => newline = false,
                'r' => raw = true,
                'R' => {
                    raw = true;
                    stop = true;
                }
                'u' => {
                    let rest: String = chars[j + 1..].iter().collect();
                    let v = if rest.is_empty() {
                        i += 1;
                        args.get(i).cloned().unwrap_or_default()
                    } else {
                        rest
                    };
                    match v.parse() {
                        Ok(n) => fd = n,
                        Err(_) => return Err(fail(sh, &Msg::NotNumber("print".to_string(), v), 1)),
                    }
                    j = chars.len();
                    continue;
                }
                c => {
                    return Err(fail(
                        sh,
                        &Msg::Usage("print".to_string(), format!("option -{c} is not valid.")),
                        2,
                    ));
                }
            }
            j += 1;
        }
        i += 1;
        if stop {
            break;
        }
    }
    let mut out = String::new();
    let mut go = true;
    for (k, a) in args[i..].iter().enumerate() {
        if k > 0 {
            out.push(' ');
        }
        if raw {
            out.push_str(a);
        } else if !echo_escapes(a, &mut out) {
            go = false;
            break;
        }
    }
    if go && newline {
        out.push('\n');
    }
    if sh.fds.get(fd).is_none() {
        return Err(fail(sh, &Msg::BadFd(fd.to_string()), 1));
    }
    sh.write_fd(fd, out.as_bytes())?;
    Ok(0)
}

/// Read one line from a descriptor a byte at a time (so nothing past
/// the newline is consumed). `None` at end of file with nothing read.
fn read_line(sh: &Shell, fd: u32, raw: bool) -> (Field, bool) {
    let mut bytes: Vec<(u8, bool)> = Vec::new();
    let mut eof = false;
    let Some(f) = sh.fds.get(fd) else {
        return (Field::new(), true);
    };
    let mut b = [0u8; 1];
    let mut escaped = false;
    loop {
        match f.read(&mut b) {
            Ok(1) => {}
            _ => {
                eof = true;
                break;
            }
        }
        let c = b[0];
        if escaped {
            escaped = false;
            if c == b'\n' {
                continue;
            }
            bytes.push((c, true));
            continue;
        }
        if c == b'\\' && !raw {
            escaped = true;
            continue;
        }
        if c == b'\n' {
            break;
        }
        bytes.push((c, false));
    }
    // Decode as UTF-8, carrying each byte's quoting to its character.
    let raw_bytes: Vec<u8> = bytes.iter().map(|(b, _)| *b).collect();
    let text = String::from_utf8_lossy(&raw_bytes).into_owned();
    let mut field = Field::new();
    let mut bi = 0;
    for ch in text.chars() {
        let q = bytes.get(bi).is_some_and(|(_, q)| *q);
        bi += ch.len_utf8();
        field.push((ch, if q { Q } else { S }));
    }
    (field, eof)
}

/// `read [-r] [-p prompt] [-u n] [name...]`.
pub(crate) fn read(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "r", "pu", false)?;
    let fd = match o.value('u') {
        Some(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => {
                return Err(fail(
                    sh,
                    &Msg::NotNumber("read".to_string(), v.to_string()),
                    1,
                ));
            }
        },
        None => 0,
    };
    if let Some(p) = o.value('p') {
        let p = p.to_string();
        sh.err(&p);
    }
    let mut names = o.operands.clone();
    if names.is_empty() {
        names.push("REPLY".to_string());
    }
    for n in &names {
        if !crate::syntax::is_name(n) {
            return Err(fail(sh, &Msg::BadName("read".to_string(), n.clone()), 2));
        }
    }
    let (field, eof) = read_line(sh, fd, o.has('r'));
    let ifs = sh.var("IFS").unwrap_or_else(|| " \t\n".to_string());
    let values = split_read(&field, &ifs, names.len());
    for (i, n) in names.iter().enumerate() {
        let v = values.get(i).cloned().unwrap_or_default();
        if sh.vars.set(n, &v).is_err() {
            return Err(fail(sh, &Msg::ReadOnly(n.clone()), 1));
        }
    }
    Ok(i32::from(eof))
}

/// Split a line into at most `n` values, the last taking the rest.
fn split_read(field: &Field, ifs: &str, n: usize) -> Vec<String> {
    let is_ifs = |i: usize| field[i].1 & Q == 0 && ifs.contains(field[i].0);
    let is_ws = |i: usize| is_ifs(i) && matches!(field[i].0, ' ' | '\t' | '\n');
    let len = field.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < len && is_ws(i) {
        i += 1;
    }
    while out.len() + 1 < n && i < len {
        let mut cur = String::new();
        while i < len && !is_ifs(i) {
            cur.push(field[i].0);
            i += 1;
        }
        out.push(cur);
        while i < len && is_ws(i) {
            i += 1;
        }
        if i < len && is_ifs(i) {
            i += 1;
            while i < len && is_ws(i) {
                i += 1;
            }
        }
    }
    if i < len || out.len() < n {
        let mut end = len;
        while end > i && is_ws(end - 1) {
            end -= 1;
        }
        out.push(field[i.min(end)..end].iter().map(|(c, _)| *c).collect());
    }
    out
}

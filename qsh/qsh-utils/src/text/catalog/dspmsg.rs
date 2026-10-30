// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dspmsg [-s set] catalog msgnum ['default' [argument...]]`: write a
//! message from a catalog, or the default when the catalog or message
//! is missing. `%s` takes the next argument, `%n$s` the n-th, `%%` is
//! a percent sign; no newline is added. A catalog name without a slash
//! is found through `NLSPATH` (`%N` the name, `%L` `LANG`, `%l` `%t`
//! `%c` its parts, `%%`), then `/usr/lib/nls/msg/%L/%N`, then the
//! working directory.

use qsh::sys::UtilCtx;

use super::format::{Catalog, SET_DEFAULT};
use crate::common::args::getopt;
use crate::common::io::{Out, read_input};
use crate::common::{complain, usage};

const NAME: &str = "dspmsg";
const DEFAULT_PATH: &str = "/usr/lib/nls/msg/%L/%N:/usr/lib/nls/msg/%N";

/// Fill an `NLSPATH` template.
fn fill(tpl: &str, name: &str, lang: &str) -> String {
    let (lt, codeset) = lang.split_once('.').unwrap_or((lang, ""));
    let (language, territory) = lt.split_once('_').unwrap_or((lt, ""));
    let mut out = String::new();
    let mut it = tpl.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('N') => out.push_str(name),
            Some('L') => out.push_str(lang),
            Some('l') => out.push_str(language),
            Some('t') => out.push_str(territory),
            Some('c') => out.push_str(codeset),
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

/// The catalog's bytes, found by path or through `NLSPATH`.
fn find(cx: &UtilCtx<'_>, name: &str) -> Option<Vec<u8>> {
    if name.contains('/') {
        return read_input(cx, name).ok();
    }
    let lang = cx
        .getenv("LC_ALL")
        .filter(|v| !v.is_empty())
        .or_else(|| cx.getenv("LC_MESSAGES").filter(|v| !v.is_empty()))
        .or_else(|| cx.getenv("LANG").filter(|v| !v.is_empty()))
        .unwrap_or("C")
        .to_string();
    let mut paths = String::new();
    if let Some(p) = cx.getenv("NLSPATH") {
        paths.push_str(p);
        paths.push(':');
    }
    paths.push_str(DEFAULT_PATH);
    for tpl in paths.split(':').filter(|t| !t.is_empty()) {
        let p = fill(tpl, name, &lang);
        if let Ok(b) = read_input(cx, &p) {
            return Some(b);
        }
    }
    // Last, the name in the working directory.
    read_input(cx, name).ok()
}

/// Resolve `\n`, `\t`, `\\` in a default message.
fn escapes(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Substitute `%s`, `%n$s` and `%%`.
pub fn substitute(text: &[u8], args: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut next = 0;
    let mut i = 0;
    while i < text.len() {
        if text[i] != b'%' {
            out.push(text[i]);
            i += 1;
            continue;
        }
        let rest = &text[i + 1..];
        if rest.first() == Some(&b'%') {
            out.push(b'%');
            i += 2;
            continue;
        }
        if rest.first() == Some(&b's') {
            if let Some(a) = args.get(next) {
                out.extend_from_slice(a.as_bytes());
            }
            next += 1;
            i += 2;
            continue;
        }
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits > 0 && rest.get(digits) == Some(&b'$') && rest.get(digits + 1) == Some(&b's') {
            let n: usize = String::from_utf8_lossy(&rest[..digits]).parse().unwrap_or(0);
            if let Some(a) = n.checked_sub(1).and_then(|k| args.get(k)) {
                out.extend_from_slice(a.as_bytes());
            }
            i += digits + 3;
            continue;
        }
        out.push(b'%');
        i += 1;
    }
    out
}

/// Run `dspmsg`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "s:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let set = match p.value('s') {
        None => SET_DEFAULT,
        Some(v) => match v.parse::<u32>() {
            Ok(n) if n > 0 => n,
            _ => return usage(cx, NAME, &format!("{v} is not a valid set number.")),
        },
    };
    let ops = &p.operands;
    if ops.len() < 2 {
        return usage(cx, NAME, "A catalog and a message number are needed.");
    }
    let num: u32 = match ops[1].parse() {
        Ok(n) if n > 0 => n,
        _ => return usage(cx, NAME, &format!("{} is not a valid message number.", ops[1])),
    };
    let args = if ops.len() > 3 { &ops[3..] } else { &[][..] };
    let found = find(cx, &ops[0])
        .and_then(|b| Catalog::decode(&b))
        .and_then(|c| c.get(set, num).map(<[u8]>::to_vec));
    let text = match (found, ops.get(2)) {
        (Some(t), _) => t,
        (None, Some(d)) => escapes(d).into_bytes(),
        (None, None) => {
            complain(
                cx,
                NAME,
                &format!("Message {num} of set {set} is not in catalog {}.", ops[0]),
            );
            return 1;
        }
    };
    let mut out = Out::stdout(cx);
    out.write(&substitute(&text, args));
    out.finish(cx, NAME, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subst() {
        let a = vec!["x".to_string(), "y".to_string()];
        assert_eq!(substitute(b"%s-%s-%s", &a), b"x-y-");
        assert_eq!(substitute(b"%2$s %1$s 100%%", &a), b"y x 100%");
        assert_eq!(substitute(b"%d %", &a), b"%d %");
        assert_eq!(fill("/x/%L/%l/%t/%c/%N", "cat", "en_US.UTF-8"), "/x/en_US.UTF-8/en/US/UTF-8/cat");
    }
}

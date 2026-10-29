// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sort [-m] [-o output] [-bdfinru] [-t char] [-k keydef]... [file...]`,
//! `sort -c [-bdfinru] [-t char] [-k keydef]... [file]`: sort lines by
//! keys, byte order, with the last-resort comparison of whole lines.

use std::cmp::Ordering;

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Out, inputs, open_write, read_input, split_lines};
use crate::common::{complain, complain_sys, usage};

/// Comparison flags.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct Flags {
    b: bool,
    d: bool,
    f: bool,
    i: bool,
    n: bool,
    r: bool,
}

impl Flags {
    fn any(self) -> bool {
        self.d || self.f || self.i || self.n || self.r
    }

    fn set(&mut self, c: u8) -> bool {
        match c {
            b'b' => self.b = true,
            b'd' => self.d = true,
            b'f' => self.f = true,
            b'i' => self.i = true,
            b'n' => self.n = true,
            b'r' => self.r = true,
            _ => return false,
        }
        true
    }
}

/// One `-k` key: start field and character, end field and character
/// (0 is the end of the line), and flags.
#[derive(Clone, Copy, Debug)]
struct Key {
    f1: usize,
    c1: usize,
    b1: bool,
    f2: usize,
    c2: usize,
    b2: bool,
    flags: Flags,
}

fn parse_key(spec: &str, global: Flags) -> Option<Key> {
    let (start, end) = match spec.split_once(',') {
        Some((a, b)) => (a, Some(b)),
        None => (spec, None),
    };
    let part = |s: &str, is_start: bool| -> Option<(usize, usize, bool, Flags)> {
        let mut flags = Flags::default();
        let mut b = false;
        let digits_end = |s: &str| s.bytes().take_while(u8::is_ascii_digit).count();
        let n = digits_end(s);
        if n == 0 {
            return None;
        }
        let f: usize = s[..n].parse().ok()?;
        let mut rest = &s[n..];
        let mut c = usize::from(is_start);
        if let Some(r) = rest.strip_prefix('.') {
            let m = digits_end(r);
            if m == 0 {
                return None;
            }
            c = r[..m].parse().ok()?;
            rest = &r[m..];
        }
        for ch in rest.bytes() {
            if ch == b'b' {
                b = true;
            } else if !flags.set(ch) {
                return None;
            }
        }
        if f == 0 {
            return None;
        }
        Some((f, c, b, flags))
    };
    let (f1, c1, b1, fl1) = part(start, true)?;
    if c1 == 0 {
        return None;
    }
    let (f2, c2, b2, fl2) = match end {
        Some(e) => part(e, false)?,
        None => (0, 0, false, Flags::default()),
    };
    let mut flags = Flags::default();
    for fl in [fl1, fl2] {
        if fl.d {
            flags.d = true;
        }
        if fl.f {
            flags.f = true;
        }
        if fl.i {
            flags.i = true;
        }
        if fl.n {
            flags.n = true;
        }
        if fl.r {
            flags.r = true;
        }
    }
    let (b1, b2) = if global.b { (true, true) } else { (b1, b2) };
    if !flags.any() {
        flags = Flags {
            b: false,
            ..global
        };
    }
    Some(Key {
        f1,
        c1,
        b1,
        f2,
        c2,
        b2,
        flags,
    })
}

/// Field boundaries of a line: (start, end) of each field. Without a
/// separator a field includes the blanks before it.
fn fields(line: &[u8], sep: Option<u8>) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    match sep {
        Some(s) => {
            let mut start = 0;
            for (i, &b) in line.iter().enumerate() {
                if b == s {
                    out.push((start, i));
                    start = i + 1;
                }
            }
            out.push((start, line.len()));
        }
        None => {
            let mut i = 0;
            while i < line.len() {
                let start = i;
                while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
                    i += 1;
                }
                while i < line.len() && line[i] != b' ' && line[i] != b'\t' {
                    i += 1;
                }
                out.push((start, i));
            }
            if out.is_empty() {
                out.push((0, 0));
            }
        }
    }
    out
}

/// The bytes a key selects in a line.
fn extract<'a>(line: &'a [u8], key: &Key, sep: Option<u8>) -> &'a [u8] {
    let fl = fields(line, sep);
    let skip_blanks = |mut p: usize, end: usize| {
        while p < end && (line[p] == b' ' || line[p] == b'\t') {
            p += 1;
        }
        p
    };
    let start = match fl.get(key.f1 - 1) {
        None => return &[],
        Some(&(s, e)) => {
            let s = if key.b1 { skip_blanks(s, e) } else { s };
            (s + key.c1 - 1).min(line.len())
        }
    };
    let end = if key.f2 == 0 {
        line.len()
    } else {
        match fl.get(key.f2 - 1) {
            None => line.len(),
            Some(&(s, e)) => {
                if key.c2 == 0 {
                    e
                } else {
                    let s = if key.b2 { skip_blanks(s, e) } else { s };
                    (s + key.c2).min(line.len())
                }
            }
        }
    };
    if end <= start {
        return &[];
    }
    &line[start..end]
}

/// Compare numerically: leading blanks, an optional sign, digits with
/// one decimal point. What is not a number is zero.
fn numeric(a: &[u8], b: &[u8]) -> Ordering {
    fn parse(s: &[u8]) -> (bool, &[u8], &[u8]) {
        let mut i = 0;
        while i < s.len() && (s[i] == b' ' || s[i] == b'\t') {
            i += 1;
        }
        let neg = s.get(i) == Some(&b'-');
        if neg || s.get(i) == Some(&b'+') {
            i += 1;
        }
        let is = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        let int = &s[is..i];
        let mut frac: &[u8] = &[];
        if s.get(i) == Some(&b'.') {
            i += 1;
            let fs = i;
            while i < s.len() && s[i].is_ascii_digit() {
                i += 1;
            }
            frac = &s[fs..i];
        }
        let int = {
            let mut k = 0;
            while k < int.len() && int[k] == b'0' {
                k += 1;
            }
            &int[k..]
        };
        let frac = {
            let mut k = frac.len();
            while k > 0 && frac[k - 1] == b'0' {
                k -= 1;
            }
            &frac[..k]
        };
        let zero = int.is_empty() && frac.is_empty();
        (neg && !zero, int, frac)
    }
    let (na, ia, fa) = parse(a);
    let (nb, ib, fb) = parse(b);
    let mag = ia
        .len()
        .cmp(&ib.len())
        .then_with(|| ia.cmp(ib))
        .then_with(|| fa.cmp(fb));
    match (na, nb) {
        (false, true) => Ordering::Greater,
        (true, false) => Ordering::Less,
        (true, true) => mag.reverse(),
        (false, false) => mag,
    }
}

fn transform(s: &[u8], fl: Flags) -> Vec<u8> {
    s.iter()
        .filter(|&&b| !fl.d || b.is_ascii_alphanumeric() || b == b' ' || b == b'\t')
        .filter(|&&b| !fl.i || (0x20..0x7f).contains(&b))
        .map(|&b| if fl.f { b.to_ascii_uppercase() } else { b })
        .collect()
}

fn compare_with(a: &[u8], b: &[u8], fl: Flags) -> Ordering {
    let o = if fl.n {
        numeric(a, b)
    } else if fl.d || fl.f || fl.i {
        transform(a, fl).cmp(&transform(b, fl))
    } else {
        a.cmp(b)
    };
    if fl.r { o.reverse() } else { o }
}

struct Sorter {
    keys: Vec<Key>,
    global: Flags,
    sep: Option<u8>,
    unique: bool,
}

impl Sorter {
    fn key_cmp(&self, a: &[u8], b: &[u8]) -> Ordering {
        if self.keys.is_empty() {
            let (ka, kb) = if self.global.b {
                (
                    a.trim_ascii_start(),
                    b.trim_ascii_start(),
                )
            } else {
                (a, b)
            };
            return compare_with(ka, kb, self.global);
        }
        for k in &self.keys {
            let o = compare_with(extract(a, k, self.sep), extract(b, k, self.sep), k.flags);
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    }

    fn cmp(&self, a: &[u8], b: &[u8]) -> Ordering {
        let o = self.key_cmp(a, b);
        if o != Ordering::Equal || self.unique {
            return o;
        }
        let o = a.cmp(b);
        if self.global.r { o.reverse() } else { o }
    }
}

/// Run `sort`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "sort";
    let p = match getopt(&cx.argv[1..], "cmo:ubdfinrt:k:sz") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut global = Flags::default();
    for c in "bdfinr".bytes() {
        if p.has(c as char) {
            global.set(c);
        }
    }
    let sep = match p.value('t') {
        None => None,
        Some(t) if t.len() == 1 => Some(t.as_bytes()[0]),
        Some(_) => return usage(cx, name, "The separator must be one character."),
    };
    let mut keys = Vec::new();
    for k in p.values('k') {
        match parse_key(k, global) {
            Some(key) => keys.push(key),
            None => return usage(cx, name, &format!("The key {k} is not valid.")),
        }
    }
    let sorter = Sorter {
        keys,
        global,
        sep,
        unique: p.has('u'),
    };
    let check = p.has('c');
    let files = inputs(&p.operands);
    let mut lines: Vec<Vec<u8>> = Vec::new();
    for f in &files {
        match read_input(cx, f) {
            Ok(d) => {
                for l in split_lines(&d) {
                    lines.push(l.strip_suffix(b"\n").unwrap_or(l).to_vec());
                }
            }
            Err(e) => {
                complain_sys(cx, name, f, e);
                return 2;
            }
        }
    }
    if check {
        for (i, w) in lines.windows(2).enumerate() {
            let o = sorter.cmp(&w[0], &w[1]);
            if o == Ordering::Greater || (sorter.unique && o == Ordering::Equal) {
                complain(
                    cx,
                    name,
                    &format!(
                        "{}: line {} is out of order: {}",
                        files[0],
                        i + 2,
                        String::from_utf8_lossy(&w[1])
                    ),
                );
                return 1;
            }
        }
        return 0;
    }
    lines.sort_by(|a, b| sorter.cmp(a, b));
    if sorter.unique {
        lines.dedup_by(|a, b| sorter.key_cmp(a, b) == Ordering::Equal);
    }
    let mut out = match p.value('o') {
        None => Out::stdout(cx),
        Some(o) => match open_write(cx, o, false) {
            Ok(fd) => Out::on(Some(fd)),
            Err(e) => {
                complain_sys(cx, name, o, e);
                return 2;
            }
        },
    };
    for l in &lines {
        if !out.write(l) || !out.write(b"\n") {
            break;
        }
    }
    out.finish(cx, name, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_order() {
        assert_eq!(numeric(b"10", b"9"), Ordering::Greater);
        assert_eq!(numeric(b"-1", b"0"), Ordering::Less);
        assert_eq!(numeric(b"-10", b"-9"), Ordering::Less);
        assert_eq!(numeric(b"1.5", b"1.25"), Ordering::Greater);
        assert_eq!(numeric(b"abc", b"0"), Ordering::Equal);
        assert_eq!(numeric(b"  007", b"7"), Ordering::Equal);
        assert_eq!(numeric(b"-0", b"0"), Ordering::Equal);
    }

    #[test]
    fn key_extraction() {
        let k = parse_key("2,2", Flags::default()).unwrap();
        assert_eq!(extract(b"a bb c", &k, None), b" bb");
        let k = parse_key("2b,2", Flags::default()).unwrap();
        assert_eq!(extract(b"a bb c", &k, None), b"bb");
        let k = parse_key("2.2", Flags::default()).unwrap();
        assert_eq!(extract(b"a bb c", &k, None), b"bb c");
        let k = parse_key("2,3.1", Flags::default()).unwrap();
        assert_eq!(extract(b"a:bb:cc", &k, Some(b':')), b"bb:c");
        let k = parse_key("3", Flags::default()).unwrap();
        assert_eq!(extract(b"a:b", &k, Some(b':')), b"");
        assert!(parse_key("0", Flags::default()).is_none());
        assert!(parse_key("1.0", Flags::default()).is_none());
    }
}

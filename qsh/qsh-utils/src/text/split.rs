// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `split [-l n | -b n[k|m]] [-a n] [file [name]]`: pieces of a file
//! named `xaa`, `xab`, ... (or `name` plus the suffix).

use qsh::sys::stream::write_all;
use qsh::sys::UtilCtx;

use crate::common::args::getopt_from;
use crate::common::io::{Lines, input, open_write};
use crate::common::{complain, complain_sys, number, usage};

fn suffix(n: usize, len: usize) -> Option<String> {
    let mut s = vec![b'a'; len];
    let mut k = n;
    for i in (0..len).rev() {
        s[i] = b'a' + (k % 26) as u8;
        k /= 26;
    }
    if k > 0 {
        return None;
    }
    String::from_utf8(s).ok()
}

/// Run `split`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "split";
    let p = match getopt_from(&cx.argv[1..], "l:b:a:", &|a| {
        a.len() > 1 && a.starts_with('-') && a[1..].bytes().all(|b| b.is_ascii_digit())
    }) {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut lines: u64 = 1000;
    let mut bytes: Option<u64> = None;
    let mut sfx = 2usize;
    for (o, v) in &p.opts {
        let v = v.as_deref().unwrap_or("");
        match o {
            '\u{0}' => lines = v[1..].parse().unwrap_or(1000),
            'l' => match number(v) {
                Some(n) if n > 0 => {
                    lines = n;
                    bytes = None;
                }
                _ => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            'b' => {
                let (digits, mult) = match v.as_bytes().last() {
                    Some(b'k') => (&v[..v.len() - 1], 1024),
                    Some(b'm') => (&v[..v.len() - 1], 1024 * 1024),
                    _ => (v, 1),
                };
                match number(digits) {
                    Some(n) if n > 0 => bytes = Some(n * mult),
                    _ => return usage(cx, name, &format!("{v} is not a valid size.")),
                }
            }
            'a' => match number(v) {
                Some(n) if n > 0 => sfx = n as usize,
                _ => return usage(cx, name, &format!("{v} is not a valid number.")),
            },
            _ => {}
        }
    }
    if p.operands.len() > 2 {
        return usage(cx, name, "Too many operands.");
    }
    let src = p.operands.first().map_or("-", String::as_str);
    let prefix = p.operands.get(1).map_or("x", String::as_str);
    let fd = match input(cx, src) {
        Ok(fd) => fd,
        Err(e) => {
            complain_sys(cx, name, src, e);
            return 1;
        }
    };
    let mut rd = Lines::new(fd);
    let mut piece = 0usize;
    loop {
        let mut chunk = Vec::new();
        match bytes {
            Some(n) => {
                while (chunk.len() as u64) < n {
                    match rd.next_byte() {
                        Ok(Some(b)) => chunk.push(b),
                        Ok(None) => break,
                        Err(e) => {
                            complain_sys(cx, name, src, e.into());
                            return 1;
                        }
                    }
                }
            }
            None => {
                for _ in 0..lines {
                    match rd.next_line() {
                        Ok(Some(l)) => chunk.extend(l),
                        Ok(None) => break,
                        Err(e) => {
                            complain_sys(cx, name, src, e.into());
                            return 1;
                        }
                    }
                }
            }
        }
        if chunk.is_empty() {
            break;
        }
        let Some(s) = suffix(piece, sfx) else {
            complain(cx, name, "The suffix length is too short for the pieces.");
            return 1;
        };
        let path = format!("{prefix}{s}");
        match open_write(cx, &path, false) {
            Ok(fd) => {
                if let Err(e) = write_all(&*fd, &chunk) {
                    complain_sys(cx, name, &path, e.into());
                    return 1;
                }
            }
            Err(e) => {
                complain_sys(cx, name, &path, e);
                return 1;
            }
        }
        piece += 1;
    }
    0
}

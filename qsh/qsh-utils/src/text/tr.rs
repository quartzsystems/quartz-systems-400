// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tr [-c | -C] [-s] string1 string2`, `tr -s [-c] string1`,
//! `tr -d [-c] string1`, `tr -ds [-c] string1 string2`: translate,
//! squeeze or delete bytes. Strings take escapes, ranges, classes,
//! equivalence classes and `[x*n]` repeats.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::{Out, stdin};
use crate::common::usage;

/// One element of an expanded string.
#[derive(Clone, Copy)]
enum Item {
    Byte(u8),
    /// `[x*]`: repeat to fill.
    Fill(u8),
}

fn class_bytes(name: &str) -> Option<Vec<u8>> {
    let f: fn(u8) -> bool = match name {
        "alpha" => |b| b.is_ascii_alphabetic(),
        "digit" => |b| b.is_ascii_digit(),
        "alnum" => |b| b.is_ascii_alphanumeric(),
        "upper" => |b| b.is_ascii_uppercase(),
        "lower" => |b| b.is_ascii_lowercase(),
        "space" => |b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c),
        "blank" => |b| b == b' ' || b == b'\t',
        "punct" => |b| b.is_ascii_punctuation(),
        "print" => |b| (0x20..0x7f).contains(&b),
        "graph" => |b| (0x21..0x7f).contains(&b),
        "cntrl" => |b| b < 0x20 || b == 0x7f,
        "xdigit" => |b| b.is_ascii_hexdigit(),
        _ => return None,
    };
    Some((0..=255u8).filter(|&b| f(b)).collect())
}

/// Expand a `tr` string. `second` allows `[x*n]`.
fn expand(s: &[u8], second: bool) -> Result<Vec<Item>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    let next_byte = |i: &mut usize| -> Option<u8> {
        if *i >= s.len() {
            return None;
        }
        let b = s[*i];
        *i += 1;
        if b != b'\\' || *i >= s.len() {
            return Some(b);
        }
        let e = s[*i];
        *i += 1;
        Some(match e {
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'0'..=b'7' => {
                let mut v = u32::from(e - b'0');
                let mut n = 1;
                while n < 3 && *i < s.len() && (b'0'..=b'7').contains(&s[*i]) {
                    v = v * 8 + u32::from(s[*i] - b'0');
                    *i += 1;
                    n += 1;
                }
                (v & 0xff) as u8
            }
            other => other,
        })
    };
    while i < s.len() {
        if s[i] == b'['
            && let Some(end) = s[i + 1..].iter().position(|&b| b == b']') {
                let inner = &s[i + 1..i + 1 + end];
                if let Some(name) = inner.strip_prefix(b":").and_then(|x| x.strip_suffix(b":")) {
                    let name = String::from_utf8_lossy(name).into_owned();
                    let bytes =
                        class_bytes(&name).ok_or(format!("The class {name} is not valid."))?;
                    out.extend(bytes.into_iter().map(Item::Byte));
                    i += end + 2;
                    continue;
                }
                if let Some(eq) = inner.strip_prefix(b"=").and_then(|x| x.strip_suffix(b"=")) {
                    if eq.len() == 1 {
                        out.push(Item::Byte(eq[0]));
                        i += end + 2;
                        continue;
                    }
                    return Err("The equivalence class is not valid.".to_string());
                }
                if let Some(star) = inner.iter().position(|&b| b == b'*')
                    && star >= 1
                {
                    let mut j = i + 1;
                    let c = next_byte(&mut j).unwrap_or(b'[');
                    if j == i + 1 + star {
                        let count = &inner[star + 1..];
                        if !second {
                            return Err("A repeat is only valid in the second string.".to_string());
                        }
                        if count.is_empty() {
                            out.push(Item::Fill(c));
                        } else {
                            let txt = String::from_utf8_lossy(count);
                            let n = if txt.starts_with('0') && txt.len() > 1 {
                                usize::from_str_radix(&txt[1..], 8).ok()
                            } else {
                                txt.parse().ok()
                            }
                            .ok_or("The repeat count is not valid.".to_string())?;
                            out.extend(std::iter::repeat_n(Item::Byte(c), n));
                        }
                        i += end + 2;
                        continue;
                    }
                }
            }
        let Some(b) = next_byte(&mut i) else {
            break;
        };
        if i + 1 < s.len() && s[i] == b'-' {
            let mut j = i + 1;
            if let Some(hi) = next_byte(&mut j) {
                if hi < b {
                    return Err("The range is not in ascending order.".to_string());
                }
                out.extend((b..=hi).map(Item::Byte));
                i = j;
                continue;
            }
        }
        out.push(Item::Byte(b));
    }
    Ok(out)
}

/// Run `tr`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "tr";
    let p = match getopt(&cx.argv[1..], "cCds") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let complement = p.has('c') || p.has('C');
    let delete = p.has('d');
    let squeeze = p.has('s');
    let want = if delete && !squeeze { 1 } else if delete || !squeeze { 2 } else { 1 };
    let strings = &p.operands;
    if strings.len() < want || strings.len() > 2 || (delete && !squeeze && strings.len() != 1) {
        return usage(cx, name, "The number of strings is not right.");
    }
    let s1 = match expand(strings[0].as_bytes(), false) {
        Ok(v) => v,
        Err(e) => return usage(cx, name, &e),
    };
    let mut set1 = [false; 256];
    for it in &s1 {
        if let Item::Byte(b) = it {
            set1[*b as usize] = true;
        }
    }
    if complement {
        for v in &mut set1 {
            *v = !*v;
        }
    }
    let s1_bytes: Vec<u8> = if complement {
        (0..=255u8).filter(|&b| set1[b as usize]).collect()
    } else {
        s1.iter()
            .filter_map(|i| match i {
                Item::Byte(b) => Some(*b),
                Item::Fill(_) => None,
            })
            .collect()
    };
    // Translation table and the squeeze set.
    let mut map: [u8; 256] = [0; 256];
    for (i, m) in map.iter_mut().enumerate() {
        *m = i as u8;
    }
    let mut squeeze_set = [false; 256];
    let translating = !delete && strings.len() == 2;
    if translating {
        let s2 = match expand(strings[1].as_bytes(), true) {
            Ok(v) => v,
            Err(e) => return usage(cx, name, &e),
        };
        let mut s2_bytes: Vec<u8> = Vec::new();
        let fills = s2.iter().filter(|i| matches!(i, Item::Fill(_))).count();
        let fixed = s2.len() - fills;
        for it in &s2 {
            match it {
                Item::Byte(b) => s2_bytes.push(*b),
                Item::Fill(b) => {
                    let n = s1_bytes.len().saturating_sub(fixed) / fills.max(1);
                    s2_bytes.extend(std::iter::repeat_n(*b, n));
                }
            }
        }
        if s2_bytes.is_empty() {
            return usage(cx, name, "The second string is empty.");
        }
        for (k, &from) in s1_bytes.iter().enumerate() {
            let to = s2_bytes.get(k).copied().unwrap_or(*s2_bytes.last().unwrap_or(&from));
            map[from as usize] = to;
        }
        if squeeze {
            for &b in &s2_bytes {
                squeeze_set[b as usize] = true;
            }
        }
    } else if squeeze && !delete {
        squeeze_set = set1;
    } else if delete && squeeze {
        let s2 = match expand(strings[1].as_bytes(), false) {
            Ok(v) => v,
            Err(e) => return usage(cx, name, &e),
        };
        for it in &s2 {
            if let Item::Byte(b) = it {
                squeeze_set[*b as usize] = true;
            }
        }
    }
    let input = stdin(cx);
    let mut out = Out::stdout(cx);
    let mut buf = [0u8; 16384];
    let mut last: Option<u8> = None;
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let mut o = Vec::with_capacity(n);
        for &b in &buf[..n] {
            if delete && set1[b as usize] {
                continue;
            }
            let c = if translating { map[b as usize] } else { b };
            if squeeze_set[c as usize] && last == Some(c) {
                continue;
            }
            last = Some(c);
            o.push(c);
        }
        if !out.write(&o) {
            break;
        }
    }
    out.finish(cx, name, 0)
}

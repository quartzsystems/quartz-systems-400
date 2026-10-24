// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fixed-form string operations: `CAT`, `SCAN`, `CHECK`, `CHECKR`,
//! `XLATE`, `SUBST`, `TESTN`, `TESTZ` and the bit operations. Character
//! operands come as their images (`mov::image`); a result is written
//! left-aligned, its other positions kept unless padded.

use qdecimal::Decimal;

use crate::mov::image;
use crate::value::{Desc, Kind, bytes, write_num};

const BLANK: u8 = 0x40;

/// Put text into a character target: left-aligned, the rest kept or
/// blanked; a varying target as long as its current value.
pub fn put_text(dst: u64, dd: &Desc, text: &[u8], pad: bool) {
    let out = bytes(dst, dd.size());
    if dd.kind == Kind::VarChar {
        // A varying target keeps its current length.
        let p = dd.prefix();
        let cur = crate::value::var_len(out, p).min(dd.len as usize);
        let n = text.len().min(cur);
        out[p..p + n].copy_from_slice(&text[..n]);
        if pad {
            out[p + n..p + cur].fill(BLANK);
        }
    } else {
        let n = text.len().min(out.len());
        out[..n].copy_from_slice(&text[..n]);
        if pad {
            out[n..].fill(BLANK);
        }
    }
}

/// `CAT`: `a` (its trailing blanks dropped and `blanks` blanks added
/// when `blanks` is not negative), then `b`.
pub fn cat(pad: bool, blanks: i64, dst: u64, dd: &Desc, a: u64, ad: &Desc, b: u64, bd: &Desc) -> Result<(), mi::Cond> {
    let mut text = image(a, ad)?;
    if blanks >= 0 {
        while text.last() == Some(&BLANK) {
            text.pop();
        }
        text.extend(std::iter::repeat_n(BLANK, usize::try_from(blanks).unwrap_or(0)));
    }
    text.extend(image(b, bd)?);
    put_text(dst, dd, &text, pad);
    Ok(())
}

/// Write positions into a numeric result: one, or an array of `count`
/// elements (zeros after the last found).
fn put_positions(res: u64, rd: &Desc, count: usize, found: &[usize]) -> Result<(), mi::Cond> {
    let size = rd.size() as u64;
    for i in 0..count {
        let v = found.get(i).copied().unwrap_or(0);
        write_num(res + i as u64 * size, rd, &Decimal::new(v as i128, 0).unwrap_or_else(|_| Decimal::zero()))?;
    }
    Ok(())
}

/// `SCAN`: where `needle` (its first `nlen` bytes, all when negative)
/// is in `hay` from `start`; every position into an array result.
/// Whether it was found.
pub fn scan(needle: &[u8], hay: &[u8], start: usize, res: u64, rd: &Desc, count: usize) -> Result<bool, mi::Cond> {
    let mut found = Vec::new();
    if !needle.is_empty() && start >= 1 {
        let mut i = start - 1;
        while i + needle.len() <= hay.len() {
            if &hay[i..i + needle.len()] == needle {
                found.push(i + 1);
                if found.len() >= count.max(1) {
                    break;
                }
            }
            i += 1;
        }
    }
    if count > 0 {
        put_positions(res, rd, count, &found)?;
    }
    Ok(!found.is_empty())
}

/// `CHECK` and `CHECKR`: the positions of the bytes of `src` not in
/// `set`, from `start` rightwards (`CHECK`) or leftwards (`CHECKR`).
pub fn check(right: bool, set: &[u8], src: &[u8], start: usize, res: u64, rd: &Desc, count: usize) -> Result<bool, mi::Cond> {
    let mut found = Vec::new();
    let want = count.max(1);
    if right {
        let from = start.min(src.len());
        for i in (0..from).rev() {
            if !set.contains(&src[i]) {
                found.push(i + 1);
                if found.len() >= want {
                    break;
                }
            }
        }
    } else if start >= 1 {
        for (i, b) in src.iter().enumerate().skip(start - 1) {
            if !set.contains(b) {
                found.push(i + 1);
                if found.len() >= want {
                    break;
                }
            }
        }
    }
    if count > 0 {
        put_positions(res, rd, count, &found)?;
    }
    Ok(!found.is_empty())
}

/// `XLATE`: `src` with each byte of `from` (from position `start`)
/// replaced by the byte of `to` at the same place.
#[must_use]
pub fn xlate(from: &[u8], to: &[u8], src: &[u8], start: usize) -> Vec<u8> {
    src.iter()
        .enumerate()
        .map(|(i, b)| {
            if i + 1 < start {
                return *b;
            }
            match from.iter().position(|f| f == b) {
                Some(k) if k < to.len() => to[k],
                _ => *b,
            }
        })
        .collect()
}

/// `TESTN`: 1 all digits (the last may carry a sign zone), 2 leading
/// blanks then digits, 3 all blanks, 0 otherwise.
#[must_use]
pub fn testn(src: &[u8]) -> i32 {
    if src.iter().all(|b| *b == BLANK) {
        return 3;
    }
    let lead = src.iter().take_while(|b| **b == BLANK).count();
    let rest = &src[lead..];
    let digits = rest.iter().enumerate().all(|(i, b)| {
        let d = b & 0x0F <= 9;
        let z = b >> 4;
        d && (z == 0xF || i + 1 == rest.len() && matches!(z, 0xC | 0xD))
    });
    match (digits, lead) {
        (true, 0) => 1,
        (true, _) => 2,
        _ => 0,
    }
}

/// `TESTZ`: the zone of the first byte: 1 plus (`&`, `A`-`I`, `{`),
/// 2 minus (`-`, `J`-`R`, `}`), 3 any other.
#[must_use]
pub fn testz(src: &[u8]) -> i32 {
    match src.first() {
        Some(0x50) => 1,
        Some(0x60) => 2,
        Some(b) if b >> 4 == 0xC => 1,
        Some(b) if b >> 4 == 0xD => 2,
        _ => 3,
    }
}

/// The bit operations on one byte: `BITON` 0, `BITOFF` 1, `TESTB` 2.
/// `TESTB` returns 1 when every bit tested is off, 2 when some are on
/// and some off, 3 when all are on, 0 when no bit is tested.
pub fn bits(op: i32, byte: &mut u8, mask: u8) -> i32 {
    match op {
        0 => {
            *byte |= mask;
            0
        }
        1 => {
            *byte &= !mask;
            0
        }
        _ => {
            if mask == 0 {
                0
            } else if *byte & mask == 0 {
                1
            } else if *byte & mask == mask {
                3
            } else {
                2
            }
        }
    }
}

/// The bytes of `src` in `[start, start + len)` (1-based, `len`
/// negative to the end), or `None` when the range is outside it.
fn window(src: &[u8], start: i64, len: i64) -> Option<(usize, usize)> {
    let from = usize::try_from(start).ok()?.checked_sub(1)?;
    if from > src.len() {
        return None;
    }
    let to = if len < 0 { src.len() } else { from + usize::try_from(len).ok()? };
    (to <= src.len()).then_some((from, to))
}

/// `%SCANR`: the last position of `needle` in the window; 0 when
/// absent, `None` for a window outside the string.
#[must_use]
pub fn scanr(needle: &[u8], hay: &[u8], start: i64, len: i64) -> Option<usize> {
    let (from, to) = window(hay, start, len)?;
    if needle.is_empty() || to - from < needle.len() {
        return Some(0);
    }
    Some((from..=to - needle.len()).rev().find(|&i| &hay[i..i + needle.len()] == needle).map_or(0, |i| i + 1))
}

/// `%SCANRPL`: every `scan` in the window replaced by `repl`.
#[must_use]
pub fn scanrpl(scan: &[u8], repl: &[u8], src: &[u8], start: i64, len: i64) -> Option<Vec<u8>> {
    let (from, to) = window(src, start, len)?;
    let mut out = src[..from].to_vec();
    let mut i = from;
    while i < to {
        if !scan.is_empty() && i + scan.len() <= to && &src[i..i + scan.len()] == scan {
            out.extend_from_slice(repl);
            i += scan.len();
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out.extend_from_slice(&src[to..]);
    Some(out)
}

/// `%SPLIT`: the pieces of `src` between bytes of `seps`, empty pieces
/// left out.
#[must_use]
pub fn split(src: &[u8], seps: &[u8]) -> Vec<Vec<u8>> {
    src.split(|b| seps.contains(b)).filter(|p| !p.is_empty()).map(<[u8]>::to_vec).collect()
}

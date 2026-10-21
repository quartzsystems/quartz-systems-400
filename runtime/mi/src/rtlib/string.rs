// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text storage as code-unit strings, and the string instructions.
//! Narrow text is CP037 bytes, wide text is UCS-2; both are handled
//! as `u16` units so one implementation serves `char`, `ucs2` and
//! `graphic`, with conversion at a destination of the other width.

use std::cmp::Ordering;

use codegen::Cond;
use codegen::abi::Env;
use qir::Type;

use super::core::raise_on;
use super::mem::{job, typed};

/// A text value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text {
    /// Code units: bytes for narrow text, UCS-2 units for wide.
    pub units: Vec<u16>,
    /// Wide (UCS-2) rather than CP037.
    pub wide: bool,
}

fn blank(wide: bool) -> u16 {
    if wide { 0x20 } else { 0x40 }
}

fn is_wide(t: &Type) -> bool {
    matches!(t, Type::Ucs2(_) | Type::VarUcs2(_) | Type::Graphic(_))
}

/// Read a text operand. Dates, times, timestamps and indicators read
/// as narrow text; anything else reads as its raw bytes.
pub(crate) fn read_text(addr: i64, desc: i64) -> Text {
    let (b, t) = typed(addr, desc);
    match &t {
        Type::VarChar(_) => {
            let n = usize::from(u16::from_be_bytes([b[0], b[1]])).min(b.len() - 2);
            Text {
                units: b[2..2 + n].iter().map(|x| u16::from(*x)).collect(),
                wide: false,
            }
        }
        Type::Ucs2(_) | Type::Graphic(_) => Text {
            units: b
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect(),
            wide: true,
        },
        Type::VarUcs2(_) => {
            let n = usize::from(u16::from_be_bytes([b[0], b[1]])).min((b.len() - 2) / 2);
            Text {
                units: b[2..2 + 2 * n]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect(),
                wide: true,
            }
        }
        _ => Text {
            units: b.iter().map(|x| u16::from(*x)).collect(),
            wide: false,
        },
    }
}

/// Units converted to the width of a destination.
fn to_width(src: &Text, wide: bool) -> Vec<u16> {
    if src.wide == wide {
        return src.units.clone();
    }
    if wide {
        src.units
            .iter()
            .map(|u| {
                let c = ccsid::char_of(u8::try_from(*u).unwrap_or(0x6F));
                u16::try_from(u32::from(c)).unwrap_or(0x3F)
            })
            .collect()
    } else {
        src.units
            .iter()
            .map(|u| {
                char::from_u32(u32::from(*u))
                    .and_then(ccsid::byte_of)
                    .map_or(0x6F, u16::from)
            })
            .collect()
    }
}

/// Write text to an operand: fixed types padded with blanks or
/// truncated, varying types taking the length that fits.
pub(crate) fn write_text(addr: i64, desc: i64, text: &Text) {
    let (b, t) = typed(addr, desc);
    let wide = is_wide(&t);
    let units = to_width(text, wide);
    match &t {
        Type::VarChar(n) => {
            let len = units.len().min(*n as usize);
            b[..2].copy_from_slice(&u16::try_from(len).expect("fits").to_be_bytes());
            for (i, u) in units.iter().take(len).enumerate() {
                b[2 + i] = u8::try_from(*u).unwrap_or(0x6F);
            }
        }
        Type::VarUcs2(n) => {
            let len = units.len().min(*n as usize);
            b[..2].copy_from_slice(&u16::try_from(len).expect("fits").to_be_bytes());
            for (i, u) in units.iter().take(len).enumerate() {
                b[2 + 2 * i..4 + 2 * i].copy_from_slice(&u.to_be_bytes());
            }
        }
        Type::Ucs2(_) | Type::Graphic(_) => {
            let n = b.len() / 2;
            for i in 0..n {
                let u = units.get(i).copied().unwrap_or(0x20);
                b[2 * i..2 * i + 2].copy_from_slice(&u.to_be_bytes());
            }
        }
        _ => {
            for (i, slot) in b.iter_mut().enumerate() {
                *slot = u8::try_from(units.get(i).copied().unwrap_or(0x40)).unwrap_or(0x6F);
            }
        }
    }
}

/// A position or length outside the text.
fn range(start: i64, len: i64, total: usize) -> Result<(usize, usize), Cond> {
    let s = usize::try_from(start)
        .ok()
        .filter(|s| *s >= 1)
        .ok_or(Cond::Mch0603)?;
    let l = usize::try_from(len).map_err(|_| Cond::Mch0603)?;
    if s - 1 + l > total {
        return Err(Cond::Mch0603);
    }
    Ok((s - 1, l))
}

/// `str_cat(env, dst, dty, a, aty, b, bty)`.
pub extern "sysv64" fn str_cat(
    _env: *mut Env,
    dst: i64,
    dty: i64,
    a: i64,
    aty: i64,
    b: i64,
    bty: i64,
) {
    let x = read_text(a, aty);
    let y = read_text(b, bty);
    let wide = x.wide || y.wide;
    let mut units = to_width(&x, wide);
    units.extend(to_width(&y, wide));
    write_text(dst, dty, &Text { units, wide });
}

/// `str_subst(env, dst, dty, src, sty, start, len)`.
pub extern "sysv64" fn str_subst(
    env: *mut Env,
    dst: i64,
    dty: i64,
    src: i64,
    sty: i64,
    start: i64,
    len: i64,
) {
    let s = read_text(src, sty);
    match range(start, len, s.units.len()) {
        Ok((at, n)) => write_text(
            dst,
            dty,
            &Text {
                units: s.units[at..at + n].to_vec(),
                wide: s.wide,
            },
        ),
        Err(c) => raise_on(job(env), c),
    }
}

/// `str_scan(env, needle, nty, hay, hty, start) -> i32`.
pub extern "sysv64" fn str_scan(
    env: *mut Env,
    needle: i64,
    nty: i64,
    hay: i64,
    hty: i64,
    start: i64,
) -> i32 {
    let n = read_text(needle, nty);
    let h = read_text(hay, hty);
    let n = to_width(&n, h.wide);
    let from = if start == 0 { 1 } else { start };
    let Ok((at, _)) = range(from, 0, h.units.len()) else {
        raise_on(job(env), Cond::Mch0603);
        return 0;
    };
    if n.is_empty() || n.len() > h.units.len() {
        return 0;
    }
    h.units[at..]
        .windows(n.len())
        .position(|w| w == n.as_slice())
        .map_or(0, |p| i32::try_from(at + p + 1).unwrap_or(0))
}

/// `str_check(env, set, sty, src, srcty, start, reverse) -> i32`.
pub extern "sysv64" fn str_check(
    env: *mut Env,
    set: i64,
    sty: i64,
    src: i64,
    srcty: i64,
    start: i64,
    reverse: i64,
) -> i32 {
    let set = read_text(set, sty);
    let s = read_text(src, srcty);
    let set = to_width(&set, s.wide);
    // Without a start, a reverse check begins at the last position.
    let from = match (start, reverse != 0) {
        (0, true) => i64::try_from(s.units.len()).unwrap_or(1).max(1),
        (0, false) => 1,
        (n, _) => n,
    };
    let Ok((at, _)) = range(from, 0, s.units.len()) else {
        raise_on(job(env), Cond::Mch0603);
        return 0;
    };
    let bad = |u: &u16| !set.contains(u);
    let pos = if reverse != 0 {
        s.units[..=at.min(s.units.len().saturating_sub(1))]
            .iter()
            .rposition(bad)
    } else {
        s.units[at..].iter().position(bad).map(|p| at + p)
    };
    pos.map_or(0, |p| i32::try_from(p + 1).unwrap_or(0))
}

/// `str_xlate(env, dst, dty, from, fty, to, tty, src, sty, start)`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn str_xlate(
    env: *mut Env,
    dst: i64,
    dty: i64,
    from: i64,
    fty: i64,
    to: i64,
    tty: i64,
    src: i64,
    sty: i64,
    start: i64,
) {
    let s = read_text(src, sty);
    let f = to_width(&read_text(from, fty), s.wide);
    let t = to_width(&read_text(to, tty), s.wide);
    let from_at = if start == 0 { 1 } else { start };
    let Ok((at, _)) = range(from_at, 0, s.units.len()) else {
        raise_on(job(env), Cond::Mch0603);
        return;
    };
    let mut units = s.units.clone();
    for u in units.iter_mut().skip(at) {
        if let Some(i) = f.iter().position(|x| x == u) {
            *u = t.get(i).copied().unwrap_or(blank(s.wide));
        }
    }
    write_text(
        dst,
        dty,
        &Text {
            units,
            wide: s.wide,
        },
    );
}

/// `str_trim(env, dst, dty, src, sty, chars, cty, side)`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn str_trim(
    _env: *mut Env,
    dst: i64,
    dty: i64,
    src: i64,
    sty: i64,
    chars: i64,
    cty: i64,
    side: i64,
) {
    let s = read_text(src, sty);
    let set: Vec<u16> = if chars == 0 {
        vec![blank(s.wide)]
    } else {
        to_width(&read_text(chars, cty), s.wide)
    };
    let mut lo = 0;
    let mut hi = s.units.len();
    if side != 2 {
        while lo < hi && set.contains(&s.units[lo]) {
            lo += 1;
        }
    }
    if side != 1 {
        while hi > lo && set.contains(&s.units[hi - 1]) {
            hi -= 1;
        }
    }
    write_text(
        dst,
        dty,
        &Text {
            units: s.units[lo..hi].to_vec(),
            wide: s.wide,
        },
    );
}

/// Compare two texts, the shorter padded with blanks: EBCDIC
/// collation for narrow text, code-unit order for wide.
#[allow(clippy::many_single_char_names)]
pub(crate) fn compare(a: &Text, b: &Text) -> Ordering {
    let wide = a.wide || b.wide;
    let mut x = to_width(a, wide);
    let mut y = to_width(b, wide);
    let n = x.len().max(y.len());
    x.resize(n, blank(wide));
    y.resize(n, blank(wide));
    if wide {
        x.cmp(&y)
    } else {
        let xb: Vec<u8> = x.iter().map(|u| u8::try_from(*u).unwrap_or(0x6F)).collect();
        let yb: Vec<u8> = y.iter().map(|u| u8::try_from(*u).unwrap_or(0x6F)).collect();
        ccsid::ebcdic_cmp(&xb, &yb)
    }
}

/// `str_cmp(env, a, aty, b, bty) -> -1|0|1`.
pub extern "sysv64" fn str_cmp(_env: *mut Env, a: i64, aty: i64, b: i64, bty: i64) -> i32 {
    match compare(&read_text(a, aty), &read_text(b, bty)) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// `str_len(env, src, sty) -> i32`.
pub extern "sysv64" fn str_len(_env: *mut Env, src: i64, sty: i64) -> i32 {
    i32::try_from(read_text(src, sty).units.len()).unwrap_or(i32::MAX)
}

/// `str_setlen(env, dst, dty, n)`.
pub extern "sysv64" fn str_setlen(env: *mut Env, dst: i64, dty: i64, n: i64) {
    let (b, t) = typed(dst, dty);
    let max = t.string_len().unwrap_or(0);
    let Ok(len) = u16::try_from(n) else {
        raise_on(job(env), Cond::Mch0603);
        return;
    };
    if u32::from(len) > max || !t.is_varying() {
        raise_on(job(env), Cond::Mch0603);
        return;
    }
    b[..2].copy_from_slice(&len.to_be_bytes());
}

/// `str_replace(env, dst, dty, src, sty, repl, rty, start, len)`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn str_replace(
    env: *mut Env,
    dst: i64,
    dty: i64,
    src: i64,
    sty: i64,
    repl: i64,
    rty: i64,
    start: i64,
    len: i64,
) {
    let s = read_text(src, sty);
    let r = to_width(&read_text(repl, rty), s.wide);
    let len = if len < 0 {
        i64::try_from(r.len()).unwrap_or(0)
    } else {
        len
    };
    let Ok((at, n)) = range(start, len, s.units.len()) else {
        raise_on(job(env), Cond::Mch0603);
        return;
    };
    let mut units = s.units[..at].to_vec();
    units.extend_from_slice(&r);
    units.extend_from_slice(&s.units[at + n..]);
    write_text(
        dst,
        dty,
        &Text {
            units,
            wide: s.wide,
        },
    );
}

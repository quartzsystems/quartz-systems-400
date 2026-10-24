// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bound procedures of the string, array, bit and move operations
//! (`ABI.md` §3: the environment, the argument count, the descriptor
//! list, then the parameters). Storage comes as an address with a
//! descriptor word (`value::Desc`).

use codegen::abi::Env;

use crate::value::{Desc, bytes};
use crate::{arrays, mov, strops};

fn raise(env: *mut Env, c: mi::Cond) {
    mi::storage::job_of(env).raise(c);
}

fn a(x: i64) -> u64 {
    x.cast_unsigned()
}

fn n(x: i64) -> usize {
    usize::try_from(x).unwrap_or(0)
}

fn img(env: *mut Env, addr: i64, d: i64) -> Option<Vec<u8>> {
    match mov::image(a(addr), &Desc::of(d)) {
        Ok(v) => Some(v),
        Err(c) => {
            raise(env, c);
            None
        }
    }
}

/// `_QRNX_MOVEA(pad, dst, dd, dcount, src, sd, scount)`.
pub extern "sysv64" fn movea(env: *mut Env, _argc: i64, _od: i64, pad: i32, dst: i64, dd: i64, dcount: i64, src: i64, sd: i64, scount: i64) {
    if let Err(c) = mov::movea(pad != 0, a(dst), &Desc::of(dd), n(dcount), a(src), &Desc::of(sd), n(scount)) {
        raise(env, c);
    }
}

/// `_QRNX_ZONE(op, dst, dd, src, sd)`: the zone moves.
pub extern "sysv64" fn zone(env: *mut Env, _argc: i64, _od: i64, op: i32, dst: i64, dd: i64, src: i64, sd: i64) {
    if let Err(c) = mov::zone(op, a(dst), &Desc::of(dd), a(src), &Desc::of(sd)) {
        raise(env, c);
    }
}

/// `_QRNX_CAT(pad, blanks, dst, dd, a, ad, b, bd)`.
pub extern "sysv64" fn cat(env: *mut Env, _argc: i64, _od: i64, pad: i32, blanks: i64, dst: i64, dd: i64, x: i64, xd: i64, y: i64, yd: i64) {
    if let Err(c) = strops::cat(pad != 0, blanks, a(dst), &Desc::of(dd), a(x), &Desc::of(xd), a(y), &Desc::of(yd)) {
        raise(env, c);
    }
}

/// `_QRNX_SCAN(needle, nd, nlen, hay, hd, start, res, rd, count) -> i32`:
/// 1 found, 0 not, 2 a start outside the string.
pub extern "sysv64" fn scan(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    needle: i64,
    nd: i64,
    nlen: i64,
    hay: i64,
    hd: i64,
    start: i64,
    res: i64,
    rd: i64,
    count: i64,
) -> i32 {
    let (Some(mut ne), Some(h)) = (img(env, needle, nd), img(env, hay, hd)) else { return 0 };
    if nlen >= 0 {
        ne.truncate(n(nlen));
    }
    if start < 1 || n(start) > h.len().max(1) {
        return 2;
    }
    match strops::scan(&ne, &h, n(start), a(res), &Desc::of(rd), n(count)) {
        Ok(f) => i32::from(f),
        Err(c) => {
            raise(env, c);
            0
        }
    }
}

/// `_QRNX_CHECK(right, set, sd, src, srcd, start, res, rd, count) -> i32`:
/// 1 found, 0 not, 2 a start outside the string; `start` 0 is the
/// default (the first or the last position).
pub extern "sysv64" fn check(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    right: i32,
    set: i64,
    sd: i64,
    src: i64,
    srcd: i64,
    start: i64,
    res: i64,
    rd: i64,
    count: i64,
) -> i32 {
    let (Some(s), Some(t)) = (img(env, set, sd), img(env, src, srcd)) else { return 0 };
    let start = if start == 0 {
        if right != 0 { t.len() } else { 1 }
    } else {
        if start < 1 || n(start) > t.len() {
            return 2;
        }
        n(start)
    };
    match strops::check(right != 0, &s, &t, start, a(res), &Desc::of(rd), n(count)) {
        Ok(f) => i32::from(f),
        Err(c) => {
            raise(env, c);
            0
        }
    }
}

/// `_QRNX_XLATE(pad, from, fd, to, td, src, srcd, start, dst, dd) -> i32`:
/// 2 for a start outside the string.
pub extern "sysv64" fn xlate(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    pad: i32,
    from: i64,
    fd: i64,
    to: i64,
    td: i64,
    src: i64,
    srcd: i64,
    start: i64,
    dst: i64,
    dd: i64,
) -> i32 {
    let (Some(f), Some(t), Some(s)) = (img(env, from, fd), img(env, to, td), img(env, src, srcd)) else { return 0 };
    if start < 1 || n(start) > s.len().max(1) {
        return 2;
    }
    let out = strops::xlate(&f, &t, &s, n(start));
    strops::put_text(a(dst), &Desc::of(dd), &out, pad != 0);
    0
}

/// `_QRNX_SUBST(pad, len, src, sd, start, dst, dd) -> i32`: `len`
/// negative takes the rest; 2 for a start or length outside the string.
pub extern "sysv64" fn subst(env: *mut Env, _argc: i64, _od: i64, pad: i32, len: i64, src: i64, sd: i64, start: i64, dst: i64, dd: i64) -> i32 {
    let Some(s) = img(env, src, sd) else { return 0 };
    if start < 1 || n(start) > s.len() {
        return 2;
    }
    let from = n(start) - 1;
    let len = if len < 0 { s.len() - from } else { n(len) };
    if len == 0 || from + len > s.len() {
        return 2;
    }
    strops::put_text(a(dst), &Desc::of(dd), &s[from..from + len], pad != 0);
    0
}

/// `_QRNX_TESTN(src, sd) -> i32`.
pub extern "sysv64" fn testn(env: *mut Env, _argc: i64, _od: i64, src: i64, sd: i64) -> i32 {
    img(env, src, sd).map_or(0, |s| strops::testn(&s))
}

/// `_QRNX_TESTZ(src, sd) -> i32`.
pub extern "sysv64" fn testz(env: *mut Env, _argc: i64, _od: i64, src: i64, sd: i64) -> i32 {
    img(env, src, sd).map_or(3, |s| strops::testz(&s))
}

/// `_QRNX_BITS(op, dst, mask) -> i32`: one byte each.
pub extern "sysv64" fn bits(_env: *mut Env, _argc: i64, _od: i64, op: i32, dst: i64, mask: i64) -> i32 {
    let m = bytes(a(mask), 1)[0];
    let b = &mut bytes(a(dst), 1)[0];
    strops::bits(op, b, m)
}

/// `_QRNX_LOOKUP(arg, arr, ed, start, count, want, stride, off) -> i64`:
/// the 1-based index found in the low 32 bits and what matched above
/// them; 0 when nothing matched. Elements are `stride` bytes apart, the
/// key `off` bytes into each.
pub extern "sysv64" fn lookup(
    _env: *mut Env,
    _argc: i64,
    _od: i64,
    arg: i64,
    arr: i64,
    ed: i64,
    start: i64,
    count: i64,
    want: i32,
    stride: i64,
    off: i64,
) -> i64 {
    match arrays::lookup(a(arg), a(arr) + a(off), &Desc::of(ed), n(start), n(count), want, a(stride)) {
        Some((i, k)) => i64::try_from(i).unwrap_or(0) | (i64::from(k) << 32),
        None => 0,
    }
}

/// `_QRNX_SORTA(base, start, count, elsize, keyoff, kd, desc)`.
pub extern "sysv64" fn sorta(_env: *mut Env, _argc: i64, _od: i64, base: i64, start: i64, count: i64, elsize: i64, keyoff: i64, kd: i64, desc: i32) {
    arrays::sorta(a(base) + a(start) * a(elsize), n(count), n(elsize), n(keyoff), &Desc::of(kd), desc != 0);
}

/// Text into a varying field of `cap` bytes.
fn put_var(dst: i64, cap: i64, text: &[u8]) {
    let (cap, k) = (n(cap), text.len().min(n(cap)));
    crate::value::write_var(a(dst), crate::value::prefix_for(cap), &text[..k]);
}

/// `_QRNX_SCANR(needle, nd, hay, hd, start, len) -> i32`.
pub extern "sysv64" fn scanr(env: *mut Env, _argc: i64, _od: i64, needle: i64, nd: i64, hay: i64, hd: i64, start: i64, len: i64) -> i32 {
    let (Some(ne), Some(h)) = (img(env, needle, nd), img(env, hay, hd)) else { return 0 };
    if let Some(p) = strops::scanr(&ne, &h, start, len) {
        i32::try_from(p).unwrap_or(0)
    } else {
        raise(env, mi::Cond::Rnx0100);
        0
    }
}

/// `_QRNX_SCANRPL(dst, cap, scan, sd, repl, rd, src, srcd, start, len)`.
pub extern "sysv64" fn scanrpl(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    dst: i64,
    cap: i64,
    scan: i64,
    sd: i64,
    repl: i64,
    rd: i64,
    src: i64,
    srcd: i64,
    start: i64,
    len: i64,
) {
    let (Some(s), Some(r), Some(t)) = (img(env, scan, sd), img(env, repl, rd), img(env, src, srcd)) else { return };
    match strops::scanrpl(&s, &r, &t, start, len) {
        Some(out) => put_var(dst, cap, &out),
        None => raise(env, mi::Cond::Rnx0100),
    }
}

/// `_QRNX_SPLIT(dst, elen, cap, src, sd, seps, sepd) -> i64`: the
/// pieces into an array of `cap` varying elements of `elen` bytes.
pub extern "sysv64" fn split(env: *mut Env, _argc: i64, _od: i64, dst: i64, elen: i64, cap: i64, src: i64, sd: i64, seps: i64, pd: i64) -> i64 {
    let (Some(s), Some(p)) = (img(env, src, sd), img(env, seps, pd)) else { return 0 };
    let pieces = strops::split(&s, &p);
    let size = i64::try_from(n(elen) + crate::value::prefix_for(n(elen))).unwrap_or(0);
    let mut k = 0;
    for piece in pieces.iter().take(n(cap)) {
        put_var(dst + k * size, elen, piece);
        k += 1;
    }
    k
}

/// `_QRNX_EXTREME(arr, ed, start, count, stride, off, max) -> i64`.
pub extern "sysv64" fn extreme(_env: *mut Env, _argc: i64, _od: i64, arr: i64, ed: i64, start: i64, count: i64, stride: i64, off: i64, max: i32) -> i64 {
    i64::try_from(arrays::extreme(a(arr) + a(off), &Desc::of(ed), n(start), n(count), a(stride), max != 0)).unwrap_or(0)
}

/// `_QRNX_STRGET(dst, cap, src)`: the bytes before the first null.
pub extern "sysv64" fn strget(_env: *mut Env, _argc: i64, _od: i64, dst: i64, cap: i64, src: i64) {
    let s = bytes(a(src), n(cap));
    let end = s.iter().position(|b| *b == 0).unwrap_or(s.len());
    let t = s[..end].to_vec();
    put_var(dst, cap, &t);
}

/// `_QRNX_STRSET(dst, cap, src, sd)`: the value and a null, in `cap`
/// bytes at most.
pub extern "sysv64" fn strset(env: *mut Env, _argc: i64, _od: i64, dst: i64, cap: i64, src: i64, sd: i64) {
    let Some(v) = img(env, src, sd) else { return };
    let out = bytes(a(dst), n(cap));
    let k = v.len().min(out.len().saturating_sub(1));
    out[..k].copy_from_slice(&v[..k]);
    if k < out.len() {
        out[k] = 0;
    }
}

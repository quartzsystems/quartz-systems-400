// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bound procedures for dates, times and timestamps. Every operand
//! is an address, its descriptor word and its format word (`dates`).
//! A failure signals `RNX0112`, `RNX0113` or `RNX0114` (status 112,
//! 113, 114) to the caller.

use codegen::abi::Env;

use crate::dates::{self, Class, Dt, DtErr, Fmt};
use crate::value::{Desc, Kind};

fn signal(env: *mut Env, e: DtErr) {
    let _ = mi::storage::job_of(env).send_message(
        e.msgid(),
        "QRNXMSG",
        "QSYS",
        Vec::new(),
        qir::MsgType::Escape,
        mi::msg::QueueRef::Same,
    );
}

/// The class of an operand: its own for date storage, its format's for
/// characters and numbers.
fn class(d: &Desc, f: Fmt) -> Class {
    match d.kind {
        Kind::Date => Class::Date,
        Kind::Time => Class::Time,
        Kind::Timestamp => Class::Stamp,
        _ => dates::class_of(f),
    }
}

fn read(addr: i64, desc: i64, fmt: i64) -> Result<(Dt, Class), DtErr> {
    let d = Desc::of(desc);
    let f = Fmt::of(fmt);
    Ok((dates::read(addr.cast_unsigned(), &d, f)?, class(&d, f)))
}

/// Both operands in one class: a date against a timestamp is midnight.
fn pair(a: (Dt, Class), b: (Dt, Class)) -> (Dt, Dt) {
    if a.1 == b.1 {
        return (a.0, b.0);
    }
    (dates::recast(a.0, a.1, Class::Stamp), dates::recast(b.0, b.1, Class::Stamp))
}

/// `_QRNX_DTCVT(dst, dd, df, src, sd, sf)`: assignment and conversion.
pub extern "sysv64" fn cvt(env: *mut Env, _argc: i64, _od: i64, dst: i64, dd: i64, df: i64, src: i64, sd: i64, sf: i64) {
    let r = read(src, sd, sf).and_then(|(v, from)| {
        let d = Desc::of(dd);
        let f = Fmt::of(df);
        let v = dates::recast(v, from, class(&d, f));
        dates::write(dst.cast_unsigned(), &d, f, v)
    });
    if let Err(e) = r {
        signal(env, e);
    }
}

/// `_QRNX_DTCMP(a, ad, af, b, bd, bf) -> -1|0|1`.
pub extern "sysv64" fn cmp(env: *mut Env, _argc: i64, _od: i64, a: i64, ad: i64, af: i64, b: i64, bd: i64, bf: i64) -> i32 {
    match read(a, ad, af).and_then(|x| Ok((x, read(b, bd, bf)?))) {
        Ok((x, y)) => {
            let (x, y) = pair(x, y);
            match x.cmp(&y) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }
        }
        Err(e) => {
            signal(env, e);
            0
        }
    }
}

/// `_QRNX_DTADD(dst, dd, df, src, sd, sf, n, unit)`: `src + n` units.
pub extern "sysv64" fn add(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    dst: i64,
    dd: i64,
    df: i64,
    src: i64,
    sd: i64,
    sf: i64,
    n: i64,
    unit: i32,
) {
    let r = read(src, sd, sf).and_then(|(v, c)| {
        let v = dates::add(v, c, n, unit)?;
        let d = Desc::of(dd);
        let f = Fmt::of(df);
        dates::write(dst.cast_unsigned(), &d, f, dates::recast(v, c, class(&d, f)))
    });
    if let Err(e) = r {
        signal(env, e);
    }
}

/// `_QRNX_DTDIFF(a, ad, af, b, bd, bf, unit) -> i64`: `%DIFF`.
pub extern "sysv64" fn diff(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    a: i64,
    ad: i64,
    af: i64,
    b: i64,
    bd: i64,
    bf: i64,
    unit: i32,
) -> i64 {
    let r = read(a, ad, af).and_then(|x| Ok((x, read(b, bd, bf)?))).and_then(|(x, y)| {
        let (x, y) = pair(x, y);
        dates::diff(x, y, unit)
    });
    r.unwrap_or_else(|e| {
        signal(env, e);
        0
    })
}

/// `_QRNX_DTSUB(src, sd, sf, unit) -> i64`: `%SUBDT`, `EXTRCT`.
pub extern "sysv64" fn sub(env: *mut Env, _argc: i64, _od: i64, src: i64, sd: i64, sf: i64, unit: i32) -> i64 {
    read(src, sd, sf).and_then(|(v, _)| dates::part(v, unit)).unwrap_or_else(|e| {
        signal(env, e);
        0
    })
}

/// `_QRNX_DTTEST(src, sd, sf) -> 1|0`: whether the operand holds a
/// valid value (`TEST`).
pub extern "sysv64" fn test(_env: *mut Env, _argc: i64, _od: i64, src: i64, sd: i64, sf: i64) -> i32 {
    i32::from(read(src, sd, sf).is_ok())
}

/// `_QRNX_DTNOW(dst, dd, df)`: the current date, time or timestamp.
pub extern "sysv64" fn now(env: *mut Env, _argc: i64, _od: i64, dst: i64, dd: i64, df: i64) {
    let d = Desc::of(dd);
    let f = Fmt::of(df);
    let v = dates::recast(dates::now(), Class::Stamp, class(&d, f));
    if let Err(e) = dates::write(dst.cast_unsigned(), &d, f, v) {
        signal(env, e);
    }
}

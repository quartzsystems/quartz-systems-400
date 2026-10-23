// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bound procedures generated RPG code calls. Each follows the
//! procedure convention (`ABI.md` §3): the environment, the argument
//! count, the descriptor list, then the parameters: addresses for
//! by-reference ones, values for `byval` ones.

use codegen::abi::Env;
use qdecimal::{EditOpts, Fill};

use crate::fmt;
use crate::mov;
use crate::value::{Desc, Kind, bytes, ebcdic, read_f64, read_num, write_varchar};

fn job(env: *mut Env) -> &'static mut mi::Job {
    mi::storage::job_of(env)
}

/// `_QRNX_CHARNUM(dst: varchar, src, desc, point)`: `%CHAR` of a number.
pub extern "sysv64" fn charnum(env: *mut Env, _argc: i64, _od: i64, dst: i64, src: i64, desc: i64, point: i32) {
    let d = Desc::of(desc);
    let point = char::from(u8::try_from(point).unwrap_or(b'.'));
    let text = if d.kind == Kind::Float {
        read_f64(src.cast_unsigned(), &d).map(|f| fmt::char_of_float(f, d.len == 4))
    } else {
        match read_num(src.cast_unsigned(), &d) {
            Ok(v) => d.shape().map(|sh| fmt::char_of(&v, sh, point)),
            Err(c) => {
                job(env).raise(c);
                return;
            }
        }
    };
    write_varchar(dst.cast_unsigned(), &ebcdic(&text.unwrap_or_default()));
}

/// `_QRNX_EDITC(dst: varchar, src, desc, code, fill, cur)`: `%EDITC`.
pub extern "sysv64" fn editc(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    dst: i64,
    src: i64,
    desc: i64,
    code: i32,
    fill: i32,
    cur: i64,
) {
    let d = Desc::of(desc);
    let v = match read_num(src.cast_unsigned(), &d) {
        Ok(v) => v,
        Err(c) => {
            job(env).raise(c);
            return;
        }
    };
    let Some(shape) = d.shape() else { return };
    let code = char::from_u32(u32::try_from(code).unwrap_or(0)).unwrap_or('1');
    let cur_ch = ccsid::char_of(bytes(cur.cast_unsigned(), 1)[0]);
    let opts = EditOpts {
        fill: match fill {
            1 => Fill::Asterisk,
            2 => Fill::Currency('$'),
            3 => Fill::Currency(cur_ch),
            _ => Fill::Blank,
        },
        ..EditOpts::default()
    };
    let text = qdecimal::edit_code(&v, shape, code, &opts).unwrap_or_default();
    write_varchar(dst.cast_unsigned(), &ebcdic(&text));
}

/// `_QRNX_EDITW(dst: char, dlen, src, desc, word, wlen)`: `%EDITW`.
pub extern "sysv64" fn editw(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    dst: i64,
    dlen: i32,
    src: i64,
    desc: i64,
    word: i64,
    wlen: i32,
) {
    let d = Desc::of(desc);
    let v = match read_num(src.cast_unsigned(), &d) {
        Ok(v) => v,
        Err(c) => {
            job(env).raise(c);
            return;
        }
    };
    let Some(shape) = d.shape() else { return };
    let w = ccsid::from_ebcdic(bytes(word.cast_unsigned(), usize::try_from(wlen).unwrap_or(0)));
    let text = qdecimal::edit_word(&v, shape, &w).unwrap_or_default();
    let out = bytes(dst.cast_unsigned(), usize::try_from(dlen).unwrap_or(0));
    out.fill(0x40);
    let e = ebcdic(&text);
    let n = e.len().min(out.len());
    out[..n].copy_from_slice(&e[..n]);
}

/// `_QRNX_MOVE(left, pad, dst, dd, src, sd)`: `MOVE` and `MOVEL`.
pub extern "sysv64" fn move_(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    left: i32,
    pad: i32,
    dst: i64,
    dd: i64,
    src: i64,
    sd: i64,
) {
    if let Err(c) = mov::mov(left != 0, pad != 0, dst.cast_unsigned(), &Desc::of(dd), src.cast_unsigned(), &Desc::of(sd)) {
        job(env).raise(c);
    }
}

/// `_QRNX_POW(a, b) -> f64`: `**`.
pub extern "sysv64" fn pow(_env: *mut Env, _argc: i64, _od: i64, a: f64, b: f64) -> f64 {
    a.powf(b)
}

/// `_QRNX_SQRT(a) -> f64`: `%SQRT`.
pub extern "sysv64" fn sqrt(_env: *mut Env, _argc: i64, _od: i64, a: f64) -> f64 {
    a.sqrt()
}

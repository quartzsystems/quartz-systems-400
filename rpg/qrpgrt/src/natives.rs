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
    // The fill in the low byte; DATEDIT's format (bits 8-9: MDY, DMY,
    // YMD) and separator (bits 16-23, 0 for the default); DECEDIT's
    // comma (bit 24) and leading zero (bit 25).
    let date_sep = u8::try_from((fill >> 16) & 0xFF).ok().filter(|b| *b != 0).map(char::from);
    let opts = EditOpts {
        fill: match fill & 0xFF {
            1 => Fill::Asterisk,
            2 | 3 => Fill::Currency(cur_ch),
            _ => Fill::Blank,
        },
        date_fmt: match (fill >> 8) & 3 {
            1 => qdecimal::DateEditFmt::Dmy,
            2 => qdecimal::DateEditFmt::Ymd,
            _ => qdecimal::DateEditFmt::Mdy,
        },
        date_sep,
        decimal_comma: fill & (1 << 24) != 0,
        lead_zero: fill & (1 << 25) != 0,
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

/// `_QRNX_FLTMIX(op, f, dec, desc, first) -> f64`: `f op dec` (or
/// `dec op f` when `first` is 0) for add, subtract, multiply, divide
/// (`op` 0 to 3) as IBM does a float operation with a decimal operand:
/// the decimal converted to the double toward zero from it, the
/// operation done exactly, the result the double toward zero from the
/// exact value. Error-free transforms tell whether the rounded result
/// passes the exact one.
pub extern "sysv64" fn flt_mix(env: *mut Env, _argc: i64, _od: i64, op: i32, f: f64, src: i64, desc: i64, first: i32) -> f64 {
    let d = Desc::of(desc);
    let value = match read_num(src.cast_unsigned(), &d) {
        Ok(v) => v,
        Err(c) => {
            job(env).raise(c);
            return 0.0;
        }
    };
    // The decimal as the double toward zero from it (RPG's conversion of
    // a decimal operand), then the operation on the two doubles exactly.
    let text = value.to_string();
    let mut hi: f64 = text.parse().unwrap_or(0.0);
    let lo = remainder(&text, hi);
    if hi != 0.0 && lo != 0.0 && (lo < 0.0) == (hi > 0.0) {
        hi = f64::from_bits(hi.to_bits() - 1);
    }
    let ((a, alo), (b, blo)) = if first != 0 { ((f, 0.0), (hi, 0.0)) } else { ((hi, 0.0), (f, 0.0)) };
    let (r, err) = match op {
        0 | 1 => {
            let (b, blo) = if op == 1 { (-b, -blo) } else { (b, blo) };
            let r = a + b;
            // TwoSum: the rounding error of a + b, exactly.
            let bb = r - a;
            let e = (a - (r - bb)) + (b - bb);
            (r, e + alo + blo)
        }
        2 => {
            let r = a * b;
            (r, a.mul_add(b, -r) + a * blo + alo * b)
        }
        _ => {
            // A zero divisor is the decimal divide-by-zero (status 102).
            if b == 0.0 {
                job(env).raise(mi::Cond::Mch1211);
                return 0.0;
            }
            let r = a / b;
            // (a + alo) - r (b + blo): its sign, with b's, is (true - r)'s.
            let rem = (-r).mul_add(b, a) + alo - r * blo;
            (r, if b < 0.0 { -rem } else { rem })
        }
    };
    if r != 0.0 && r.is_finite() && err != 0.0 && (err < 0.0) == (r > 0.0) {
        // The rounded result is further from zero than the exact one.
        return f64::from_bits(r.to_bits() - 1);
    }
    r
}

/// `text` (a decimal) less `hi` (its nearest double), as a double.
fn remainder(text: &str, hi: f64) -> f64 {
    // The double's exact value to 60 decimals is enough for the sign
    // and size of a remainder far below its last place.
    let exact = format!("{hi:.60}");
    let (Ok(x), Ok(h)) = (qdecimal::Decimal::parse(text), qdecimal::Decimal::parse(trim_to_63(&exact))) else {
        return 0.0;
    };
    x.sub_exact(&h).map_or(0.0, |r| r.to_string().parse().unwrap_or(0.0))
}

/// A decimal string cut to 63 significant digits.
fn trim_to_63(s: &str) -> &str {
    let mut digits = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() && (digits > 0 || c != '0') {
            digits += 1;
            if digits > 63 {
                return &s[..i];
            }
        }
    }
    s
}

/// `_QRNX_SQRT(a) -> f64`: `%SQRT`.
pub extern "sysv64" fn sqrt(env: *mut Env, _argc: i64, _od: i64, a: f64) -> f64 {
    // The square root of a negative number is status 101.
    if a < 0.0 {
        let _ = job(env).send_message("RNX0101", "QRNXMSG", "*LIBL", Vec::new(), qir::MsgType::Escape, mi::QueueRef::Same);
        return 0.0;
    }
    a.sqrt()
}

/// A float in the external form `%EDITFLT` gives: sign, one digit, the
/// point, 15 digits (7 for a 4-byte float), `E`, sign, 3 digits (2).
#[must_use]
pub fn editflt_text(f: f64, single: bool) -> String {
    #[allow(clippy::cast_possible_truncation)]
    let s = if single { format!("{:.7e}", f as f32) } else { format!("{f:.15e}") };
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let (neg, m) = m.strip_prefix('-').map_or((false, m), |r| (true, r));
    let ev: i32 = e.parse().unwrap_or(0);
    let width = if single { 2 } else { 3 };
    format!("{}{m}E{}{:0width$}", if neg { '-' } else { '+' }, if ev < 0 { '-' } else { '+' }, ev.abs())
}

/// `_QRNX_EDITFLT(dst: varchar, f, single)`: `%EDITFLT`.
pub extern "sysv64" fn editflt(_env: *mut Env, _argc: i64, _od: i64, dst: i64, f: f64, single: i32) {
    write_varchar(dst.cast_unsigned(), &ebcdic(&editflt_text(f, single != 0)));
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Character data to numbers (`%DEC`, `%INT`, `%UNS` of characters):
//! blanks around the value, one sign before or after it, digits with
//! one decimal point (`.` or the `DECEDIT` point). Anything else is
//! `RNX0105`; a value too large for the target is `RNX0103`.

use codegen::abi::Env;
use qdecimal::{Binary, Decimal, Integer, Packed, Rounding, Zoned};

use crate::value::{Desc, Kind, bytes};

fn job(env: *mut Env) -> &'static mut mi::Job {
    mi::storage::job_of(env)
}

/// Parse RPG numeric text; `None` when it is not a number.
#[must_use]
pub fn parse(text: &str, point: char) -> Option<Decimal> {
    let t = text.trim();
    if t.is_empty() {
        return Some(Decimal::zero());
    }
    let (neg, body) = if let Some(r) = t.strip_prefix('-') {
        (true, r.trim_start())
    } else if let Some(r) = t.strip_prefix('+') {
        (false, r.trim_start())
    } else if let Some(r) = t.strip_suffix('-') {
        (true, r.trim_end())
    } else if let Some(r) = t.strip_suffix('+') {
        (false, r.trim_end())
    } else {
        (false, t)
    };
    let mut seen_point = false;
    let mut digits = String::new();
    for c in body.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else if (c == '.' || c == point) && !seen_point {
            seen_point = true;
            digits.push('.');
        } else {
            return None;
        }
    }
    if digits.chars().all(|c| c == '.') {
        return None;
    }
    let v = Decimal::parse(&digits).ok()?;
    Some(if neg { v.negated() } else { v })
}

/// Write a number to a target by its descriptor, with RPG's positive
/// sign; `false` when it does not fit.
pub fn write(addr: u64, d: &Desc, v: &Decimal, r: Rounding) -> bool {
    let digits = u8::try_from(d.len.clamp(1, 63)).unwrap_or(63);
    let enc = match d.kind {
        Kind::Packed => Packed::new(digits, d.dec).and_then(|p| p.encode(v, r)),
        Kind::Zoned => Zoned::new(digits, d.dec).and_then(|z| z.encode(v, r)),
        Kind::Bin => Binary::new(digits, d.dec).and_then(|b| b.encode(v, r)),
        Kind::Float => return crate::value::write_num(addr, d, v).is_ok(),
        Kind::Int | Kind::Uns => v
            .rescale(0, r)
            .and_then(|w| Integer::new(u8::try_from(d.size()).unwrap_or(8), d.kind == Kind::Int).and_then(|i| i.encode(&w))),
        _ => return false,
    };
    let Ok(enc) = enc else { return false };
    let out = bytes(addr, enc.len());
    out.copy_from_slice(&enc);
    if !v.is_negative()
        && let Some(last) = out.last_mut()
    {
        match d.kind {
            Kind::Packed if *last & 0x0F == 0x0C => *last |= 0x0F,
            Kind::Zoned if *last >> 4 == 0x0C => *last |= 0xF0,
            _ => {}
        }
    }
    true
}

/// `_QRNX_CHARTONUM(dst, dd, src, sd, flags)`: the decimal point in
/// the low byte of `flags`, half-adjust in bit 8.
pub extern "sysv64" fn char_to_num(env: *mut Env, _argc: i64, _od: i64, dst: i64, dd: i64, src: i64, sd: i64, flags: i32) {
    let s = Desc::of(sd);
    let raw = bytes(src.cast_unsigned(), s.size());
    let text = match s.kind {
        Kind::VarChar => ccsid::from_ebcdic(s.var_data(raw)),
        _ => ccsid::from_ebcdic(raw),
    };
    let point = char::from(u8::try_from(flags & 0xFF).unwrap_or(b'.'));
    let r = if flags & 0x100 != 0 { Rounding::HalfAdjust } else { Rounding::Truncate };
    let j = job(env);
    let Some(v) = parse(&text, point) else {
        let _ = j.send_message("RNX0105", "QRNXMSG", "*LIBL", Vec::new(), qir::MsgType::Escape, mi::QueueRef::Same);
        return;
    };
    if !write(dst.cast_unsigned(), &Desc::of(dd), &v, r) {
        let _ = j.send_message("RNX0103", "QRNXMSG", "*LIBL", Vec::new(), qir::MsgType::Escape, mi::QueueRef::Same);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpg_numeric_text() {
        let p = |s| parse(s, '.').map(|d| d.to_string());
        assert_eq!(p(" 12.50 "), Some("12.50".into()));
        assert_eq!(p("-3"), Some("-3".into()));
        assert_eq!(p("3-"), Some("-3".into()));
        assert_eq!(p("+ 4"), Some("4".into()));
        assert_eq!(p("12x"), None);
        assert_eq!(p("1 2"), None);
        assert_eq!(p("."), None);
        assert_eq!(p("   "), Some("0".into()));
    }
}

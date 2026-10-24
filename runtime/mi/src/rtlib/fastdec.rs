// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A fast path for the decimal instructions: packed, zoned and integer
//! operands of at most 38 digits as scaled `i128`, with the exact
//! path's results and errors. Anything else (other storage, a value
//! past 38 digits, bytes that are not valid) is `None`, and the exact
//! path takes it.

#![allow(clippy::many_single_char_names)]

use codegen::Cond;
use qir::Type;

/// 10 to the power `n`, for `n` up to 38.
fn pow10(n: u32) -> Option<i128> {
    10i128.checked_pow(n)
}

/// An operand's value at its scale.
pub(crate) fn read(b: &[u8], t: &Type) -> Option<(i128, u8)> {
    match t {
        Type::Packed(s) if s.digits <= 38 => {
            let n = b.len() * 2 - 1;
            let pad = n - usize::from(s.digits);
            let mut v: i128 = 0;
            for k in 0..n {
                let byte = b[k / 2];
                let d = if k.is_multiple_of(2) { byte >> 4 } else { byte & 0xF };
                if d > 9 || (k < pad && d != 0) {
                    return None;
                }
                v = v * 10 + i128::from(d);
            }
            match b[b.len() - 1] & 0xF {
                0xA | 0xC | 0xE | 0xF => Some((v, s.scale)),
                0xB | 0xD => Some((-v, s.scale)),
                _ => None,
            }
        }
        Type::Zoned(s) if s.digits <= 38 => {
            let last = b.len() - 1;
            let mut v: i128 = 0;
            for (k, &byte) in b.iter().enumerate() {
                let d = byte & 0x0F;
                if d > 9 || (k != last && byte >> 4 != 0xF) {
                    return None;
                }
                v = v * 10 + i128::from(d);
            }
            match b[last] >> 4 {
                0xA | 0xC | 0xE | 0xF => Some((v, s.scale)),
                0xB | 0xD => Some((-v, s.scale)),
                _ => None,
            }
        }
        Type::I8 | Type::I16 | Type::I32 | Type::I64 => {
            let mut wide = [if b[0] & 0x80 != 0 { 0xFF } else { 0 }; 16];
            wide[16 - b.len()..].copy_from_slice(b);
            Some((i128::from_be_bytes(wide), 0))
        }
        Type::U8 | Type::U16 | Type::U32 | Type::U64 => {
            let mut wide = [0u8; 16];
            wide[16 - b.len()..].copy_from_slice(b);
            Some((i128::from_be_bytes(wide), 0))
        }
        _ => None,
    }
}

/// `v` at scale `from` taken to scale `to`, dropped digits truncated or
/// half-adjusted away from zero.
fn rescale(v: i128, from: u8, to: u8, halfadj: bool) -> Option<i128> {
    if to >= from {
        return v.checked_mul(pow10(u32::from(to - from))?);
    }
    let p = pow10(u32::from(from - to))?;
    let (q, r) = (v / p, (v % p).abs());
    Some(if halfadj && r >= p / 2 { q + v.signum() } else { q })
}

/// Whether `v` (at the shape's scale) has no more than `digits` digits.
fn fits(v: i128, digits: u8) -> bool {
    pow10(u32::from(digits)).is_none_or(|lim| v.abs() < lim)
}

/// Add (0), subtract (1), multiply (2) or divide (3) at the precision `digits`,
/// `scale`: the exact result truncated to it, a size error when its
/// integer part does not fit.
pub(crate) fn bin(op: i64, (a, sa): (i128, u8), (b, sb): (i128, u8), digits: u8, scale: u8) -> Option<Result<i128, Cond>> {
    let (v, s) = match op {
        0 | 1 => {
            let s = sa.max(sb);
            let a = rescale(a, sa, s, false)?;
            let b = rescale(b, sb, s, false)?;
            (if op == 0 { a.checked_add(b)? } else { a.checked_sub(b)? }, s)
        }
        2 => {
            let exact = sa.checked_add(sb)?;
            let p = a.checked_mul(b)?;
            if exact > scale { (rescale(p, exact, scale, false)?, scale) } else { (p, exact) }
        }
        3 => {
            // The quotient truncated to the precision's decimals; a zero
            // divisor goes the exact way, which raises it.
            if b == 0 {
                return None;
            }
            let num = a.checked_mul(pow10(u32::from(sb) + u32::from(scale))?)?;
            let den = b.checked_mul(pow10(u32::from(sa))?)?;
            (num / den, scale)
        }
        _ => return None,
    };
    let v = rescale(v, s, scale, false)?;
    Some(if fits(v, digits) { Ok(v) } else { Err(Cond::Mch1210) })
}

/// Store `v` at scale `scale` into a packed, zoned or integer target;
/// `fsign` writes a positive sign as `F`.
pub(crate) fn write(b: &mut [u8], t: &Type, v: i128, scale: u8, halfadj: bool, fsign: bool) -> Option<Result<(), Cond>> {
    match t {
        Type::Packed(s) if s.digits <= 38 => {
            let w = rescale(v, scale, s.scale, halfadj)?;
            if !fits(w, s.digits) {
                return Some(Err(Cond::Mch1210));
            }
            let mut m = w.unsigned_abs();
            let n = b.len();
            let sign = if w < 0 { 0xD } else if fsign { 0xF } else { 0xC };
            b[n - 1] = sign | (u8::try_from(m % 10).unwrap_or(0) << 4);
            m /= 10;
            for k in (0..n - 1).rev() {
                let lo = u8::try_from(m % 10).unwrap_or(0);
                m /= 10;
                let hi = u8::try_from(m % 10).unwrap_or(0);
                m /= 10;
                b[k] = (hi << 4) | lo;
            }
            Some(Ok(()))
        }
        Type::Zoned(s) if s.digits <= 38 => {
            let w = rescale(v, scale, s.scale, halfadj)?;
            if !fits(w, s.digits) {
                return Some(Err(Cond::Mch1210));
            }
            let mut m = w.unsigned_abs();
            for byte in b.iter_mut().rev() {
                *byte = 0xF0 | u8::try_from(m % 10).unwrap_or(0);
                m /= 10;
            }
            let n = b.len();
            let zone = if w < 0 { 0xD0 } else if fsign { 0xF0 } else { 0xC0 };
            b[n - 1] = zone | (b[n - 1] & 0x0F);
            Some(Ok(()))
        }
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::U8 | Type::U16 | Type::U32 | Type::U64 => {
            let w = rescale(v, scale, 0, halfadj)?;
            let bits = u32::try_from(b.len() * 8).ok()?;
            let signed = matches!(t, Type::I8 | Type::I16 | Type::I32 | Type::I64);
            let (lo, hi) = if signed { (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1) } else { (0, (1i128 << bits) - 1) };
            if w < lo || w > hi {
                return Some(Err(Cond::Mch1210));
            }
            let n = b.len();
            b.copy_from_slice(&w.to_be_bytes()[16 - n..]);
            Some(Ok(()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qdecimal::{Decimal, Packed, Rounding, Zoned};

    fn packed(digits: u8, scale: u8) -> Type {
        Type::Packed(qdecimal::Shape::new(digits, scale).unwrap())
    }

    #[test]
    fn packed_and_zoned_round_trip_as_the_codec_does() {
        for (text, d, s) in [("12.3", 3, 1), ("-12.3", 5, 2), ("0", 4, 0), ("-0.07", 7, 3), ("99999", 5, 0)] {
            let v = Decimal::parse(text).unwrap();
            let p = Packed::new(d, s).unwrap().encode(&v, Rounding::Truncate).unwrap();
            let (n, sc) = read(&p, &packed(d, s)).unwrap();
            let mut out = vec![0; p.len()];
            write(&mut out, &packed(d, s), n, sc, false, false).unwrap().unwrap();
            assert_eq!(out, p, "{text}");
            let z = Zoned::new(d, s).unwrap().encode(&v, Rounding::Truncate).unwrap();
            let zt = Type::Zoned(qdecimal::Shape::new(d, s).unwrap());
            let (n, sc) = read(&z, &zt).unwrap();
            let mut out = vec![0; z.len()];
            write(&mut out, &zt, n, sc, false, false).unwrap().unwrap();
            assert_eq!(out, z, "{text}");
        }
    }

    #[test]
    fn arithmetic_matches_the_exact_rules() {
        // 1.25 * 3 at (13,2); 0.005 + 1 truncated to (5,2); a size error.
        assert_eq!(bin(2, (125, 2), (3, 0), 13, 2), Some(Ok(375)));
        assert_eq!(bin(0, (5, 3), (1, 0), 5, 2), Some(Ok(100)));
        assert_eq!(bin(0, (999, 0), (1, 0), 3, 0), Some(Err(Cond::Mch1210)));
        // 2 / 3 at (30,10) and -7 / 2 at (5,0), truncated.
        assert_eq!(bin(3, (2, 0), (3, 0), 30, 10), Some(Ok(6_666_666_666)));
        assert_eq!(bin(3, (-7, 0), (2, 0), 5, 0), Some(Ok(-3)));
        assert_eq!(rescale(-125, 2, 1, true), Some(-13));
        assert_eq!(rescale(-124, 2, 1, true), Some(-12));
        assert!(read(&[0x1A, 0x3C], &packed(3, 0)).is_none());
    }
}

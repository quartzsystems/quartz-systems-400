// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Numeric storage as `Decimal` values, and the decimal instructions.

use codegen::Cond;
use codegen::abi::Env;
use codegen::rt::dec_flags;
use qdecimal::{Binary, Decimal, DecimalError, Float, Integer, Packed, Rounding, Shape, Typed, Zoned};
use qir::Type;

use super::core::raise_on;
use super::mem::{job, typed};

/// The condition a decimal error maps to.
pub(crate) fn cond_of(e: &DecimalError) -> Cond {
    match e {
        DecimalError::DecimalData { .. } => Cond::Mch1202,
        DecimalError::DivideByZero => Cond::Mch1211,
        _ => Cond::Mch1210,
    }
}

fn integer(t: &Type) -> Option<Integer> {
    let (bytes, signed) = match t {
        Type::I8 => (1, true),
        Type::I16 => (2, true),
        Type::I32 => (4, true),
        Type::I64 => (8, true),
        Type::U8 => (1, false),
        Type::U16 => (2, false),
        Type::U32 => (4, false),
        Type::U64 => (8, false),
        _ => return None,
    };
    Integer::new(bytes, signed).ok()
}

/// A double's exact value as a decimal, cut to 63 digits; what RPG
/// assigns before the target's truncation (`RPG_OPEN_QUESTIONS.md`,
/// floats).
fn f64_to_decimal_exact(f: f64) -> qdecimal::Result<Decimal> {
    if !f.is_finite() {
        return Err(qdecimal::DecimalError::NotFinite { what: "infinity or NaN" });
    }
    if f == 0.0 {
        return Ok(Decimal::zero());
    }
    // Rust prints the exact binary value to the precision asked for;
    // the decimals the integer digits leave room for.
    let int_digits = format!("{:.0}", f.abs().trunc()).len();
    let decimals = 63usize.saturating_sub(int_digits).min(45);
    let s = format!("{:.*}", decimals + 1, f.abs());
    // Truncate the one extra digit printed for rounding's sake.
    let s = &s[..s.len() - 1];
    let s = s.strip_suffix('.').unwrap_or(s);
    let v = Decimal::parse(s)?;
    Ok(if f < 0.0 { v.negated() } else { v })
}

/// A decimal as the double just toward zero from it, as RPG converts
/// a decimal operand of a float operation.
fn truncated_f64(v: &Decimal) -> f64 {
    let near: f64 = v.to_string().parse().unwrap_or(0.0);
    match f64_to_decimal_exact(near) {
        Ok(back) if back.abs() > v.abs() => {
            let toward_zero = if near > 0.0 { f64::from_bits(near.to_bits() - 1) } else { -f64::from_bits((-near).to_bits() - 1) };
            if near == 0.0 { near } else { toward_zero }
        }
        _ => near,
    }
}

/// Read a numeric operand as a typed value.
pub(crate) fn read_num(addr: i64, desc: i64) -> Result<Typed, Cond> {
    let (b, t) = typed(addr, desc);
    let value = match &t {
        Type::Packed(s) => Packed::new(s.digits, s.scale).and_then(|p| p.decode(b)),
        Type::Zoned(s) => Zoned::new(s.digits, s.scale).and_then(|z| z.decode(b)),
        Type::Bin(s) => Binary::new(s.digits, s.scale).and_then(|z| z.decode(b)),
        Type::F32 => Float::SINGLE.decode(b),
        // RPG takes a double's value to 17 significant digits.
        Type::F64 if desc.cast_unsigned() & codegen::abi::TypeDesc::FSIGN != 0 => {
            f64_to_decimal_exact(f64::from_be_bytes(b[..8].try_into().unwrap_or([0; 8])))
        }
        Type::F64 => Float::DOUBLE.decode(b),
        Type::Ind => Decimal::new(i128::from(b[0] == 0xF1), 0),
        _ => match integer(&t) {
            Some(i) => i.decode(b),
            None => return Err(Cond::Mch1202),
        },
    }
    .map_err(|e| cond_of(&e))?;
    Ok(match t.shape() {
        Some(s) => Typed { value, shape: s },
        None => Typed::literal(value),
    })
}

/// Write a value to a numeric operand, rounding dropped decimals as
/// asked; a value that does not fit is a size error.
pub(crate) fn write_num(addr: i64, desc: i64, v: &Decimal, rounding: Rounding) -> Result<(), Cond> {
    let (b, t) = typed(addr, desc);
    let bytes = match &t {
        Type::Packed(s) => Packed::new(s.digits, s.scale).and_then(|p| p.encode(v, rounding)),
        Type::Zoned(s) => Zoned::new(s.digits, s.scale).and_then(|z| z.encode(v, rounding)),
        Type::Bin(s) => Binary::new(s.digits, s.scale).and_then(|z| z.encode(v, rounding)),
        Type::F32 => Float::SINGLE.encode(v),
        Type::F64 => Float::DOUBLE.encode(v),
        Type::Ind => return Err(Cond::Mch1210),
        _ => match integer(&t) {
            Some(i) => v.rescale(0, rounding).and_then(|w| i.encode(&w)),
            None => return Err(Cond::Mch1210),
        },
    }
    .map_err(|e| cond_of(&e))?;
    b.copy_from_slice(&bytes);
    // The positive sign F, for RPG.
    if desc.cast_unsigned() & codegen::abi::TypeDesc::FSIGN != 0
        && let Some(last) = b.last_mut()
    {
        match &t {
            Type::Packed(_) if *last & 0x0F == 0x0C => *last |= 0x0F,
            Type::Zoned(_) if *last >> 4 == 0x0C => *last |= 0xF0,
            _ => {}
        }
    }
    Ok(())
}

/// A value cut to a decimal target's digits from the left, as
/// fixed-form RPG assignments do; other targets take it as it is.
fn fit_target(addr: i64, desc: i64, v: &Decimal, r: Rounding) -> Result<Decimal, Cond> {
    let (_, t) = typed(addr, desc);
    let shape = match &t {
        Type::Packed(s) | Type::Zoned(s) | Type::Bin(s) => qdecimal::Shape::new(s.digits, s.scale),
        _ => return Ok(v.clone()),
    }
    .map_err(|e| cond_of(&e))?;
    v.fit_truncate(shape, r).map_err(|e| cond_of(&e))
}

fn rounding(halfadj: bool) -> Rounding {
    if halfadj {
        Rounding::HalfAdjust
    } else {
        Rounding::Truncate
    }
}

fn bin(op: i64, a: &Decimal, b: &Decimal, prec: Shape) -> Result<Decimal, Cond> {
    let exact = match op {
        0 => a.add_exact(b),
        1 => a.sub_exact(b),
        2 => a.mul_exact(b),
        3 => a.div_to_scale(b, prec.scale),
        _ => a
            .div_to_scale(b, 0)
            .and_then(|q| q.mul_exact(b))
            .and_then(|qb| a.sub_exact(&qb)),
    }
    .map_err(|e| cond_of(&e))?;
    exact.fit(prec, Rounding::Truncate).map_err(|e| cond_of(&e))
}

/// `dec_bin(env, op, dst, dty, a, aty, b, bty, prec, flags)`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn dec_bin(
    env: *mut Env,
    op: i64,
    dst: i64,
    dty: i64,
    a: i64,
    aty: i64,
    b: i64,
    bty: i64,
    prec: i64,
    flags: i64,
) {
    let r = (|| {
        let av = read_num(a, aty)?;
        let bv = read_num(b, bty)?;
        let digits = u8::try_from(prec & 0xFF).map_err(|_| Cond::Mch1210)?;
        let scale = u8::try_from((prec >> 8) & 0xFF).map_err(|_| Cond::Mch1210)?;
        let shape = Shape::new(digits, scale).map_err(|_| Cond::Mch1210)?;
        let v = bin(op, &av.value, &bv.value, shape)?;
        write_num(dst, dty, &v, rounding(flags & dec_flags::HALFADJ != 0))
    })();
    if let Err(c) = r {
        raise_on(job(env), c);
    }
}

/// `dec_un(env, op, dst, dty, src, sty, halfadj)`.
pub extern "sysv64" fn dec_un(
    env: *mut Env,
    op: i64,
    dst: i64,
    dty: i64,
    src: i64,
    sty: i64,
    halfadj: i64,
) {
    let r = (|| {
        let v = read_num(src, sty)?.value;
        let v = match op {
            1 => v.negated(),
            2 => v.abs(),
            _ => v,
        };
        // Bit 1: drop the high-order digits that do not fit.
        let v = if halfadj & 2 != 0 { fit_target(dst, dty, &v, rounding(halfadj & 1 != 0))? } else { v };
        write_num(dst, dty, &v, rounding(halfadj & 1 != 0))
    })();
    if let Err(c) = r {
        raise_on(job(env), c);
    }
}

/// `dec_cmp(env, a, aty, b, bty) -> -1|0|1`.
pub extern "sysv64" fn dec_cmp(env: *mut Env, a: i64, aty: i64, b: i64, bty: i64) -> i32 {
    match (read_num(a, aty), read_num(b, bty)) {
        (Ok(x), Ok(y)) => match x.value.cmp(&y.value) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
        (Err(c), _) | (_, Err(c)) => {
            raise_on(job(env), c);
            0
        }
    }
}

/// `dec_to_int(env, src, sty, bits, signed, chk) -> i64`.
pub extern "sysv64" fn dec_to_int(
    env: *mut Env,
    src: i64,
    sty: i64,
    bits: i64,
    signed: i64,
    chk: i64,
) -> i64 {
    let r = (|| {
        let v = read_num(src, sty)?.value;
        let whole = v.rescale(0, Rounding::Truncate).map_err(|e| cond_of(&e))?;
        let i = whole.to_i128().ok_or(Cond::Mch1210)?;
        let bits = u32::try_from(bits).unwrap_or(64);
        let (lo, hi) = if signed != 0 {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        };
        if i < lo || i > hi {
            if chk != 0 {
                return Err(Cond::Mch1210);
            }
            let mask = (1i128 << bits) - 1;
            return Ok(i64::try_from(i & mask).unwrap_or(0));
        }
        Ok(i64::try_from(i).unwrap_or(0))
    })();
    match r {
        Ok(v) => v,
        Err(c) => {
            raise_on(job(env), c);
            0
        }
    }
}

/// `dec_to_float(env, src, sty) -> f64`.
pub extern "sysv64" fn dec_to_float(env: *mut Env, src: i64, sty: i64) -> f64 {
    let rpg = sty.cast_unsigned() & codegen::abi::TypeDesc::FSIGN != 0;
    match read_num(src, sty) {
        Ok(v) if rpg => truncated_f64(&v.value),
        Ok(v) => v.value.to_string().parse().unwrap_or(0.0),
        Err(c) => {
            raise_on(job(env), c);
            0.0
        }
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MOVE` and `MOVEL`: bytes moved right-aligned (`MOVE`) or
//! left-aligned (`MOVEL`). A number takes part as its zoned digits,
//! the sign in the last digit's zone; a numeric target takes the
//! digits of the moved characters and the sign of the last one's zone.
//! Without `(P)` the target's other positions keep their contents.

use qdecimal::{Decimal, Rounding, Zoned};

use crate::value::{Desc, Kind, bytes, read_num};

/// A value's bytes as `MOVE` sees them.
fn image(addr: u64, d: &Desc) -> Result<Vec<u8>, mi::Cond> {
    if d.numeric() {
        let v = read_num(addr, d)?;
        let digits = u8::try_from(d.len.clamp(1, 63)).unwrap_or(63);
        let z = Zoned::new(digits, d.dec).map_err(|_| mi::Cond::Mch1202)?;
        let mut out = z.encode(&v, Rounding::Truncate).map_err(|_| mi::Cond::Mch1210)?;
        // A positive value moves with the F zone.
        if !v.is_negative()
            && let Some(last) = out.last_mut()
        {
            *last = 0xF0 | (*last & 0x0F);
        }
        return Ok(out);
    }
    let b = bytes(addr, d.size());
    Ok(match d.kind {
        Kind::VarChar => {
            let n = usize::from(u16::from_be_bytes([b[0], b[1]]));
            b[2..2 + n.min(b.len() - 2)].to_vec()
        }
        _ => b.to_vec(),
    })
}

/// Move `src` into `dst`.
pub fn mov(left: bool, pad: bool, dst: u64, dd: &Desc, src: u64, sd: &Desc) -> Result<(), mi::Cond> {
    let from = image(src, sd)?;
    let mut to = if dd.numeric() {
        image(dst, dd).unwrap_or_else(|_| vec![0xF0; usize::try_from(dd.len).unwrap_or(0)])
    } else {
        image(dst, dd)?
    };
    let fill = if dd.numeric() { 0xF0 } else { 0x40 };
    let n = to.len();
    let m = from.len();
    if pad {
        to.fill(fill);
    }
    if left {
        let k = n.min(m);
        to[..k].copy_from_slice(&from[..k]);
    } else if m >= n {
        to.copy_from_slice(&from[m - n..]);
    } else {
        to[n - m..].copy_from_slice(&from);
    }
    if dd.numeric() {
        // Digits from every position; the sign from the last.
        let neg = to.last().is_some_and(|b| b >> 4 == 0xD || b >> 4 == 0xB);
        let mut digits: Vec<u8> = to.iter().map(|b| b & 0x0F).collect();
        for x in &mut digits {
            if *x > 9 {
                *x = 0;
            }
        }
        let text: String = digits.iter().map(|d| char::from(b'0' + d)).collect();
        let mut v = Decimal::parse(&text).map_err(|_| mi::Cond::Mch1202)?;
        v = v.rescale(0, Rounding::Truncate).map_err(|_| mi::Cond::Mch1202)?;
        let v = Decimal::new(v.mantissa_i128().unwrap_or(0), dd.dec).map_err(|_| mi::Cond::Mch1202)?;
        let v = if neg { v.negated() } else { v };
        return write_num(dst, dd, &v);
    }
    let out = bytes(dst, dd.size());
    match dd.kind {
        Kind::VarChar => {
            let n = usize::from(u16::from_be_bytes([out[0], out[1]]));
            out[2..2 + n].copy_from_slice(&to[..n]);
        }
        _ => out.copy_from_slice(&to),
    }
    Ok(())
}

fn write_num(addr: u64, d: &Desc, v: &Decimal) -> Result<(), mi::Cond> {
    let digits = u8::try_from(d.len.clamp(1, 63)).unwrap_or(63);
    let enc = match d.kind {
        Kind::Packed => qdecimal::Packed::new(digits, d.dec).and_then(|p| p.encode(v, Rounding::Truncate)),
        Kind::Zoned => Zoned::new(digits, d.dec).and_then(|z| z.encode(v, Rounding::Truncate)),
        Kind::Bin => qdecimal::Binary::new(digits, d.dec).and_then(|b| b.encode(v, Rounding::Truncate)),
        Kind::Int | Kind::Uns => {
            qdecimal::Integer::new(u8::try_from(d.size()).unwrap_or(8), d.kind == Kind::Int).and_then(|i| i.encode(v))
        }
        _ => return Err(mi::Cond::Mch1202),
    }
    .map_err(|_| mi::Cond::Mch1210)?;
    bytes(addr, enc.len()).copy_from_slice(&enc);
    Ok(())
}

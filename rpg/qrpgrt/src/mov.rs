// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MOVE`, `MOVEL`, `MOVEA` and the zone moves. Every operand takes
//! part as its image: character data as its bytes (a varying field as
//! its current value), an indicator as its one character, a number as
//! its zoned digits with the sign in the last zone (`F` plus, `D`
//! minus). The bytes move right-aligned (`MOVE`) or left-aligned
//! (`MOVEL`); without `(P)` the target's other positions keep their
//! contents. A numeric target then takes the digit half of each byte
//! and the sign of its last zone.

use qdecimal::{Decimal, Rounding};

use crate::value::{Desc, Kind, bytes, read_num};

/// The zoned image of a number: its digits, the sign in the last zone.
#[must_use]
pub fn num_image(v: &Decimal, d: &Desc) -> Vec<u8> {
    let n = usize::try_from(d.digits()).unwrap_or(1).max(1);
    let le: Vec<u8> = v.rescale(d.dec, Rounding::Truncate).map(|x| x.digits_le().to_vec()).unwrap_or_default();
    let mut out: Vec<u8> = (0..n).rev().map(|i| 0xF0 | le.get(i).copied().unwrap_or(0)).collect();
    if v.is_negative()
        && let Some(last) = out.last_mut()
    {
        *last = 0xD0 | (*last & 0x0F);
    }
    out
}

/// An operand's image.
pub fn image(addr: u64, d: &Desc) -> Result<Vec<u8>, mi::Cond> {
    if d.numeric() {
        if d.kind == Kind::Zoned {
            return Ok(bytes(addr, d.size()).to_vec());
        }
        let v = read_num(addr, d)?;
        return Ok(num_image(&v, d));
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

/// A number from a zoned image: the digit half of every byte, the sign
/// from the last zone (`B` and `D` are minus).
#[must_use]
pub fn from_image(img: &[u8], dec: u8) -> Decimal {
    let le: Vec<u8> = img.iter().rev().map(|b| if b & 0x0F > 9 { 0 } else { b & 0x0F }).collect();
    let neg = img.last().is_some_and(|b| matches!(b >> 4, 0xB | 0xD));
    Decimal::from_digits(neg, le, dec).unwrap_or_else(|_| Decimal::zero())
}

/// Write an image back into an operand.
pub fn store_image(addr: u64, d: &Desc, img: &[u8]) -> Result<(), mi::Cond> {
    if d.numeric() {
        // A digit half above 9 is not decimal data.
        if img.iter().any(|b| b & 0x0F > 9) {
            return Err(mi::Cond::Mch1202);
        }
        if d.kind == Kind::Zoned {
            // The digits as moved, the sign normalized to F or D.
            let out = bytes(addr, d.size());
            for (o, b) in out.iter_mut().zip(img) {
                let digit = if b & 0x0F > 9 { 0 } else { b & 0x0F };
                *o = 0xF0 | digit;
            }
            if img.last().is_some_and(|b| matches!(b >> 4, 0xB | 0xD))
                && let Some(last) = out.last_mut()
            {
                *last = 0xD0 | (*last & 0x0F);
            }
            return Ok(());
        }
        return crate::value::write_num(addr, d, &from_image(img, d.dec));
    }
    let out = bytes(addr, d.size());
    if d.kind == Kind::VarChar {
        let n = usize::from(u16::from_be_bytes([out[0], out[1]]));
        let n = n.min(img.len());
        out[2..2 + n].copy_from_slice(&img[..n]);
    } else {
        let n = out.len().min(img.len());
        out[..n].copy_from_slice(&img[..n]);
    }
    Ok(())
}

/// The padding byte of a target.
fn fill(d: &Desc) -> u8 {
    if d.numeric() { 0xF0 } else { 0x40 }
}

/// Move `src` into `dst`.
pub fn mov(left: bool, pad: bool, dst: u64, dd: &Desc, src: u64, sd: &Desc) -> Result<(), mi::Cond> {
    let from = image(src, sd)?;
    let mut to = image(dst, dd).unwrap_or_else(|_| vec![0xF0; dd.size()]);
    let n = to.len();
    let m = from.len();
    if pad {
        to.fill(fill(dd));
    }
    if left {
        let k = n.min(m);
        to[..k].copy_from_slice(&from[..k]);
        // A numeric target filled takes the sign of factor 2's last zone.
        if dd.numeric() && m >= n && n > 0 {
            to[n - 1] = (from[m - 1] & 0xF0) | (to[n - 1] & 0x0F);
        }
    } else if m >= n {
        to.copy_from_slice(&from[m - n..]);
    } else {
        to[n - m..].copy_from_slice(&from);
    }
    store_image(dst, dd, &to)
}

/// The image of `count` elements from `addr`.
fn elements(addr: u64, d: &Desc, count: usize) -> Result<Vec<u8>, mi::Cond> {
    let size = d.size() as u64;
    let mut out = Vec::new();
    for i in 0..count as u64 {
        out.extend(image(addr + i * size, d)?);
    }
    Ok(out)
}

/// `MOVEA`: the bytes of the source elements from the first one
/// named to the end, into the target's from its first one named.
pub fn movea(pad: bool, dst: u64, dd: &Desc, dcount: usize, src: u64, sd: &Desc, scount: usize) -> Result<(), mi::Cond> {
    let from = elements(src, sd, scount)?;
    let mut to = elements(dst, dd, dcount).unwrap_or_else(|_| vec![0xF0; dd.size() * dcount]);
    let k = to.len().min(from.len());
    to[..k].copy_from_slice(&from[..k]);
    if pad {
        let f = fill(dd);
        for b in &mut to[k..] {
            *b = f;
        }
    }
    let one = image(dst, dd).map_or(dd.size(), |i| i.len()).max(1);
    let size = dd.size() as u64;
    for (i, chunk) in to.chunks(one).enumerate() {
        store_image(dst + i as u64 * size, dd, chunk)?;
    }
    Ok(())
}

/// The zone moves: `MHHZO` 0, `MHLZO` 1, `MLHZO` 2, `MLLZO` 3.
pub fn zone(op: i32, dst: u64, dd: &Desc, src: u64, sd: &Desc) -> Result<(), mi::Cond> {
    let from = image(src, sd)?;
    let mut to = image(dst, dd)?;
    if from.is_empty() || to.is_empty() {
        return Ok(());
    }
    let z = if op == 0 || op == 1 { from[0] } else { from[from.len() - 1] } & 0xF0;
    let at = if op == 0 || op == 2 { 0 } else { to.len() - 1 };
    to[at] = z | (to[at] & 0x0F);
    store_image(dst, dd, &to)
}

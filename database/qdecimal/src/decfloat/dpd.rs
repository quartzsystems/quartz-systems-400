// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IEEE 754 interchange encodings with a densely packed decimal
//! coefficient, as IBM i stores `DECFLOAT`: sign bit, 5-bit combination
//! field, exponent continuation (8 or 12 bits), then 10-bit declets of
//! three digits each, the whole big-endian.

use super::{DecFloat, DfFormat, DfKind};

/// Three digits as a declet.
fn declet(d: u16) -> u16 {
    let (d2, d1, d0) = (d / 100, d / 10 % 10, d % 10);
    let bit = |x: u16, n: u16| (x >> n) & 1;
    // b c d of each digit, and whether it is 8 or 9.
    let low = |x: u16| x & 7;
    match (d2 >= 8, d1 >= 8, d0 >= 8) {
        (false, false, false) => (d2 << 7) | (d1 << 4) | d0,
        (false, false, true) => (d2 << 7) | (d1 << 4) | 0b1000 | bit(d0, 0),
        (false, true, false) => {
            (d2 << 7) | ((d0 & 6) << 4) | (bit(d1, 0) << 4) | 0b1010 | bit(d0, 0)
        }
        (true, false, false) => {
            ((d0 & 6) << 7) | (bit(d2, 0) << 7) | (low(d1) << 4) | 0b1100 | bit(d0, 0)
        }
        (true, true, false) => {
            ((d0 & 6) << 7) | (bit(d2, 0) << 7) | (bit(d1, 0) << 4) | 0b1110 | bit(d0, 0)
        }
        (true, false, true) => {
            ((d1 & 6) << 7)
                | (bit(d2, 0) << 7)
                | (0b01 << 5)
                | (bit(d1, 0) << 4)
                | 0b1110
                | bit(d0, 0)
        }
        (false, true, true) => (d2 << 7) | (0b10 << 5) | (bit(d1, 0) << 4) | 0b1110 | bit(d0, 0),
        (true, true, true) => {
            (bit(d2, 0) << 7) | (0b11 << 5) | (bit(d1, 0) << 4) | 0b1110 | bit(d0, 0)
        }
    }
}

/// A declet as three digits.
fn digits_of(x: u16) -> u16 {
    let (pqr, stu, wxy) = ((x >> 7) & 7, (x >> 4) & 7, x & 7);
    let (r, u, y) = (pqr & 1, stu & 1, wxy & 1);
    let (pq, st) = (pqr & 6, stu & 6);
    let (d2, d1, d0) = if x & 8 == 0 {
        (pqr, stu, wxy)
    } else {
        match (wxy >> 1, st >> 1) {
            (0b00, _) => (pqr, stu, 8 + y),
            (0b01, _) => (pqr, 8 + u, st | y),
            (0b10, _) => (8 + r, stu, pq | y),
            (_, 0b00) => (8 + r, 8 + u, pq | y),
            (_, 0b01) => (8 + r, pq | u, 8 + y),
            (_, 0b10) => (pqr, 8 + u, 8 + y),
            _ => (8 + r, 8 + u, 8 + y),
        }
    };
    d2 * 100 + d1 * 10 + d0
}

/// Bits of the exponent continuation and declets.
fn widths(format: DfFormat) -> (u32, u32) {
    match format {
        DfFormat::D16 => (8, 5),
        DfFormat::D34 => (12, 11),
    }
}

/// Encode.
pub fn encode(v: &DecFloat) -> Vec<u8> {
    let format = v.format;
    let (econt, declets) = widths(format);
    let total = 1 + 5 + econt + declets * 10;
    let sign = u128::from(v.neg) << (total - 1);
    let comb_shift = total - 6;
    let bits = match v.kind {
        DfKind::Infinity => sign | (0b11110 << comb_shift),
        DfKind::NaN => sign | (0b11111 << comb_shift),
        DfKind::SNaN => sign | (0b11111 << comb_shift) | (1 << (comb_shift - 1)),
        DfKind::Finite => {
            let mut c = v.coeff;
            let mut tail: u128 = 0;
            for i in 0..declets {
                let d = u16::try_from(c % 1000).expect("three digits");
                tail |= u128::from(declet(d)) << (i * 10);
                c /= 1000;
            }
            let msd = u32::try_from(c).expect("one digit");
            let e = u32::try_from(v.exp + format.bias()).expect("biased exponent");
            let top2 = e >> econt;
            let comb = if msd < 8 {
                (top2 << 3) | msd
            } else {
                0b11000 | (top2 << 1) | (msd & 1)
            };
            let rest = u128::from(e & ((1 << econt) - 1));
            sign | (u128::from(comb) << comb_shift) | (rest << (declets * 10)) | tail
        }
    };
    let bytes = bits.to_be_bytes();
    bytes[16 - format.byte_len()..].to_vec()
}

/// Decode; any bit pattern is some value.
pub fn decode(bytes: &[u8], format: DfFormat) -> DecFloat {
    let (econt, declets) = widths(format);
    let total = 1 + 5 + econt + declets * 10;
    let mut buf = [0u8; 16];
    buf[16 - bytes.len()..].copy_from_slice(bytes);
    let bits = u128::from_be_bytes(buf);
    let neg = (bits >> (total - 1)) & 1 == 1;
    let comb_shift = total - 6;
    let comb = u32::try_from((bits >> comb_shift) & 0b11111).expect("5 bits");
    if comb == 0b11110 {
        return DecFloat::special(DfKind::Infinity, neg, format);
    }
    if comb == 0b11111 {
        let kind = if (bits >> (comb_shift - 1)) & 1 == 1 {
            DfKind::SNaN
        } else {
            DfKind::NaN
        };
        return DecFloat::special(kind, neg, format);
    }
    let (top2, msd) = if comb >> 3 == 0b11 {
        ((comb >> 1) & 3, 8 + (comb & 1))
    } else {
        (comb >> 3, comb & 7)
    };
    let rest = u32::try_from((bits >> (declets * 10)) & ((1 << econt) - 1)).expect("exponent bits");
    let e = (top2 << econt) | rest;
    let mut c = u128::from(msd);
    for i in (0..declets).rev() {
        let d = u16::try_from((bits >> (i * 10)) & 0x3FF).expect("10 bits");
        c = c * 1000 + u128::from(digits_of(d));
    }
    let exp = i32::try_from(e).expect("small") - format.bias();
    DecFloat {
        neg,
        kind: DfKind::Finite,
        coeff: c,
        exp,
        format,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declets_round_trip() {
        for d in 0..1000 {
            assert_eq!(digits_of(declet(d)), d, "{d}");
        }
        assert_eq!(declet(999), 0x0FF);
        assert_eq!(declet(5), 0x005);
        assert_eq!(declet(10), 0x010);
        assert_eq!(declet(888), 0x06E);
    }
}

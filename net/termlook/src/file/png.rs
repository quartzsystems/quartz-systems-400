// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RGB PNG files: each row filtered the way that sums smallest (the
//! usual heuristic, ties to the lower filter), one `IDAT` from
//! [`super::deflate`].

// Bit, byte and pixel arithmetic on values the formats bound.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

use super::deflate;

/// An 8-bit RGB image as a PNG file. `px` holds `w * h * 3` bytes.
pub fn encode(w: usize, h: usize, px: &[u8]) -> Vec<u8> {
    let stride = w * 3;
    let mut raw = Vec::with_capacity((stride + 1) * h);
    let zero = vec![0u8; stride];
    let mut tries: [Vec<u8>; 5] = std::array::from_fn(|_| vec![0u8; stride]);
    for y in 0..h {
        let line = &px[y * stride..(y + 1) * stride];
        let prev = if y == 0 {
            &zero[..]
        } else {
            &px[(y - 1) * stride..y * stride]
        };
        let mut best = (u64::MAX, 0usize);
        for (f, out) in tries.iter_mut().enumerate() {
            let sum = filter(f, line, prev, out);
            if sum < best.0 {
                best = (sum, f);
            }
        }
        raw.push(best.1 as u8);
        raw.extend_from_slice(&tries[best.1]);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, *b"IHDR", &ihdr);
    chunk(&mut out, *b"IDAT", &deflate::zlib(&raw));
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// Filter `line` with filter `f` into `out`; the sum of the bytes as
/// signed magnitudes.
fn filter(f: usize, line: &[u8], prev: &[u8], out: &mut [u8]) -> u64 {
    let mut sum = 0u64;
    for i in 0..line.len() {
        let a = if i >= 3 { line[i - 3] } else { 0 };
        let b = prev[i];
        let c = if i >= 3 { prev[i - 3] } else { 0 };
        let p = match f {
            0 => 0,
            1 => a,
            2 => b,
            3 => a.midpoint(b),
            _ => paeth(a, b, c),
        };
        let v = line[i].wrapping_sub(p);
        out[i] = v;
        sum += u64::from(if v < 128 { v } else { 0u8.wrapping_sub(v) });
    }
    sum
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    #[test]
    fn crc_of_iend() {
        // Every PNG ends with the same IEND chunk.
        let p = super::encode(1, 1, &[1, 2, 3]);
        assert_eq!(
            &p[p.len() - 12..],
            &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        );
    }
}

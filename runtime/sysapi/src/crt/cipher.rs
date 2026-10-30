// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The MI built-in `_CIPHER(receiver, controls, source)` for its hash
//! function (5) with MD5 (algorithm 0), the one-call sequence: the
//! receiver and source arguments address pointers, the controls give
//! the function, algorithm, sequence and source length. Anything else
//! leaves the receiver as it was.

use mi::Job;

use crate::net::abi::{bound_c, mem, ptr_at};

/// MD5 (RFC 1321) of `data`.
#[must_use]
pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (f64::from(i + 1).sin().abs() * 4_294_967_296.0) as u32;
            v
        })
        .collect();
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let mut h: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let t = d;
            d = c;
            c = b;
            b = b.wrapping_add(a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(m[g]).rotate_left(S[i]));
            a = t;
        }
        h = [h[0].wrapping_add(a), h[1].wrapping_add(b), h[2].wrapping_add(c), h[3].wrapping_add(d)];
    }
    let mut out = [0u8; 16];
    for (i, w) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn cipher(j: &mut Job, receiver: u128, controls: u128, source: u128) -> i64 {
    let _ = (|| {
        let c = mem(j, controls, 32).ok()?;
        let funct = i16::from_be_bytes([c[0], c[1]]);
        let (alg, seq) = (c[2], c[3]);
        let len = usize::try_from(u32::from_be_bytes([c[4], c[5], c[6], c[7]])).ok()?;
        if funct != 5 || alg != 0 || seq != 0 {
            return None;
        }
        let src = ptr_at(mem(j, source, 16).ok()?, 0);
        let dst = ptr_at(mem(j, receiver, 16).ok()?, 0);
        let digest = md5(mem(j, src, len).ok()?);
        mem(j, dst, 16).ok()?.copy_from_slice(&digest);
        Some(())
    })();
    0
}

bound_c!(b_cipher(r: u128, c: u128, s: u128) -> i64 = cipher);

pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    vec![("_CIPHER", b_cipher as *const () as usize)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_rfc_1321() {
        let hex = |b: [u8; 16]| {
            b.iter().fold(String::new(), |mut s, x| {
                use std::fmt::Write as _;
                let _ = write!(s, "{x:02x}");
                s
            })
        };
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(md5(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }
}

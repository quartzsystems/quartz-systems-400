// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! MD4 (RFC 1320). Only for the NT hash; not a general-purpose digest.

use super::pad::md_blocks;

/// The MD4 digest of `data`.
pub fn md4(data: &[u8]) -> [u8; 16] {
    let mut s = [0x6745_2301u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    md_blocks(data, |block| compress(&mut s, block));
    let mut out = [0u8; 16];
    for (i, w) in s.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[allow(clippy::many_single_char_names)]
fn compress(s: &mut [u32; 4], block: &[u8; 64]) {
    let mut x = [0u32; 16];
    for (i, w) in x.iter_mut().enumerate() {
        *w = u32::from_le_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }
    let [mut a, mut b, mut c, mut d] = *s;
    let f = |x: u32, y: u32, z: u32| (x & y) | (!x & z);
    let g = |x: u32, y: u32, z: u32| (x & y) | (x & z) | (y & z);
    let h = |x: u32, y: u32, z: u32| x ^ y ^ z;
    for &i in &[0usize, 4, 8, 12] {
        a = a.wrapping_add(f(b, c, d)).wrapping_add(x[i]).rotate_left(3);
        d = d
            .wrapping_add(f(a, b, c))
            .wrapping_add(x[i + 1])
            .rotate_left(7);
        c = c
            .wrapping_add(f(d, a, b))
            .wrapping_add(x[i + 2])
            .rotate_left(11);
        b = b
            .wrapping_add(f(c, d, a))
            .wrapping_add(x[i + 3])
            .rotate_left(19);
    }
    for &i in &[0usize, 1, 2, 3] {
        let k = 0x5a82_7999u32;
        a = a
            .wrapping_add(g(b, c, d))
            .wrapping_add(x[i])
            .wrapping_add(k)
            .rotate_left(3);
        d = d
            .wrapping_add(g(a, b, c))
            .wrapping_add(x[i + 4])
            .wrapping_add(k)
            .rotate_left(5);
        c = c
            .wrapping_add(g(d, a, b))
            .wrapping_add(x[i + 8])
            .wrapping_add(k)
            .rotate_left(9);
        b = b
            .wrapping_add(g(c, d, a))
            .wrapping_add(x[i + 12])
            .wrapping_add(k)
            .rotate_left(13);
    }
    for &i in &[0usize, 2, 1, 3] {
        let k = 0x6ed9_eba1u32;
        a = a
            .wrapping_add(h(b, c, d))
            .wrapping_add(x[i])
            .wrapping_add(k)
            .rotate_left(3);
        d = d
            .wrapping_add(h(a, b, c))
            .wrapping_add(x[i + 8])
            .wrapping_add(k)
            .rotate_left(9);
        c = c
            .wrapping_add(h(d, a, b))
            .wrapping_add(x[i + 4])
            .wrapping_add(k)
            .rotate_left(11);
        b = b
            .wrapping_add(h(c, d, a))
            .wrapping_add(x[i + 12])
            .wrapping_add(k)
            .rotate_left(15);
    }
    s[0] = s[0].wrapping_add(a);
    s[1] = s[1].wrapping_add(b);
    s[2] = s[2].wrapping_add(c);
    s[3] = s[3].wrapping_add(d);
}

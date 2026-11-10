// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Merkle-Damgard padding MD4 and MD5 share (little-endian length).

/// Feeds `data`, padded, to `f` one 64-byte block at a time.
pub(crate) fn md_blocks(data: &[u8], mut f: impl FnMut(&[u8; 64])) {
    let (blocks, rest) = data.as_chunks::<64>();
    for c in blocks {
        f(c);
    }
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let n = if rest.len() < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[n - 8..n].copy_from_slice(&bits.to_le_bytes());
    for c in tail[..n].as_chunks::<64>().0 {
        f(c);
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HMAC-MD5 (RFC 2104).

use super::md5::md5;

/// HMAC-MD5 of the concatenation of `parts` under `key`.
pub fn hmac_md5(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..16].copy_from_slice(&md5(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let len: usize = parts.iter().map(|p| p.len()).sum();
    let mut inner = Vec::with_capacity(64 + len);
    inner.extend(k.iter().map(|b| b ^ 0x36));
    for p in parts {
        inner.extend_from_slice(p);
    }
    let ih = md5(&inner);
    let mut outer = Vec::with_capacity(80);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&ih);
    md5(&outer)
}

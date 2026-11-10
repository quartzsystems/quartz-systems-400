// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NT one-way functions ([MS-NLMP] 3.3.1, 3.3.2).

use crate::digest::{hmac_md5, md4};

/// `s` as UTF-16LE bytes.
pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// NTOWFv1: MD4 of the UTF-16LE password, the "NT hash". Password-equivalent.
pub fn nt_hash(password: &str) -> [u8; 16] {
    md4(&utf16le(password))
}

/// NTOWFv2: HMAC-MD5 under the NT hash of the upper-cased user name and the
/// domain as given, both UTF-16LE.
pub fn ntowf_v2(nt_hash: &[u8; 16], user: &str, domain: &str) -> [u8; 16] {
    let u = utf16le(&user.to_uppercase());
    let d = utf16le(domain);
    hmac_md5(nt_hash, &[&u, &d])
}

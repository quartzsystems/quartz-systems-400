// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! AES-256-GCM with a random 96-bit nonce: `nonce || ciphertext || tag`.

use graviola::aead::AesGcm;

use crate::crypto::random_array;
use crate::error::{CertError, Result};

/// Nonce bytes.
pub const NONCE: usize = 12;
/// Tag bytes.
pub const TAG: usize = 16;

/// Encrypt `plain` under `key` with `aad` authenticated.
pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let nonce: [u8; NONCE] = random_array()?;
    let mut body = plain.to_vec();
    let mut tag = [0u8; TAG];
    AesGcm::new(key).encrypt(&nonce, aad, &mut body, &mut tag);
    let mut out = Vec::with_capacity(NONCE + body.len() + TAG);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&body);
    out.extend_from_slice(&tag);
    Ok(out)
}

/// Decrypt what [`seal`] made. A wrong key or any change is `BadPassword`.
pub fn open(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < NONCE + TAG {
        return Err(CertError::Corrupt("sealed data too short".into()));
    }
    let nonce: [u8; NONCE] = sealed[..NONCE].try_into().expect("nonce");
    let (body, tag) = sealed[NONCE..].split_at(sealed.len() - NONCE - TAG);
    let mut body = body.to_vec();
    AesGcm::new(key)
        .decrypt(&nonce, aad, &mut body, tag)
        .map_err(|_| CertError::BadPassword)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper() {
        let key = [3u8; 32];
        let s = seal(&key, b"hdr", b"secret").unwrap();
        assert_eq!(open(&key, b"hdr", &s).unwrap(), b"secret");
        assert!(open(&key, b"HDR", &s).is_err());
        assert!(open(&[4u8; 32], b"hdr", &s).is_err());
        let mut t = s.clone();
        t[13] ^= 1;
        assert!(open(&key, b"hdr", &t).is_err());
        assert!(open(&key, b"", &s[..5]).is_err());
    }
}

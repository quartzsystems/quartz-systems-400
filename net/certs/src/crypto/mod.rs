// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cryptographic building blocks: randomness, AES-256-GCM sealing, key
//! derivation and the private keys. Asymmetric work and AEAD are
//! graviola's; PKCS#12's older ciphers live in `pkcs12::pbe`.

pub mod aead;
pub mod kdf;
pub mod keys;

use crate::error::Result;

/// `n` random bytes from the system's generator.
pub fn random(n: usize) -> Result<Vec<u8>> {
    let mut b = vec![0u8; n];
    graviola::random::fill(&mut b)?;
    Ok(b)
}

/// A random fixed-size array.
pub fn random_array<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    graviola::random::fill(&mut b)?;
    Ok(b)
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

/// SHA-1 of `data` (fingerprints and subject key identifiers only).
pub fn sha1(data: &[u8]) -> [u8; 20] {
    use sha1::Digest;
    sha1::Sha1::digest(data).into()
}

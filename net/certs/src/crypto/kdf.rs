// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key derivation: Argon2id for store passwords, PBKDF2 for PBES2, and
//! the PKCS#12 KDF (RFC 7292 appendix B) for PKCS#12's MAC and ciphers.

use hmac::{KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{CertError, Result};

/// Argon2id costs kept in each store's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argon2Params {
    /// Memory in KiB.
    pub m_cost: u32,
    /// Passes.
    pub t_cost: u32,
    /// Lanes.
    pub p_cost: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            m_cost: 19 * 1024,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

/// The 32-byte store key from a password and salt.
pub fn store_key(password: &str, salt: &[u8], p: Argon2Params) -> Result<Zeroizing<[u8; 32]>> {
    let params = argon2::Params::new(p.m_cost, p.t_cost, p.p_cost, Some(32))
        .map_err(|e| CertError::Corrupt(format!("argon2 parameters: {e}")))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    a.hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|e| CertError::Crypto(format!("argon2: {e}")))?;
    Ok(out)
}

/// The PRF of a PBKDF2 derivation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prf {
    /// HMAC-SHA1 (the PBKDF2 default).
    HmacSha1,
    /// HMAC-SHA256.
    HmacSha256,
}

/// PBKDF2 (RFC 8018 section 5.2).
pub fn pbkdf2(prf: Prf, password: &[u8], salt: &[u8], iterations: u32, len: usize) -> Vec<u8> {
    match prf {
        Prf::HmacSha1 => pbkdf2_with::<hmac::Hmac<Sha1>>(password, salt, iterations, len),
        Prf::HmacSha256 => pbkdf2_with::<hmac::Hmac<Sha256>>(password, salt, iterations, len),
    }
}

fn pbkdf2_with<M: Mac + KeyInit + Clone>(
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    len: usize,
) -> Vec<u8> {
    let base = <M as KeyInit>::new_from_slice(password).expect("HMAC takes any key");
    let mut out = Vec::with_capacity(len);
    let mut block = 1u32;
    while out.len() < len {
        let mut m = base.clone();
        m.update(salt);
        m.update(&block.to_be_bytes());
        let mut u = m.finalize().into_bytes().to_vec();
        let mut t = u.clone();
        for _ in 1..iterations {
            let mut m = base.clone();
            m.update(&u);
            u = m.finalize().into_bytes().to_vec();
            for (a, b) in t.iter_mut().zip(&u) {
                *a ^= b;
            }
        }
        out.extend_from_slice(&t);
        block += 1;
    }
    out.truncate(len);
    out
}

/// What a PKCS#12 KDF output is for (the "ID" byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P12Purpose {
    /// Cipher key (1).
    Key = 1,
    /// IV (2).
    Iv = 2,
    /// MAC key (3).
    Mac = 3,
}

/// The hash a PKCS#12 KDF uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P12Hash {
    /// SHA-1 (legacy files).
    Sha1,
    /// SHA-256.
    Sha256,
}

/// A password as PKCS#12 wants it: `BMPString` (UTF-16BE), two zero bytes.
pub fn bmp_password(password: &str) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::with_capacity(password.len() * 2 + 2));
    for u in password.encode_utf16() {
        out.extend_from_slice(&u.to_be_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// The PKCS#12 KDF over a BMP password.
pub fn pkcs12(
    hash: P12Hash,
    bmp_password: &[u8],
    salt: &[u8],
    purpose: P12Purpose,
    iterations: u32,
    len: usize,
) -> Vec<u8> {
    match hash {
        P12Hash::Sha1 => pkcs12_with::<Sha1>(bmp_password, salt, purpose, iterations, len),
        P12Hash::Sha256 => pkcs12_with::<Sha256>(bmp_password, salt, purpose, iterations, len),
    }
}

fn pkcs12_with<D: Digest>(
    pwd: &[u8],
    salt: &[u8],
    purpose: P12Purpose,
    iterations: u32,
    len: usize,
) -> Vec<u8> {
    // Both SHA-1 and SHA-256 have 64-byte blocks.
    const V: usize = 64;
    let hash_len = <D as Digest>::output_size();
    let diversifier = [purpose as u8; V];
    let fill = |src: &[u8]| -> Vec<u8> {
        if src.is_empty() {
            return Vec::new();
        }
        let n = V * src.len().div_ceil(V);
        src.iter().copied().cycle().take(n).collect()
    };
    let mut input = fill(salt);
    input.extend(fill(pwd));
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let mut a = D::new()
            .chain_update(diversifier)
            .chain_update(&input)
            .finalize()
            .to_vec();
        for _ in 1..iterations {
            a = D::digest(&a).to_vec();
        }
        out.extend_from_slice(&a[..hash_len.min(a.len())]);
        if out.len() >= len {
            break;
        }
        let b: Vec<u8> = a.iter().copied().cycle().take(V).collect();
        for chunk in input.chunks_mut(V) {
            // chunk = (chunk + b + 1) mod 2^(8V)
            let mut carry = 1u16;
            for k in (0..V).rev() {
                let sum = u16::from(chunk[k]) + u16::from(b[k]) + carry;
                chunk[k] = (sum & 0xFF) as u8;
                carry = sum >> 8;
            }
        }
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        use std::fmt::Write;
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
    }

    #[test]
    fn pbkdf2_rfc6070_vector() {
        // RFC 6070: "password", "salt", 2 iterations, 20 bytes.
        let k = pbkdf2(Prf::HmacSha1, b"password", b"salt", 2, 20);
        assert_eq!(hex(&k), "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957");
        // RFC 7914 section 11: PBKDF2-HMAC-SHA256, "passwd", "salt", 1, 64.
        let k = pbkdf2(Prf::HmacSha256, b"passwd", b"salt", 1, 64);
        assert_eq!(&hex(&k)[..32], "55ac046e56e3089fec1691c22544b605");
    }

    #[test]
    fn pkcs12_kdf_known_vector() {
        // A widely used vector: SHA-1, password "smeg", salt 0A58CF64530D823F,
        // ID 1, 1 iteration, 24 bytes.
        let salt = [0x0A, 0x58, 0xCF, 0x64, 0x53, 0x0D, 0x82, 0x3F];
        let k = pkcs12(
            P12Hash::Sha1,
            &bmp_password("smeg"),
            &salt,
            P12Purpose::Key,
            1,
            24,
        );
        assert_eq!(hex(&k), "8aaae6297b6cb04642ab5b077851284eb7128f1a2a7fbca3");
        let iv = pkcs12(
            P12Hash::Sha1,
            &bmp_password("smeg"),
            &salt,
            P12Purpose::Iv,
            1,
            8,
        );
        assert_eq!(hex(&iv), "79993dfe048d3b76");
    }

    #[test]
    fn store_key_depends_on_password_and_salt() {
        let p = Argon2Params {
            m_cost: 64,
            t_cost: 1,
            p_cost: 1,
        };
        let a = store_key("a", &[1u8; 16], p).unwrap();
        let b = store_key("b", &[1u8; 16], p).unwrap();
        let c = store_key("a", &[2u8; 16], p).unwrap();
        assert_ne!(*a, *b);
        assert_ne!(*a, *c);
        assert_eq!(*a, *store_key("a", &[1u8; 16], p).unwrap());
    }
}

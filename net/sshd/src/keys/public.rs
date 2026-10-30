// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Public keys in their SSH encodings (RFC 4253 6.6, RFC 5656, RFC 8709,
//! RFC 8332): parsed from blobs, checked against signatures, printed.

use graviola::hashing::{Hash, Sha256};
use graviola::signing::ecdsa::{P256, VerifyingKey as EcVerify};
use graviola::signing::eddsa::Ed25519VerifyingKey;
use graviola::signing::rsa::VerifyingKey as RsaVerify;

use crate::wire::{Reader, Writer, b64, der};

/// `ssh-ed25519`.
pub const ED25519: &str = "ssh-ed25519";
/// `ecdsa-sha2-nistp256`.
pub const ECDSA_P256: &str = "ecdsa-sha2-nistp256";
/// `ssh-rsa`, the RSA key type (signatures use the SHA-2 names).
pub const RSA: &str = "ssh-rsa";
/// `rsa-sha2-256`.
pub const RSA_SHA256: &str = "rsa-sha2-256";
/// `rsa-sha2-512`.
pub const RSA_SHA512: &str = "rsa-sha2-512";

/// Signature algorithms accepted from clients, for `server-sig-algs`.
pub const SIG_ALGS: &[&str] = &[ED25519, ECDSA_P256, RSA_SHA512, RSA_SHA256];

/// A public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicKey {
    /// Ed25519: the 32-byte point.
    Ed25519([u8; 32]),
    /// ECDSA over P-256: the uncompressed point.
    EcdsaP256(Vec<u8>),
    /// RSA: modulus and exponent, unsigned big-endian.
    Rsa {
        /// The modulus.
        n: Vec<u8>,
        /// The public exponent.
        e: Vec<u8>,
    },
}

impl PublicKey {
    /// Parse a key blob.
    #[must_use]
    pub fn from_blob(blob: &[u8]) -> Option<Self> {
        let mut r = Reader::new(blob);
        let kind = r.str().ok()?;
        let key = match kind {
            ED25519 => {
                let k = r.bytes().ok()?;
                Self::Ed25519(k.try_into().ok()?)
            }
            ECDSA_P256 => {
                if r.str().ok()? != "nistp256" {
                    return None;
                }
                let q = r.bytes().ok()?;
                if q.len() != 65 || q[0] != 4 {
                    return None;
                }
                Self::EcdsaP256(q.to_vec())
            }
            RSA => {
                let e = r.mpint().ok()?.to_vec();
                let n = r.mpint().ok()?.to_vec();
                Self::Rsa { n, e }
            }
            _ => return None,
        };
        r.is_empty().then_some(key)
    }

    /// The key type name.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ed25519(_) => ED25519,
            Self::EcdsaP256(_) => ECDSA_P256,
            Self::Rsa { .. } => RSA,
        }
    }

    /// The key blob.
    #[must_use]
    pub fn blob(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.str(self.kind());
        match self {
            Self::Ed25519(k) => {
                w.bytes(k);
            }
            Self::EcdsaP256(q) => {
                w.str("nistp256").bytes(q);
            }
            Self::Rsa { n, e } => {
                w.mpint(e).mpint(n);
            }
        }
        w.finish()
    }

    /// Whether `alg` is a signature algorithm for this key.
    #[must_use]
    pub fn signs_with(&self, alg: &str) -> bool {
        match self {
            Self::Rsa { .. } => alg == RSA_SHA256 || alg == RSA_SHA512,
            _ => alg == self.kind(),
        }
    }

    /// Check a signature blob (`string alg, string sig`) over `data`.
    #[must_use]
    pub fn verify(&self, sig_blob: &[u8], data: &[u8]) -> bool {
        let mut r = Reader::new(sig_blob);
        let (Ok(alg), Ok(sig)) = (r.str(), r.bytes()) else {
            return false;
        };
        if !self.signs_with(alg) {
            return false;
        }
        match self {
            Self::Ed25519(k) => Ed25519VerifyingKey::from_bytes(k)
                .and_then(|v| v.verify(sig, data))
                .is_ok(),
            Self::EcdsaP256(q) => {
                let mut s = Reader::new(sig);
                let (Ok(rr), Ok(ss)) = (s.mpint(), s.mpint()) else {
                    return false;
                };
                if rr.len() > 32 || ss.len() > 32 {
                    return false;
                }
                let mut fixed = [0u8; 64];
                fixed[32 - rr.len()..32].copy_from_slice(rr);
                fixed[64 - ss.len()..].copy_from_slice(ss);
                EcVerify::<P256>::from_x962_uncompressed(q)
                    .and_then(|v| v.verify::<Sha256>(&[data], &fixed))
                    .is_ok()
            }
            Self::Rsa { n, e } => {
                let Ok(v) = RsaVerify::from_pkcs1_der(&der::rsa_public(n, e)) else {
                    return false;
                };
                if alg == RSA_SHA512 {
                    v.verify_pkcs1_sha512(sig, data).is_ok()
                } else {
                    v.verify_pkcs1_sha256(sig, data).is_ok()
                }
            }
        }
    }

    /// `SHA256:` and the unpadded base64 of the blob's hash, as
    /// `ssh-keygen -l` prints.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        let h = Sha256::hash(&self.blob());
        format!("SHA256:{}", b64(h.as_ref()).trim_end_matches('='))
    }

    /// The `authorized_keys` / `.pub` form: type and base64 blob.
    #[must_use]
    pub fn openssh(&self) -> String {
        format!("{} {}", self.kind(), b64(&self.blob()))
    }
}

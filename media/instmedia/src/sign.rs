// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The manifest's signature: ed25519 by the build key over the SHA-256
//! of every line before the `signature` line, the key named by the
//! first 8 bytes of its public key's SHA-256 (the same key ID object
//! signatures use).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use optical::manifest::Manifest;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// The key ID of a public key.
#[must_use]
pub fn key_id(public: &[u8; 32]) -> [u8; 8] {
    let h: [u8; 32] = Sha256::digest(public).into();
    h[..8].try_into().expect("8")
}

fn digest(m: &Manifest) -> [u8; 32] {
    Sha256::digest(m.signed_text().as_bytes()).into()
}

/// Sign `m` with `key`.
pub fn sign(m: &mut Manifest, key: &SigningKey) {
    m.signature = None;
    let sig = key.sign(&digest(m));
    let id = key_id(&key.verifying_key().to_bytes());
    m.signature = Some((hex::encode(id), hex::encode(sig.to_bytes())));
}

/// Check `m`'s signature against `public`. A manifest without one is
/// refused.
pub fn verify(m: &Manifest, public: &[u8; 32]) -> Result<()> {
    let (id, sig) = m
        .signature
        .as_ref()
        .ok_or_else(|| Error::Signature("the manifest is not signed".into()))?;
    if *id != hex::encode(key_id(public)) {
        return Err(Error::Signature(format!(
            "signed with key {id}, not the system's key"
        )));
    }
    let bytes: [u8; 64] = hex::decode(sig)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::Signature("the signature is not 64 bytes of hex".into()))?;
    let vk = VerifyingKey::from_bytes(public).map_err(|e| Error::Signature(e.to_string()))?;
    vk.verify(&digest(m), &Signature::from_bytes(&bytes))
        .map_err(|_| Error::Signature("the manifest was changed after it was signed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_detects_changes() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = key.verifying_key().to_bytes();
        let mut m = Manifest {
            release: "V1R0M0".into(),
            volume: "QS400_V1R0M0".into(),
            ..Manifest::default()
        };
        assert!(verify(&m, &public).is_err());
        sign(&mut m, &key);
        let text = m.to_text();
        let back = Manifest::parse(&text).unwrap();
        verify(&back, &public).unwrap();
        let mut bad = back.clone();
        bad.release = "V1R1M0".into();
        assert!(verify(&bad, &public).is_err());
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes();
        assert!(verify(&back, &other).unwrap_err().to_string().contains("not the system"));
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object signatures: ed25519 over an object's content digest
//! (`docs/SAVE_FORMAT.md` §5). The build signs what it ships; a restore
//! and `CHKOBJITG` verify.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use crate::Result;
use crate::image;
use crate::save::descriptor;

/// The key identifier: the first 8 bytes of the public key's SHA-256.
#[must_use]
pub fn key_id(public: &[u8; 32]) -> [u8; 8] {
    let h: [u8; 32] = Sha256::digest(public).into();
    h[..8].try_into().expect("8")
}

/// Sign a digest.
#[must_use]
pub fn sign(key: &SigningKey, digest: &[u8; 32]) -> [u8; 64] {
    key.sign(digest).to_bytes()
}

/// Whether `signature` by `public` is good for `digest`.
#[must_use]
pub fn verify(public: &[u8; 32], digest: &[u8; 32], signature: &[u8; 64]) -> bool {
    VerifyingKey::from_bytes(public)
        .is_ok_and(|k| k.verify(digest, &Signature::from_bytes(signature)).is_ok())
}

/// The content digest object `id` has now: what its signature covers.
pub fn content_digest(txn: &mut Txn<'_>, id: ObjectId) -> Result<[u8; 32]> {
    let e = image::export(txn, id, true)?;
    let (_, h) = ops::read_header(txn, id)?;
    let d = descriptor(&h, "", &e, 0);
    Ok(savfmt::digest(&d, &e.parts))
}

/// What `CHKOBJITG` finds for one object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// Not signed.
    Unsigned,
    /// Signed, and the signature verifies.
    Good,
    /// Signed by a key the system does not know.
    UnknownKey,
    /// Signed, and the object changed since (or the signature is bad).
    Bad,
}

/// Check object `id`'s signature against the keys `known` lists.
pub fn check(txn: &mut Txn<'_>, id: ObjectId, known: &[[u8; 32]]) -> Result<Integrity> {
    let (_, h) = ops::read_header(txn, id)?;
    let Some(sig) = h.signature else {
        return Ok(Integrity::Unsigned);
    };
    let Some(key) = known.iter().find(|k| key_id(k) == sig.key_id) else {
        return Ok(Integrity::UnknownKey);
    };
    let digest = content_digest(txn, id)?;
    Ok(if verify(key, &digest, &sig.bytes) {
        Integrity::Good
    } else {
        Integrity::Bad
    })
}

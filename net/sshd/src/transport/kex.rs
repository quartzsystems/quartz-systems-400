// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The key exchange methods, the exchange hash and key derivation
//! (RFC 4253 7.2 and 8, RFC 5656 4, RFC 8731, and the ML-KEM hybrid as
//! OpenSSH defines it). Every method here hashes with SHA-256.

use graviola::hashing::{Hash, HashContext, Sha256};
use graviola::key_agreement::mlkem768::{Ciphertext, DecapKey, EncapKey};
use graviola::key_agreement::{p256, x25519};

use super::algo::{CURVE25519, CURVE25519_LIBSSH, ECDH_P256, MLKEM768X25519};
use crate::wire::Writer;

const MLKEM_PK: usize = 1184;
const MLKEM_CT: usize = 1088;

fn x25519_shared(own: x25519::PrivateKey, peer: &[u8]) -> Option<[u8; 32]> {
    let peer: [u8; 32] = peer.try_into().ok()?;
    own.diffie_hellman(&x25519::PublicKey::from_array(&peer))
        .ok()
        .map(|s| s.0)
}

fn mpint_k(k: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.mpint(k);
    w.finish()
}

fn string_k(k: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.bytes(k);
    w.finish()
}

/// The server's half: from the client's public value `q_c`, the
/// server's `q_s` and the shared secret already encoded for hashing
/// (`mpint` or `string` as the method says). `None` for a bad `q_c`.
#[must_use]
pub fn server_exchange(method: &str, q_c: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    match method {
        CURVE25519 | CURVE25519_LIBSSH => {
            let own = x25519::PrivateKey::new_random().ok()?;
            let q_s = own.public_key().as_bytes().to_vec();
            let k = x25519_shared(own, q_c)?;
            Some((q_s, mpint_k(&k)))
        }
        ECDH_P256 => {
            let peer = p256::PublicKey::from_x962_uncompressed(q_c).ok()?;
            let own = p256::PrivateKey::new_random().ok()?;
            let q_s = own.public_key_uncompressed().to_vec();
            let k = own.diffie_hellman(&peer).ok()?;
            Some((q_s, mpint_k(&k.0)))
        }
        MLKEM768X25519 => {
            if q_c.len() != MLKEM_PK + 32 {
                return None;
            }
            let pk: [u8; MLKEM_PK] = q_c[..MLKEM_PK].try_into().ok()?;
            let (kem_k, ct) = EncapKey::from_bytes(&pk).ok()?.encaps().ok()?;
            let own = x25519::PrivateKey::new_random().ok()?;
            let own_pub = own.public_key().as_bytes();
            let cl = x25519_shared(own, &q_c[MLKEM_PK..])?;
            let mut ctx = Sha256::new();
            ctx.update(kem_k.as_ref());
            ctx.update(&cl);
            let k = ctx.finish();
            let mut q_s = ct.as_ref().to_vec();
            q_s.extend_from_slice(&own_pub);
            Some((q_s, string_k(k.as_ref())))
        }
        _ => None,
    }
}

/// The client's half in progress.
pub enum ClientKex {
    /// X25519.
    X25519(x25519::PrivateKey),
    /// P-256.
    P256(p256::PrivateKey),
    /// ML-KEM-768 and X25519.
    Hybrid(Box<DecapKey>, x25519::PrivateKey),
}

impl ClientKex {
    /// Start: the state and `q_c`.
    #[must_use]
    pub fn start(method: &str) -> Option<(Self, Vec<u8>)> {
        match method {
            CURVE25519 | CURVE25519_LIBSSH => {
                let k = x25519::PrivateKey::new_random().ok()?;
                let q = k.public_key().as_bytes().to_vec();
                Some((Self::X25519(k), q))
            }
            ECDH_P256 => {
                let k = p256::PrivateKey::new_random().ok()?;
                let q = k.public_key_uncompressed().to_vec();
                Some((Self::P256(k), q))
            }
            MLKEM768X25519 => {
                let d = DecapKey::generate().ok()?;
                let x = x25519::PrivateKey::new_random().ok()?;
                let mut q = d.encapsulation_key().as_bytes().to_vec();
                q.extend_from_slice(&x.public_key().as_bytes());
                Some((Self::Hybrid(Box::new(d), x), q))
            }
            _ => None,
        }
    }

    /// Finish with the server's `q_s`: the encoded shared secret.
    #[must_use]
    pub fn finish(self, q_s: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::X25519(k) => Some(mpint_k(&x25519_shared(k, q_s)?)),
            Self::P256(k) => {
                let peer = p256::PublicKey::from_x962_uncompressed(q_s).ok()?;
                Some(mpint_k(&k.diffie_hellman(&peer).ok()?.0))
            }
            Self::Hybrid(d, x) => {
                if q_s.len() != MLKEM_CT + 32 {
                    return None;
                }
                let ct: [u8; MLKEM_CT] = q_s[..MLKEM_CT].try_into().ok()?;
                let kem_k = d.decaps(&Ciphertext::from(ct));
                let cl = x25519_shared(x, &q_s[MLKEM_CT..])?;
                let mut ctx = Sha256::new();
                ctx.update(kem_k.as_ref());
                ctx.update(&cl);
                Some(string_k(ctx.finish().as_ref()))
            }
        }
    }
}

/// The parts of the exchange hash.
pub struct HashInput<'a> {
    /// The client's identification, without CR LF.
    pub v_c: &'a [u8],
    /// The server's identification.
    pub v_s: &'a [u8],
    /// The client's `KEXINIT` payload.
    pub i_c: &'a [u8],
    /// The server's `KEXINIT` payload.
    pub i_s: &'a [u8],
    /// The host key blob.
    pub k_s: &'a [u8],
    /// The client's public value.
    pub q_c: &'a [u8],
    /// The server's public value.
    pub q_s: &'a [u8],
    /// The shared secret, encoded.
    pub k: &'a [u8],
}

/// The exchange hash `H`.
#[must_use]
pub fn exchange_hash(h: &HashInput<'_>) -> Vec<u8> {
    let mut w = Writer::new();
    for part in [h.v_c, h.v_s, h.i_c, h.i_s, h.k_s, h.q_c, h.q_s] {
        w.bytes(part);
    }
    w.raw(h.k);
    Sha256::hash(&w.finish()).as_ref().to_vec()
}

/// One derived key (RFC 4253 7.2): `letter` A to F, `len` bytes.
#[must_use]
pub fn derive(k: &[u8], h: &[u8], session_id: &[u8], letter: u8, len: usize) -> Vec<u8> {
    let mut ctx = Sha256::new();
    ctx.update(k);
    ctx.update(h);
    ctx.update(&[letter]);
    ctx.update(session_id);
    let mut out = ctx.finish().as_ref().to_vec();
    while out.len() < len {
        let mut ctx = Sha256::new();
        ctx.update(k);
        ctx.update(h);
        ctx.update(&out);
        out.extend_from_slice(ctx.finish().as_ref());
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_halves_agree() {
        for m in [CURVE25519, ECDH_P256, MLKEM768X25519] {
            let (c, q_c) = ClientKex::start(m).unwrap();
            let (q_s, k_s) = server_exchange(m, &q_c).unwrap();
            assert_eq!(c.finish(&q_s).unwrap(), k_s, "{m}");
        }
        assert!(server_exchange(CURVE25519, &[0u8; 32]).is_none());
    }
}

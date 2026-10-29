// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The store file (docs/CERTS.md):
//!
//! ```text
//! 0   magic "QS400CRT"       8
//! 8   format_version  u32    4   = 1
//! 12  header_len      u32    4
//! 16  header          JSON       kind, created, Argon2id costs, salt
//! ..  body            AES-256-GCM nonce || ciphertext || tag
//! ```
//!
//! The body is the JSON of [`Body`], sealed with the key Argon2id derives
//! from the password and the salt; the clear prefix is the AEAD's
//! associated data, so a changed header fails to open too.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::aead;
use crate::crypto::kdf::{Argon2Params, store_key};
use crate::crypto::random;
use crate::error::{CertError, Result};
use crate::store::entry::Entry;

/// File magic.
pub const MAGIC: [u8; 8] = *b"QS400CRT";
/// Current format version.
pub const FORMAT_VERSION: u32 = 1;
const PREFIX: usize = 16;

/// What a store is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreKind {
    /// Server and client certificates with their CAs (`*SYSTEM`, others).
    System,
    /// A local certificate authority.
    CertAuth,
}

/// The clear header.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// What the store is for.
    pub kind: StoreKind,
    /// Unix time of creation.
    pub created: u64,
    /// Argon2id memory cost, KiB.
    pub m_cost: u32,
    /// Argon2id passes.
    pub t_cost: u32,
    /// Argon2id lanes.
    pub p_cost: u32,
    /// Salt, hex.
    pub salt: String,
}

impl Header {
    fn params(&self) -> Argon2Params {
        Argon2Params {
            m_cost: self.m_cost,
            t_cost: self.t_cost,
            p_cost: self.p_cost,
        }
    }

    fn salt(&self) -> Result<Vec<u8>> {
        hex::decode(&self.salt).map_err(|_| CertError::Corrupt("salt".into()))
    }
}

/// The sealed body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    /// The entries, in label order.
    pub entries: Vec<Entry>,
    /// The default certificate's label (IBM's "default key").
    pub default_label: Option<String>,
    /// Unix time of the last change.
    pub changed: u64,
}

/// The decoded file: header, body and the key that opened it.
pub struct Decoded {
    /// The header.
    pub header: Header,
    /// The body.
    pub body: Body,
    /// The derived key, kept to save again.
    pub key: Zeroizing<[u8; 32]>,
}

/// A new header with a fresh salt and the given costs.
pub fn new_header(kind: StoreKind, created: u64, params: Argon2Params) -> Result<Header> {
    Ok(Header {
        kind,
        created,
        m_cost: params.m_cost,
        t_cost: params.t_cost,
        p_cost: params.p_cost,
        salt: hex::encode(random(16)?),
    })
}

/// The key a password derives for `header`.
pub fn derive(header: &Header, password: &str) -> Result<Zeroizing<[u8; 32]>> {
    store_key(password, &header.salt()?, header.params())
}

/// Encode a store.
pub fn encode(header: &Header, body: &Body, key: &[u8; 32]) -> Result<Vec<u8>> {
    let header_json = serde_json::to_vec(header).map_err(|e| CertError::Corrupt(e.to_string()))?;
    let mut out = Vec::with_capacity(PREFIX + header_json.len() + 4096);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    let len = u32::try_from(header_json.len()).map_err(|_| CertError::Corrupt("header".into()))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&header_json);
    let plain =
        Zeroizing::new(serde_json::to_vec(body).map_err(|e| CertError::Corrupt(e.to_string()))?);
    let sealed = aead::seal(key, &out, &plain)?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Split a file into its header and sealed body.
pub fn split(bytes: &[u8]) -> Result<(Header, &[u8], &[u8])> {
    if bytes.len() < PREFIX || bytes[..8] != MAGIC {
        return Err(CertError::Corrupt("not a Quartz certificate store".into()));
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("4"));
    if version != FORMAT_VERSION {
        return Err(CertError::Corrupt(format!(
            "store format version {version}"
        )));
    }
    let len = usize::try_from(u32::from_le_bytes(bytes[12..16].try_into().expect("4")))
        .map_err(|_| CertError::Corrupt("header length".into()))?;
    let end = PREFIX
        .checked_add(len)
        .filter(|e| *e <= bytes.len())
        .ok_or_else(|| CertError::Corrupt("header length".into()))?;
    let header: Header = serde_json::from_slice(&bytes[PREFIX..end])
        .map_err(|e| CertError::Corrupt(format!("header: {e}")))?;
    Ok((header, &bytes[..end], &bytes[end..]))
}

/// Decode with a password.
pub fn decode(bytes: &[u8], password: &str) -> Result<Decoded> {
    let (header, aad, sealed) = split(bytes)?;
    let key = derive(&header, password)?;
    decode_with_key(&header, aad, sealed, key)
}

/// Decode with an already derived key.
pub fn decode_with_key(
    header: &Header,
    aad: &[u8],
    sealed: &[u8],
    key: Zeroizing<[u8; 32]>,
) -> Result<Decoded> {
    let plain = Zeroizing::new(aead::open(&key, aad, sealed)?);
    let body: Body =
        serde_json::from_slice(&plain).map_err(|e| CertError::Corrupt(format!("body: {e}")))?;
    Ok(Decoded {
        header: header.clone(),
        body,
        key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn fast() -> Argon2Params {
        Argon2Params {
            m_cost: 64,
            t_cost: 1,
            p_cost: 1,
        }
    }

    #[test]
    fn round_trip_and_wrong_password() {
        let h = new_header(StoreKind::System, 5, fast()).unwrap();
        let body = Body {
            entries: vec![],
            default_label: Some("X".into()),
            changed: 9,
        };
        let key = derive(&h, "pw").unwrap();
        let bytes = encode(&h, &body, &key).unwrap();
        let d = decode(&bytes, "pw").unwrap();
        assert_eq!(d.header, h);
        assert_eq!(d.body, body);
        assert!(matches!(decode(&bytes, "no"), Err(CertError::BadPassword)));
        // A header edit breaks the seal.
        let mut t = bytes.clone();
        let pos = 16 + 5;
        t[pos] ^= 1;
        assert!(decode(&t, "pw").is_err());
        assert!(decode(b"KDB", "pw").is_err());
    }
}

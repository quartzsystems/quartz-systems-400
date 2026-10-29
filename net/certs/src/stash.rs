// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stash: a store's password sealed by the key the LIC partition
//! holds, so servers open the store without anyone typing it.
//!
//! ```text
//! 0   magic "QS400STS"   8
//! 8   version u32        4   = 1
//! 12  generation u32     4   the key slot's generation
//! 16  sealed             AES-256-GCM nonce || ciphertext || tag, AAD = [0, 16)
//! ```

use licpart::{KEY_BYTES, KeyPurpose, KeySlot, Record};
use zeroize::Zeroizing;

use crate::crypto::{aead, random_array};
use crate::error::{CertError, Result};
use crate::files::StoreFiles;
use crate::paths::stash_path;

const MAGIC: [u8; 8] = *b"QS400STS";
const VERSION: u32 = 1;

/// Seals and unseals small secrets. The system's sealer holds the LIC
/// partition's key; tests use one with a fixed key.
pub trait Sealer: Send + Sync {
    /// The key's generation, written beside the sealed data.
    fn generation(&self) -> u32;
    /// Seal `plain` with `aad` authenticated.
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>>;
    /// Unseal; `Unseal` when the key or data does not match.
    fn unseal(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>>;
}

/// A sealer over a 32-byte key: the key slot's, or a test key.
pub struct KeySealer {
    key: Zeroizing<[u8; KEY_BYTES]>,
    generation: u32,
}

impl KeySealer {
    /// From a key slot.
    pub fn from_slot(slot: &KeySlot) -> Self {
        Self {
            key: Zeroizing::new(slot.key),
            generation: slot.generation,
        }
    }

    /// From the LIC record's certificate stash slot, if it has one.
    pub fn from_record(record: &Record) -> Option<Self> {
        record.key_slot(KeyPurpose::CertStash).map(Self::from_slot)
    }

    /// A fixed key for tests and host tools.
    pub fn for_tests() -> Self {
        Self {
            key: Zeroizing::new([0x51; KEY_BYTES]),
            generation: 1,
        }
    }
}

impl Sealer for KeySealer {
    fn generation(&self) -> u32 {
        self.generation
    }

    fn seal(&self, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
        aead::seal(&self.key, aad, plain)
    }

    fn unseal(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
        aead::open(&self.key, aad, sealed).map_err(|_| CertError::Unseal)
    }
}

/// A fresh stash key slot, generation `generation`.
pub fn new_key_slot(generation: u32, now: u64) -> Result<KeySlot> {
    Ok(KeySlot {
        purpose: KeyPurpose::CertStash,
        generation,
        created: now,
        key: random_array()?,
    })
}

/// Give the record a stash key slot if it has none. Returns whether the
/// record changed (and so must be written).
pub fn ensure_key_slot(record: &mut Record, now: u64) -> Result<bool> {
    if record.key_slot(KeyPurpose::CertStash).is_some() {
        return Ok(false);
    }
    record.set_key_slot(new_key_slot(1, now)?);
    Ok(true)
}

/// Write the stash of the store at `store_path`.
pub fn write(
    files: &dyn StoreFiles,
    store_path: &str,
    password: &str,
    sealer: &dyn Sealer,
) -> Result<()> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&sealer.generation().to_le_bytes());
    let body = sealer.seal(&out, password.as_bytes())?;
    out.extend_from_slice(&body);
    files.write(&stash_path(store_path), &out)?;
    Ok(())
}

/// Read the store's password from its stash.
pub fn read(
    files: &dyn StoreFiles,
    store_path: &str,
    sealer: &dyn Sealer,
) -> Result<Zeroizing<String>> {
    let path = stash_path(store_path);
    let bytes = files.read(&path)?.ok_or(CertError::NotFound(path))?;
    if bytes.len() < 16 || bytes[..8] != MAGIC {
        return Err(CertError::Corrupt("stash".into()));
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().expect("4"));
    if version != VERSION {
        return Err(CertError::Corrupt(format!("stash version {version}")));
    }
    let generation = u32::from_le_bytes(bytes[12..16].try_into().expect("4"));
    if generation != sealer.generation() {
        return Err(CertError::Unseal);
    }
    let plain = Zeroizing::new(sealer.unseal(&bytes[..16], &bytes[16..])?);
    String::from_utf8(plain.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| CertError::Corrupt("stash text".into()))
}

/// Remove the store's stash.
pub fn remove(files: &dyn StoreFiles, store_path: &str) -> Result<()> {
    files.remove(&stash_path(store_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;

    #[test]
    fn stash_round_trips_and_needs_the_same_key() {
        let files = MemFiles::new();
        let sealer = KeySealer::for_tests();
        write(&files, "/QIBM/x/S.KDB", "s3cret", &sealer).unwrap();
        assert!(files.exists("/QIBM/x/S.RDB").unwrap());
        assert_eq!(
            read(&files, "/QIBM/x/S.KDB", &sealer).unwrap().as_str(),
            "s3cret"
        );

        let mut rec = Record::new(0);
        assert!(ensure_key_slot(&mut rec, 7).unwrap());
        assert!(!ensure_key_slot(&mut rec, 8).unwrap());
        let other = KeySealer::from_record(&rec).unwrap();
        assert!(matches!(
            read(&files, "/QIBM/x/S.KDB", &other),
            Err(CertError::Unseal)
        ));
        let mut rotated = rec.key_slot(KeyPurpose::CertStash).unwrap().clone();
        rotated.generation = 2;
        assert!(matches!(
            read(&files, "/QIBM/x/S.KDB", &KeySealer::from_slot(&rotated)),
            Err(CertError::Unseal)
        ));
        remove(&files, "/QIBM/x/S.KDB").unwrap();
        assert!(matches!(
            read(&files, "/QIBM/x/S.KDB", &sealer),
            Err(CertError::NotFound(_))
        ));
    }
}

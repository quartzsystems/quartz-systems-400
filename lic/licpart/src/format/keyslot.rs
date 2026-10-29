// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key slots: 32-byte keys held by the LIC partition, one per purpose,
//! for sealing secrets kept elsewhere (the certificate store's password).
//!
//! ```text
//! 0   slot_version  u8   1
//! 1   purpose       u8   see KeyPurpose
//! 2   generation    u32  bumped on each rotation
//! 6   created       u64  Unix time
//! 14  key           32
//! ```

use std::fmt;

use crate::error::LicError;
use crate::format::{Reader, put_u32, put_u64};

/// Current slot encoding version.
pub const KEYSLOT_VERSION: u8 = 1;
/// Bytes in a slot's key.
pub const KEY_BYTES: usize = 32;
const SLOT_LEN: usize = 14 + KEY_BYTES;

/// What a key slot's key protects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeyPurpose {
    /// Seals the certificate stores' passwords (the stash).
    CertStash,
}

impl KeyPurpose {
    fn as_u8(self) -> u8 {
        match self {
            KeyPurpose::CertStash => 1,
        }
    }

    fn from_u8(v: u8) -> Result<Self, LicError> {
        match v {
            1 => Ok(KeyPurpose::CertStash),
            _ => Err(LicError::Malformed("key slot purpose")),
        }
    }
}

/// One key slot.
#[derive(Clone, PartialEq, Eq)]
pub struct KeySlot {
    /// What the key protects.
    pub purpose: KeyPurpose,
    /// Rotation count; 1 for the first key.
    pub generation: u32,
    /// Unix time the key was made.
    pub created: u64,
    /// The key.
    pub key: [u8; KEY_BYTES],
}

impl fmt::Debug for KeySlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeySlot")
            .field("purpose", &self.purpose)
            .field("generation", &self.generation)
            .field("created", &self.created)
            .field("key", &"<redacted>")
            .finish()
    }
}

impl KeySlot {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SLOT_LEN);
        out.push(KEYSLOT_VERSION);
        out.push(self.purpose.as_u8());
        put_u32(&mut out, self.generation);
        put_u64(&mut out, self.created);
        out.extend_from_slice(&self.key);
        out
    }

    pub(crate) fn decode(data: &[u8]) -> Result<Self, LicError> {
        if data.len() != SLOT_LEN {
            return Err(LicError::Malformed("key slot length"));
        }
        let mut r = Reader::new(data);
        if r.u8()? != KEYSLOT_VERSION {
            return Err(LicError::Malformed("key slot version"));
        }
        let purpose = KeyPurpose::from_u8(r.u8()?)?;
        let generation = r.u32()?;
        let created = r.u64()?;
        let key = r.bytes(KEY_BYTES)?.try_into().expect("32 bytes");
        Ok(Self {
            purpose,
            generation,
            created,
            key,
        })
    }
}

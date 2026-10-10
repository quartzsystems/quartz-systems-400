// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 128-bit object IDs. See `docs/STORE_FORMAT.md` §2.

use std::fmt;

use crate::spec::{Layout, field};

/// The spec layout of an encoded ID.
pub static LAYOUT: Layout = Layout {
    name: "ObjectId",
    size: Some(16),
    fields: &[
        field("asp", 0, 2),
        field("type_code", 2, 2),
        field("serial", 4, 8),
        field("generation", 12, 4),
    ],
};

/// Identity of an object. Field order is sort order; the big-endian
/// encoding keeps that order byte-wise.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId {
    /// ASP number; 0 is the temporary ASP.
    pub asp: u16,
    /// Type code from the registry.
    pub type_code: u16,
    /// Monotonic per ASP, never reused.
    pub serial: u64,
    /// Bumped when the object is deleted.
    pub generation: u32,
}

impl ObjectId {
    /// "No object": marks metadata pages.
    pub const NONE: Self = Self {
        asp: 0,
        type_code: 0,
        serial: 0,
        generation: 0,
    };

    /// Whether this is [`NONE`](Self::NONE).
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }

    /// The 16-byte encoding.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0..2].copy_from_slice(&self.asp.to_be_bytes());
        b[2..4].copy_from_slice(&self.type_code.to_be_bytes());
        b[4..12].copy_from_slice(&self.serial.to_be_bytes());
        b[12..16].copy_from_slice(&self.generation.to_be_bytes());
        b
    }

    /// Decode 16 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Self {
        Self {
            asp: u16::from_be_bytes([b[0], b[1]]),
            type_code: u16::from_be_bytes([b[2], b[3]]),
            serial: u64::from_be_bytes(b[4..12].try_into().expect("8 bytes")),
            generation: u32::from_be_bytes(b[12..16].try_into().expect("4 bytes")),
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04X}-{:04X}-{:016X}-{:08X}",
            self.asp, self.type_code, self.serial, self.generation
        )
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectId({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_preserves_order() {
        let a = ObjectId {
            asp: 1,
            type_code: 2,
            serial: 300,
            generation: 1,
        };
        let b = ObjectId {
            asp: 1,
            type_code: 2,
            serial: 301,
            generation: 0,
        };
        assert!(a < b);
        assert!(a.to_bytes() < b.to_bytes());
        assert_eq!(ObjectId::from_bytes(&a.to_bytes()), a);
        assert!(ObjectId::NONE.is_none());
        assert_eq!(a.to_string(), "0001-0002-000000000000012C-00000001");
    }
}

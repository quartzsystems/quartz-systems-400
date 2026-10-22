// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MbrAttr`: the attribute area of a `*MBR`. See `docs/DB_FORMAT.md` §4.

use store::obj::ObjectId;

use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400MBR";
/// Version this code writes.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 128;
/// Header tree root slot of the access path.
pub const PATH_ROOT: usize = 0;
/// Header tree root slot of the free-slot tree.
pub const FREE_ROOT: usize = 1;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "MbrAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("flags", 10, 2),
        field("slot_len", 12, 4),
        field("record_count", 16, 8),
        field("deleted_count", 24, 8),
        field("slots", 32, 8),
        field("max_records", 40, 8),
        field("increment_slots", 48, 8),
        field("path_entries", 56, 8),
        field("key_len", 64, 2),
        field("format", 66, 2),
        field("null_map_len", 68, 2),
        field("base_count", 70, 2),
        field("file", 72, 16),
        field("last_save", 88, 8),
        field("last_restore", 96, 8),
        field("identity_next", 104, 8),
        field("reserved2", 112, 16),
    ],
};

/// The attribute area, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MbrAttr {
    /// The access path must be rebuilt before use.
    pub path_invalid: bool,
    /// `REUSEDLT(*YES)`: deleted slots are reused.
    pub reuse_deleted: bool,
    /// Bytes per record slot.
    pub slot_len: u32,
    /// Active records.
    pub record_count: u64,
    /// Deleted records still holding a slot.
    pub deleted_count: u64,
    /// Slots used.
    pub slots: u64,
    /// Most records, 0 for no limit.
    pub max_records: u64,
    /// Slots added per growth.
    pub increment_slots: u64,
    /// Entries in the access path.
    pub path_entries: u64,
    /// Encoded key bytes, 0 for arrival sequence.
    pub key_len: u16,
    /// Format the path serves.
    pub format: u16,
    /// Bytes of null map per record.
    pub null_map_len: u16,
    /// Logical member: based-on members after the attributes.
    pub base_count: u16,
    /// The owning file.
    pub file: ObjectId,
    /// Unix seconds, 0 for never.
    pub last_save: u64,
    /// Unix seconds, 0 for never.
    pub last_restore: u64,
    /// The next value of the format's identity column.
    pub identity_next: i64,
}

impl MbrAttr {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&VERSION.to_le_bytes());
        let flags = u16::from(self.path_invalid) | u16::from(self.reuse_deleted) << 1;
        b[10..12].copy_from_slice(&flags.to_le_bytes());
        b[12..16].copy_from_slice(&self.slot_len.to_le_bytes());
        b[16..24].copy_from_slice(&self.record_count.to_le_bytes());
        b[24..32].copy_from_slice(&self.deleted_count.to_le_bytes());
        b[32..40].copy_from_slice(&self.slots.to_le_bytes());
        b[40..48].copy_from_slice(&self.max_records.to_le_bytes());
        b[48..56].copy_from_slice(&self.increment_slots.to_le_bytes());
        b[56..64].copy_from_slice(&self.path_entries.to_le_bytes());
        b[64..66].copy_from_slice(&self.key_len.to_le_bytes());
        b[66..68].copy_from_slice(&self.format.to_le_bytes());
        b[68..70].copy_from_slice(&self.null_map_len.to_le_bytes());
        b[70..72].copy_from_slice(&self.base_count.to_le_bytes());
        b[72..88].copy_from_slice(&self.file.to_bytes());
        b[88..96].copy_from_slice(&self.last_save.to_le_bytes());
        b[96..104].copy_from_slice(&self.last_restore.to_le_bytes());
        b[104..112].copy_from_slice(&self.identity_next.to_le_bytes());
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| DbError::Corrupt {
            what: format!("member attributes: {what}"),
        };
        if b.len() < ATTR_LEN || &b[..8] != MAGIC {
            return Err(bad("bad magic"));
        }
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().expect("4"));
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().expect("8"));
        if u16_at(8) != VERSION {
            return Err(bad("unknown version"));
        }
        Ok(Self {
            path_invalid: u16_at(10) & 1 != 0,
            reuse_deleted: u16_at(10) & 2 != 0,
            slot_len: u32_at(12),
            record_count: u64_at(16),
            deleted_count: u64_at(24),
            slots: u64_at(32),
            max_records: u64_at(40),
            increment_slots: u64_at(48),
            path_entries: u64_at(56),
            key_len: u16_at(64),
            format: u16_at(66),
            null_map_len: u16_at(68),
            base_count: u16_at(70),
            file: ObjectId::from_bytes(&b[72..88]),
            last_save: u64_at(88),
            last_restore: u64_at(96),
            identity_next: i64::from_le_bytes(b[104..112].try_into().expect("8")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let a = MbrAttr {
            path_invalid: true,
            reuse_deleted: true,
            slot_len: 121,
            record_count: 5,
            deleted_count: 2,
            slots: 7,
            max_records: 13_000,
            increment_slots: 1000,
            path_entries: 5,
            key_len: 8,
            format: 0,
            null_map_len: 1,
            base_count: 0,
            file: ObjectId {
                asp: 1,
                type_code: 0x13,
                serial: 9,
                generation: 0,
            },
            last_save: 0,
            last_restore: 0,
            identity_next: -3,
        };
        assert_eq!(MbrAttr::decode(&a.encode()).unwrap(), a);
        assert!(MbrAttr::decode(&[0u8; ATTR_LEN]).is_err());
    }
}

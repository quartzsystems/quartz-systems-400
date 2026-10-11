// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dependents table of a physical file: logical members whose access
//! paths its members maintain. See `docs/DB_FORMAT.md` §2.

use store::obj::ObjectId;

use crate::spec::{Layout, field};

/// Bytes per entry.
pub const ENTRY_LEN: usize = 48;
/// Entries a new table has room for.
pub const INITIAL_CAP: u32 = 4;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "DependentEntry",
    size: Some(ENTRY_LEN),
    fields: &[
        field("lf_file", 0, 16),
        field("lf_member", 16, 16),
        field("base_member", 32, 16),
    ],
};

/// One dependent logical member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DependentEntry {
    /// The logical file.
    pub lf_file: ObjectId,
    /// Its member.
    pub lf_member: ObjectId,
    /// The physical member it depends on.
    pub base_member: ObjectId,
}

impl DependentEntry {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ENTRY_LEN] {
        let mut b = [0u8; ENTRY_LEN];
        b[..16].copy_from_slice(&self.lf_file.to_bytes());
        b[16..32].copy_from_slice(&self.lf_member.to_bytes());
        b[32..48].copy_from_slice(&self.base_member.to_bytes());
        b
    }

    /// Decode.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        Self {
            lf_file: ObjectId::from_bytes(&b[..16]),
            lf_member: ObjectId::from_bytes(&b[16..32]),
            base_member: ObjectId::from_bytes(&b[32..48]),
        }
    }
}

/// Encode a whole table into `cap` slots.
#[must_use]
pub fn encode_table(entries: &[DependentEntry], cap: u32) -> Vec<u8> {
    let cap = usize::try_from(cap).expect("fits");
    let mut out = vec![0u8; cap * ENTRY_LEN];
    for (i, e) in entries.iter().enumerate() {
        out[i * ENTRY_LEN..(i + 1) * ENTRY_LEN].copy_from_slice(&e.encode());
    }
    out
}

/// Decode the first `count` entries of a table.
#[must_use]
pub fn decode_table(bytes: &[u8], count: u32) -> Vec<DependentEntry> {
    (0..usize::try_from(count).expect("fits"))
        .map(|i| DependentEntry::decode(&bytes[i * ENTRY_LEN..(i + 1) * ENTRY_LEN]))
        .collect()
}

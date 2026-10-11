// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The member table in a file's byte space. See `docs/DB_FORMAT.md` §2.

use qsname::Name;
use store::obj::ObjectId;

use crate::spec::{Layout, field};

/// Bytes per entry.
pub const ENTRY_LEN: usize = 48;
/// Entries a new file's table has room for.
pub const INITIAL_CAP: u32 = 8;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "MemberEntry",
    size: Some(ENTRY_LEN),
    fields: &[
        field("name", 0, 10),
        field("srctype", 10, 10),
        field("flags", 20, 2),
        field("reserved", 22, 10),
        field("object", 32, 16),
    ],
};

/// One member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberEntry {
    /// Member name.
    pub name: Name,
    /// Source type of a source member, if any.
    pub srctype: Option<Name>,
    /// The `*MBR` object.
    pub object: ObjectId,
}

impl MemberEntry {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ENTRY_LEN] {
        let mut b = [0u8; ENTRY_LEN];
        b[..10].copy_from_slice(self.name.as_bytes());
        b[10..20].copy_from_slice(
            self.srctype
                .map_or([ccsid::SPACE; 10], |n| *n.as_bytes())
                .as_slice(),
        );
        b[32..48].copy_from_slice(&self.object.to_bytes());
        b
    }

    /// Decode.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        let srctype: [u8; 10] = b[10..20].try_into().expect("10");
        Self {
            name: Name::from_bytes(b[..10].try_into().expect("10")),
            srctype: (srctype != [ccsid::SPACE; 10]).then(|| Name::from_bytes(srctype)),
            object: ObjectId::from_bytes(&b[32..48]),
        }
    }
}

/// Encode a whole table into `cap` slots.
#[must_use]
pub fn encode_table(entries: &[MemberEntry], cap: u32) -> Vec<u8> {
    let cap = usize::try_from(cap).expect("fits");
    let mut out = vec![0u8; cap * ENTRY_LEN];
    for (i, e) in entries.iter().enumerate() {
        out[i * ENTRY_LEN..(i + 1) * ENTRY_LEN].copy_from_slice(&e.encode());
    }
    out
}

/// Decode the first `count` entries of a table.
#[must_use]
pub fn decode_table(bytes: &[u8], count: u16) -> Vec<MemberEntry> {
    (0..usize::from(count))
        .map(|i| MemberEntry::decode(&bytes[i * ENTRY_LEN..(i + 1) * ENTRY_LEN]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let e = MemberEntry {
            name: Name::parse("CUSTOMER").unwrap(),
            srctype: Some(Name::parse("PF").unwrap()),
            object: ObjectId {
                asp: 1,
                type_code: 0xFE,
                serial: 77,
                generation: 0,
            },
        };
        assert_eq!(MemberEntry::decode(&e.encode()), e);
        let plain = MemberEntry { srctype: None, ..e };
        assert_eq!(MemberEntry::decode(&plain.encode()), plain);
        let table = encode_table(&[e, plain], 4);
        assert_eq!(table.len(), 4 * ENTRY_LEN);
        assert_eq!(decode_table(&table, 2), vec![e, plain]);
    }
}

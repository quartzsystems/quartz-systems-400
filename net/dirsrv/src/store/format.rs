// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*DIRDB` attribute area and the trees' keys (docs/DIRSRV.md
//! §5.2). Frozen at V1R2M0 as fmtcheck's `dirdb` format.

use sha2::{Digest, Sha256};
use store::btree::FixedKey;
use store::spec::{Layout, field};

/// The format's version, in `DirdbAttr`.
pub const DIRDB_VERSION: u16 = 1;

/// Tree slots in the header's `tree_roots`.
pub const SLOT_ENTRIES: usize = 0;
/// The names tree.
pub const SLOT_NAMES: usize = 1;
/// The index tree.
pub const SLOT_INDEX: usize = 2;
/// The change log.
pub const SLOT_LOG: usize = 3;

/// Bytes of a record per tree value.
pub const CHUNK: usize = 1024;
/// Longest entry or change record.
pub const MAX_RECORD: usize = 16 * 1024 * 1024;
/// Suffixes a store holds.
pub const MAX_SUFFIXES: usize = 16;
/// Bytes of the attribute area `DirdbAttr` uses.
pub const ATTR_LEN: usize = 256;
/// The number the first configured index gets; 1 to 6 are always on.
pub const FIRST_CONFIGURED_INDEX: u16 = 7;

/// Bytes of a names or index key.
pub const KEY_LEN: usize = 128;
/// RDN bytes a names key holds before it hashes.
pub const NAME_RDN: usize = 118;
/// Key bytes an index key holds before it hashes.
pub const INDEX_BYTES: usize = 115;
/// The `form` of a hashed key.
pub const HASHED: u16 = 0xFFFF;
const HASH_LEN: usize = 32;

/// The spec layout of the attribute area.
pub static ATTR_LAYOUT: Layout = Layout {
    name: "DirdbAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("version", 0, 2),
        field("suffix_count", 2, 2),
        field("index_count", 4, 2),
        field("next_index", 6, 2),
        field("next_id", 8, 8),
        field("next_change", 16, 8),
        field("entry_count", 24, 8),
        field("index_keys", 32, 8),
        field("log_first", 40, 8),
        field("log_count", 48, 8),
        field("log_max", 56, 8),
        field("reserved", 64, 64),
        field("suffixes", 128, 128),
    ],
};

/// The spec layout of an entries tree key.
pub static ENTRY_KEY_LAYOUT: Layout = Layout {
    name: "EntryKey",
    size: Some(10),
    fields: &[field("entry", 0, 8), field("chunk", 8, 2)],
};

/// The spec layout of a change log key.
pub static CHANGE_KEY_LAYOUT: Layout = Layout {
    name: "ChangeKey",
    size: Some(10),
    fields: &[field("number", 0, 8), field("chunk", 8, 2)],
};

/// The spec layout of a names key.
pub static NAME_KEY_LAYOUT: Layout = Layout {
    name: "NameKey",
    size: Some(KEY_LEN),
    fields: &[
        field("parent", 0, 8),
        field("rdn", 8, NAME_RDN),
        field("form", 126, 2),
    ],
};

/// The spec layout of a names value.
pub static NAME_VALUE_LAYOUT: Layout = Layout {
    name: "NameValue",
    size: Some(8),
    fields: &[field("entry", 0, 8)],
};

/// The spec layout of an index key.
pub static INDEX_KEY_LAYOUT: Layout = Layout {
    name: "IndexKey",
    size: Some(KEY_LEN),
    fields: &[
        field("index", 0, 2),
        field("kind", 2, 1),
        field("key", 3, INDEX_BYTES),
        field("form", 118, 2),
        field("entry", 120, 8),
    ],
};

/// Little-endian reads and writes.
pub(crate) mod le {
    pub fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    pub fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
    }
    pub fn u64_at(b: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(b[at..at + 8].try_into().expect("8 bytes"))
    }
    pub fn put_u16(b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn put_u32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn put_u64(b: &mut [u8], at: usize, v: u64) {
        b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
}

/// The attribute area: counters, the change log's bounds, the suffixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirdbAttr {
    /// `DIRDB_VERSION`.
    pub version: u16,
    /// Index definitions in entry 0.
    pub index_count: u16,
    /// The number the next configured index gets.
    pub next_index: u16,
    /// The next entry ID.
    pub next_id: u64,
    /// The next change number.
    pub next_change: u64,
    /// Entries, entry 0 not counted.
    pub entry_count: u64,
    /// Keys in the index tree.
    pub index_keys: u64,
    /// The oldest change number held, 0 for none.
    pub log_first: u64,
    /// Change records held.
    pub log_count: u64,
    /// Change records kept, 0 for no limit.
    pub log_max: u64,
    /// The suffixes' root entries, in the order they were added.
    pub suffixes: Vec<u64>,
}

impl DirdbAttr {
    /// A new store's.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: DIRDB_VERSION,
            index_count: 0,
            next_index: FIRST_CONFIGURED_INDEX,
            next_id: 1,
            next_change: 1,
            entry_count: 0,
            index_keys: 0,
            log_first: 0,
            log_count: 0,
            log_max: 0,
            suffixes: Vec::new(),
        }
    }

    /// The area's bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut a = [0u8; ATTR_LEN];
        le::put_u16(&mut a, 0, self.version);
        le::put_u16(
            &mut a,
            2,
            u16::try_from(self.suffixes.len()).expect("16 suffixes"),
        );
        le::put_u16(&mut a, 4, self.index_count);
        le::put_u16(&mut a, 6, self.next_index);
        le::put_u64(&mut a, 8, self.next_id);
        le::put_u64(&mut a, 16, self.next_change);
        le::put_u64(&mut a, 24, self.entry_count);
        le::put_u64(&mut a, 32, self.index_keys);
        le::put_u64(&mut a, 40, self.log_first);
        le::put_u64(&mut a, 48, self.log_count);
        le::put_u64(&mut a, 56, self.log_max);
        for (i, s) in self.suffixes.iter().enumerate() {
            le::put_u64(&mut a, 128 + 8 * i, *s);
        }
        a
    }

    /// Read the area; `None` when it is not a version this code reads.
    #[must_use]
    pub fn decode(a: &[u8]) -> Option<Self> {
        if a.len() < ATTR_LEN || le::u16_at(a, 0) != DIRDB_VERSION {
            return None;
        }
        let n = usize::from(le::u16_at(a, 2));
        if n > MAX_SUFFIXES {
            return None;
        }
        Some(Self {
            version: DIRDB_VERSION,
            index_count: le::u16_at(a, 4),
            next_index: le::u16_at(a, 6),
            next_id: le::u64_at(a, 8),
            next_change: le::u64_at(a, 16),
            entry_count: le::u64_at(a, 24),
            index_keys: le::u64_at(a, 32),
            log_first: le::u64_at(a, 40),
            log_count: le::u64_at(a, 48),
            log_max: le::u64_at(a, 56),
            suffixes: (0..n).map(|i| le::u64_at(a, 128 + 8 * i)).collect(),
        })
    }
}

impl Default for DirdbAttr {
    fn default() -> Self {
        Self::new()
    }
}

/// An entries or change log key: a record's ID and one of its chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChunkKey {
    /// The entry ID or change number.
    pub id: u64,
    /// The chunk.
    pub chunk: u16,
}

impl ChunkKey {
    /// The first chunk of `id`.
    #[must_use]
    pub const fn first(id: u64) -> Self {
        Self { id, chunk: 0 }
    }

    /// The last possible chunk of `id`.
    #[must_use]
    pub const fn last(id: u64) -> Self {
        Self {
            id,
            chunk: u16::MAX,
        }
    }
}

impl FixedKey for ChunkKey {
    const LEN: usize = 10;
    fn write(&self, out: &mut [u8]) {
        out[..8].copy_from_slice(&self.id.to_be_bytes());
        out[8..10].copy_from_slice(&self.chunk.to_be_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        Self {
            id: u64::from_be_bytes(bytes[..8].try_into().expect("8 bytes")),
            chunk: u16::from_be_bytes([bytes[8], bytes[9]]),
        }
    }
}

/// `bytes` in `room` bytes and its form: the bytes zero padded, or their
/// first bytes and the SHA-256 of all of them.
fn fit(bytes: &[u8], room: usize, out: &mut [u8]) -> u16 {
    if bytes.len() <= room {
        out[..bytes.len()].copy_from_slice(bytes);
        u16::try_from(bytes.len()).expect("short key")
    } else {
        let keep = room - HASH_LEN;
        out[..keep].copy_from_slice(&bytes[..keep]);
        let h: [u8; HASH_LEN] = Sha256::digest(bytes).into();
        out[keep..room].copy_from_slice(&h);
        HASHED
    }
}

/// A names key: parent and RDN bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NameKey(pub [u8; KEY_LEN]);

impl NameKey {
    /// The key of `rdn` (normalized RDN bytes, or a suffix key) under
    /// `parent`.
    #[must_use]
    pub fn new(parent: u64, rdn: &[u8]) -> Self {
        let mut k = [0u8; KEY_LEN];
        k[..8].copy_from_slice(&parent.to_be_bytes());
        let form = fit(rdn, NAME_RDN, &mut k[8..8 + NAME_RDN]);
        k[126..128].copy_from_slice(&form.to_be_bytes());
        Self(k)
    }

    /// The lowest key under `parent`.
    #[must_use]
    pub fn low(parent: u64) -> Self {
        let mut k = [0u8; KEY_LEN];
        k[..8].copy_from_slice(&parent.to_be_bytes());
        Self(k)
    }

    /// The highest key under `parent`.
    #[must_use]
    pub fn high(parent: u64) -> Self {
        let mut k = [0xFFu8; KEY_LEN];
        k[..8].copy_from_slice(&parent.to_be_bytes());
        Self(k)
    }

    /// The parent's entry ID.
    #[must_use]
    pub fn parent(&self) -> u64 {
        u64::from_be_bytes(self.0[..8].try_into().expect("8 bytes"))
    }
}

impl FixedKey for NameKey {
    const LEN: usize = KEY_LEN;
    fn write(&self, out: &mut [u8]) {
        out[..KEY_LEN].copy_from_slice(&self.0);
    }
    fn read(bytes: &[u8]) -> Self {
        Self(bytes[..KEY_LEN].try_into().expect("128 bytes"))
    }
}

/// The kinds of index key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IndexKind {
    /// The attribute is present.
    Presence = 0,
    /// A value under the equality rule.
    Equality = 1,
    /// A gram of a value under the substrings rule.
    Substring = 2,
}

impl IndexKind {
    /// The `IndexDef` `kinds` bit of this kind.
    #[must_use]
    pub const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// An index key: index, kind, key bytes, entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IndexKey(pub [u8; KEY_LEN]);

impl IndexKey {
    /// The key of `entry` for `bytes` in index `index`.
    #[must_use]
    pub fn new(index: u16, kind: IndexKind, bytes: &[u8], entry: u64) -> Self {
        let mut k = [0u8; KEY_LEN];
        k[..2].copy_from_slice(&index.to_be_bytes());
        k[2] = kind as u8;
        let form = fit(bytes, INDEX_BYTES, &mut k[3..3 + INDEX_BYTES]);
        k[118..120].copy_from_slice(&form.to_be_bytes());
        k[120..128].copy_from_slice(&entry.to_be_bytes());
        Self(k)
    }

    /// The same key with `entry` in place of its own.
    #[must_use]
    pub fn with_entry(mut self, entry: u64) -> Self {
        self.0[120..128].copy_from_slice(&entry.to_be_bytes());
        self
    }

    /// The lowest key of index `index`.
    #[must_use]
    pub fn index_low(index: u16) -> Self {
        let mut k = [0u8; KEY_LEN];
        k[..2].copy_from_slice(&index.to_be_bytes());
        Self(k)
    }

    /// The highest key of index `index`.
    #[must_use]
    pub fn index_high(index: u16) -> Self {
        let mut k = [0xFFu8; KEY_LEN];
        k[..2].copy_from_slice(&index.to_be_bytes());
        Self(k)
    }

    /// The index number.
    #[must_use]
    pub fn index(&self) -> u16 {
        u16::from_be_bytes([self.0[0], self.0[1]])
    }

    /// The entry ID.
    #[must_use]
    pub fn entry(&self) -> u64 {
        u64::from_be_bytes(self.0[120..128].try_into().expect("8 bytes"))
    }
}

impl FixedKey for IndexKey {
    const LEN: usize = KEY_LEN;
    fn write(&self, out: &mut [u8]) {
        out[..KEY_LEN].copy_from_slice(&self.0);
    }
    fn read(bytes: &[u8]) -> Self {
        Self(bytes[..KEY_LEN].try_into().expect("128 bytes"))
    }
}

/// Every layout of the format, for `tests/spec.rs`.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &ATTR_LAYOUT,
        &ENTRY_KEY_LAYOUT,
        &super::record::ENTRY_HEADER_LAYOUT,
        &super::record::ATTR_HEADER_LAYOUT,
        &super::record::VALUE_HEADER_LAYOUT,
        &super::record::DEFS_HEADER_LAYOUT,
        &super::record::DEF_LAYOUT,
        &NAME_KEY_LAYOUT,
        &NAME_VALUE_LAYOUT,
        &INDEX_KEY_LAYOUT,
        &CHANGE_KEY_LAYOUT,
        &super::record::CHANGE_HEADER_LAYOUT,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_round_trips() {
        let mut a = DirdbAttr::new();
        a.suffixes = vec![3, 9];
        a.log_max = 50;
        assert_eq!(DirdbAttr::decode(&a.encode()), Some(a));
        assert!(DirdbAttr::decode(&[0u8; ATTR_LEN]).is_none());
    }

    #[test]
    fn keys_order_and_hash() {
        let a = NameKey::new(5, b"cn=a");
        let b = NameKey::new(5, b"cn=a\0");
        assert_ne!(a, b);
        assert!(a < b);
        assert!(NameKey::low(5) <= a && a <= NameKey::high(5));
        assert!(NameKey::high(4) < NameKey::low(5));
        assert_eq!(a.parent(), 5);
        let long1 = vec![b'x'; 200];
        let mut long2 = long1.clone();
        long2[199] = b'y';
        let (h1, h2) = (NameKey::new(1, &long1), NameKey::new(1, &long2));
        assert_ne!(h1, h2);
        assert_eq!(&h1.0[126..], &HASHED.to_be_bytes());
        let k = IndexKey::new(2, IndexKind::Equality, b"smith", 77);
        assert_eq!((k.index(), k.entry()), (2, 77));
        assert!(k.with_entry(0) < k && k < k.with_entry(78));
        let mut buf = [0u8; 10];
        ChunkKey { id: 3, chunk: 2 }.write(&mut buf);
        assert_eq!(ChunkKey::read(&buf), ChunkKey { id: 3, chunk: 2 });
    }
}

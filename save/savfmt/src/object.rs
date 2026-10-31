// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The per-object records, the parts' own encodings, the index and the
//! trailer.

use crate::be::{arr, get16, get32, get64, put16, put32, put64};
use crate::error::{Error, Result};
use crate::spec::{Layout, field, var};

/// "No owner" in `owner_index`.
pub const NO_OWNER: u32 = u32::MAX;

/// `ObjectDescriptor` size.
pub const DESCRIPTOR_LEN: usize = 512;
/// `ChunkHeader` size.
pub const CHUNK_LEN: usize = 24;
/// `ObjectEnd` size.
pub const END_LEN: usize = 48;
/// `SignatureRecord` size.
pub const SIGNATURE_LEN: usize = 112;
/// `IndexEntry` size.
pub const INDEX_LEN: usize = 64;
/// `Trailer` size.
pub const TRAILER_LEN: usize = 64;
/// `RefEntry` size.
pub const REF_LEN: usize = 64;
/// `AuthorityEntry` size.
pub const AUTH_LEN: usize = 16;

/// `ObjectDescriptor`.
pub static DESCRIPTOR_LAYOUT: Layout = Layout {
    name: "ObjectDescriptor",
    size: Some(DESCRIPTOR_LEN),
    fields: &[
        field("index", 0, 4),
        field("owner_index", 4, 4),
        field("type_code", 8, 2),
        field("flags", 10, 2),
        field("domain", 12, 1),
        field("state", 13, 1),
        field("reserved", 14, 2),
        field("original_id", 16, 16),
        field("library_id", 32, 16),
        field("library", 48, 10),
        field("name", 58, 10),
        field("attribute", 68, 10),
        field("text", 78, 50),
        field("owner", 128, 10),
        field("primary_group", 138, 10),
        field("creator", 148, 10),
        field("system", 158, 8),
        field("release_v", 166, 1),
        field("release_r", 167, 1),
        field("release_m", 168, 1),
        field("reserved2", 169, 7),
        field("created", 176, 8),
        field("changed", 184, 8),
        field("saved", 192, 8),
        field("restored", 200, 8),
        field("last_used", 208, 8),
        field("days_used", 216, 4),
        field("public_authority", 220, 4),
        field("autl", 224, 10),
        field("audit", 234, 10),
        field("owner_authority", 244, 4),
        field("group_authority", 248, 4),
        field("auth_flags", 252, 4),
        field("size_bytes", 256, 8),
        field("part_count", 264, 2),
        field("tail_len", 266, 2),
        field("owned_count", 268, 4),
        field("reserved3", 272, 240),
    ],
};

/// `AuthorityHeader`.
pub static AUTH_HEADER_LAYOUT: Layout = Layout {
    name: "AuthorityHeader",
    size: Some(8),
    fields: &[field("index", 0, 4), field("count", 4, 4)],
};

/// `AuthorityEntry`.
pub static AUTH_ENTRY_LAYOUT: Layout = Layout {
    name: "AuthorityEntry",
    size: Some(AUTH_LEN),
    fields: &[
        field("profile", 0, 10),
        field("bits", 10, 4),
        field("reserved", 14, 2),
    ],
};

/// `ChunkHeader`.
pub static CHUNK_LAYOUT: Layout = Layout {
    name: "ChunkHeader",
    size: Some(CHUNK_LEN),
    fields: &[
        field("index", 0, 4),
        field("part", 4, 1),
        field("reserved", 5, 3),
        field("offset", 8, 8),
        field("part_len", 16, 8),
    ],
};

/// `ObjectEnd`.
pub static END_LAYOUT: Layout = Layout {
    name: "ObjectEnd",
    size: Some(END_LEN),
    fields: &[
        field("index", 0, 4),
        field("parts", 4, 2),
        field("reserved", 6, 2),
        field("bytes", 8, 8),
        field("digest", 16, 32),
    ],
};

/// `SignatureRecord`.
pub static SIGNATURE_LAYOUT: Layout = Layout {
    name: "SignatureRecord",
    size: Some(SIGNATURE_LEN),
    fields: &[
        field("index", 0, 4),
        field("algorithm", 4, 2),
        field("reserved", 6, 2),
        field("key_id", 8, 8),
        field("signature", 16, 64),
        field("digest", 80, 32),
    ],
};

/// `TreeImage`.
pub static TREE_LAYOUT: Layout = Layout {
    name: "TreeImage",
    size: None,
    fields: &[
        field("key_len", 0, 2),
        field("reserved", 2, 2),
        field("count", 4, 4),
        var("entries", 8),
    ],
};

/// `RefEntry`.
pub static REF_LAYOUT: Layout = Layout {
    name: "RefEntry",
    size: Some(REF_LEN),
    fields: &[
        field("original_id", 0, 16),
        field("type_code", 16, 2),
        field("region", 18, 2),
        field("library", 20, 10),
        field("name", 30, 10),
        field("member", 40, 10),
        field("offset", 50, 8),
        field("flags", 58, 2),
        field("reserved", 60, 4),
    ],
};

/// `AuthTableEntry`: a row of a profile's authority table (the engine
/// writes it as a `*USRPRF`'s typed part).
pub static AUTH_TABLE_LAYOUT: Layout = Layout {
    name: "AuthTableEntry",
    size: Some(36),
    fields: &[
        field("library", 0, 10),
        field("name", 10, 10),
        field("member", 20, 10),
        field("type_code", 30, 2),
        field("bits", 32, 4),
    ],
};

/// `IndexEntry`.
pub static INDEX_LAYOUT: Layout = Layout {
    name: "IndexEntry",
    size: Some(INDEX_LEN),
    fields: &[
        field("index", 0, 4),
        field("owner_index", 4, 4),
        field("offset", 8, 8),
        field("type_code", 16, 2),
        field("flags", 18, 2),
        field("library", 20, 10),
        field("name", 30, 10),
        field("attribute", 40, 10),
        field("reserved", 50, 6),
        field("size_bytes", 56, 8),
    ],
};

/// `Trailer`.
pub static TRAILER_LAYOUT: Layout = Layout {
    name: "Trailer",
    size: Some(TRAILER_LEN),
    fields: &[
        field("objects", 0, 4),
        field("not_saved", 4, 4),
        field("index_offset", 8, 8),
        field("records", 16, 4),
        field("index_records", 20, 4),
        field("bytes", 24, 8),
        field("stream_id", 32, 16),
        field("reserved", 48, 16),
    ],
};

/// A part kind (`SAVE_FORMAT.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Part(pub u8);

impl Part {
    /// The attribute area.
    pub const ATTR: Part = Part(1);
    /// The byte space.
    pub const SPACE: Part = Part(2);
    /// References by name.
    pub const REFS: Part = Part(7);
    /// An output queue's spooled files.
    pub const SPOOL: Part = Part(8);
    /// An IFS object's metadata record.
    pub const IFS_META: Part = Part(9);
    /// An IFS object's data.
    pub const IFS_DATA: Part = Part(10);

    /// The tree in header slot `slot` (0..4).
    #[must_use]
    pub fn tree(slot: usize) -> Part {
        assert!(slot < 4, "four tree slots");
        Part(3 + u8::try_from(slot).expect("small"))
    }

    /// A type's own export `n` (0..16).
    #[must_use]
    pub fn typed(n: u8) -> Part {
        assert!(n < 16, "sixteen typed parts");
        Part(16 + n)
    }
}

/// Descriptor flag bits.
pub mod flags {
    /// The program adopts its owner's authority.
    pub const ADOPTS: u16 = 1;
    /// A signature record follows.
    pub const SIGNED: u16 = 1 << 1;
    /// An IFS object; the tail is its path.
    pub const IFS: u16 = 1 << 2;
    /// An authorities record follows.
    pub const PRIVATE_AUTH: u16 = 1 << 3;
    /// The data was not saved.
    pub const NO_DATA: u16 = 1 << 4;
    /// Storage was freed by the save.
    pub const FREED: u16 = 1 << 5;
}

/// An object's descriptor. Name fields are kept as their CP037 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct ObjectDescriptor {
    pub index: u32,
    pub owner_index: u32,
    pub type_code: u16,
    pub flags: u16,
    pub domain: u8,
    pub state: u8,
    pub original_id: [u8; 16],
    pub library_id: [u8; 16],
    pub library: [u8; 10],
    pub name: [u8; 10],
    pub attribute: [u8; 10],
    pub text: [u8; 50],
    pub owner: [u8; 10],
    pub primary_group: [u8; 10],
    pub creator: [u8; 10],
    pub system: [u8; 8],
    pub release: (u8, u8, u8),
    pub created: u64,
    pub changed: u64,
    pub saved: u64,
    pub restored: u64,
    pub last_used: u64,
    pub days_used: u32,
    pub public_authority: u32,
    pub autl: [u8; 10],
    pub audit: [u8; 10],
    pub owner_authority: u32,
    pub group_authority: u32,
    pub auth_flags: u32,
    pub size_bytes: u64,
    pub part_count: u16,
    pub tail_len: u16,
    pub owned_count: u32,
}

impl Default for ObjectDescriptor {
    fn default() -> Self {
        let blank10 = [0x40; 10];
        Self {
            index: 0,
            owner_index: NO_OWNER,
            type_code: 0,
            flags: 0,
            domain: 1,
            state: 1,
            original_id: [0; 16],
            library_id: [0; 16],
            library: blank10,
            name: blank10,
            attribute: blank10,
            text: [0x40; 50],
            owner: blank10,
            primary_group: blank10,
            creator: blank10,
            system: [0x40; 8],
            release: (0, 0, 0),
            created: 0,
            changed: 0,
            saved: 0,
            restored: 0,
            last_used: 0,
            days_used: 0,
            public_authority: 0,
            autl: blank10,
            audit: blank10,
            owner_authority: 0,
            group_authority: 0,
            auth_flags: 0,
            size_bytes: 0,
            part_count: 0,
            tail_len: 0,
            owned_count: 0,
        }
    }
}

impl ObjectDescriptor {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; DESCRIPTOR_LEN] {
        let mut b = [0u8; DESCRIPTOR_LEN];
        put32(&mut b, 0, self.index);
        put32(&mut b, 4, self.owner_index);
        put16(&mut b, 8, self.type_code);
        put16(&mut b, 10, self.flags);
        b[12] = self.domain;
        b[13] = self.state;
        b[16..32].copy_from_slice(&self.original_id);
        b[32..48].copy_from_slice(&self.library_id);
        b[48..58].copy_from_slice(&self.library);
        b[58..68].copy_from_slice(&self.name);
        b[68..78].copy_from_slice(&self.attribute);
        b[78..128].copy_from_slice(&self.text);
        b[128..138].copy_from_slice(&self.owner);
        b[138..148].copy_from_slice(&self.primary_group);
        b[148..158].copy_from_slice(&self.creator);
        b[158..166].copy_from_slice(&self.system);
        b[166] = self.release.0;
        b[167] = self.release.1;
        b[168] = self.release.2;
        put64(&mut b, 176, self.created);
        put64(&mut b, 184, self.changed);
        put64(&mut b, 192, self.saved);
        put64(&mut b, 200, self.restored);
        put64(&mut b, 208, self.last_used);
        put32(&mut b, 216, self.days_used);
        put32(&mut b, 220, self.public_authority);
        b[224..234].copy_from_slice(&self.autl);
        b[234..244].copy_from_slice(&self.audit);
        put32(&mut b, 244, self.owner_authority);
        put32(&mut b, 248, self.group_authority);
        put32(&mut b, 252, self.auth_flags);
        put64(&mut b, 256, self.size_bytes);
        put16(&mut b, 264, self.part_count);
        put16(&mut b, 266, self.tail_len);
        put32(&mut b, 268, self.owned_count);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < DESCRIPTOR_LEN {
            return Err(Error::Truncated);
        }
        Ok(Self {
            index: get32(b, 0),
            owner_index: get32(b, 4),
            type_code: get16(b, 8),
            flags: get16(b, 10),
            domain: b[12],
            state: b[13],
            original_id: arr(b, 16),
            library_id: arr(b, 32),
            library: arr(b, 48),
            name: arr(b, 58),
            attribute: arr(b, 68),
            text: arr(b, 78),
            owner: arr(b, 128),
            primary_group: arr(b, 138),
            creator: arr(b, 148),
            system: arr(b, 158),
            release: (b[166], b[167], b[168]),
            created: get64(b, 176),
            changed: get64(b, 184),
            saved: get64(b, 192),
            restored: get64(b, 200),
            last_used: get64(b, 208),
            days_used: get32(b, 216),
            public_authority: get32(b, 220),
            autl: arr(b, 224),
            audit: arr(b, 234),
            owner_authority: get32(b, 244),
            group_authority: get32(b, 248),
            auth_flags: get32(b, 252),
            size_bytes: get64(b, 256),
            part_count: get16(b, 264),
            tail_len: get16(b, 266),
            owned_count: get32(b, 268),
        })
    }

    /// The library name as text.
    #[must_use]
    pub fn library_name(&self) -> String {
        crate::text(&self.library)
    }

    /// The object name as text.
    #[must_use]
    pub fn object_name(&self) -> String {
        crate::text(&self.name)
    }

    /// Whether a flag bit is set.
    #[must_use]
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }
}

/// One private authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityEntry {
    /// Profile name, CP037.
    pub profile: [u8; 10],
    /// The store's authority bits.
    pub bits: u32,
}

/// Encode an authorities payload.
#[must_use]
pub fn encode_auths(index: u32, auths: &[AuthorityEntry]) -> Vec<u8> {
    let mut b = vec![0u8; 8 + auths.len() * AUTH_LEN];
    put32(&mut b, 0, index);
    put32(&mut b, 4, u32::try_from(auths.len()).expect("count"));
    for (i, a) in auths.iter().enumerate() {
        let at = 8 + i * AUTH_LEN;
        b[at..at + 10].copy_from_slice(&a.profile);
        put32(&mut b, at + 10, a.bits);
    }
    b
}

/// Decode an authorities payload: the object index and the entries.
pub fn decode_auths(b: &[u8]) -> Result<(u32, Vec<AuthorityEntry>)> {
    if b.len() < 8 {
        return Err(Error::Truncated);
    }
    let n = get32(b, 4) as usize;
    if b.len() != 8 + n * AUTH_LEN {
        return Err(Error::Truncated);
    }
    let auths = (0..n)
        .map(|i| {
            let at = 8 + i * AUTH_LEN;
            AuthorityEntry {
                profile: arr(b, at),
                bits: get32(b, at + 10),
            }
        })
        .collect();
    Ok((get32(b, 0), auths))
}

/// A chunk header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    /// Object.
    pub index: u32,
    /// Part.
    pub part: Part,
    /// Offset in the part.
    pub offset: u64,
    /// The part's length.
    pub part_len: u64,
}

impl ChunkHeader {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; CHUNK_LEN] {
        let mut b = [0u8; CHUNK_LEN];
        put32(&mut b, 0, self.index);
        b[4] = self.part.0;
        put64(&mut b, 8, self.offset);
        put64(&mut b, 16, self.part_len);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < CHUNK_LEN {
            return Err(Error::Truncated);
        }
        Ok(Self {
            index: get32(b, 0),
            part: Part(b[4]),
            offset: get64(b, 8),
            part_len: get64(b, 16),
        })
    }
}

/// An object's end record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectEnd {
    /// Object.
    pub index: u32,
    /// Parts written.
    pub parts: u16,
    /// Raw bytes of every part.
    pub bytes: u64,
    /// The object's digest.
    pub digest: [u8; 32],
}

impl ObjectEnd {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; END_LEN] {
        let mut b = [0u8; END_LEN];
        put32(&mut b, 0, self.index);
        put16(&mut b, 4, self.parts);
        put64(&mut b, 8, self.bytes);
        b[16..48].copy_from_slice(&self.digest);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != END_LEN {
            return Err(Error::Truncated);
        }
        Ok(Self {
            index: get32(b, 0),
            parts: get16(b, 4),
            bytes: get64(b, 8),
            digest: arr(b, 16),
        })
    }
}

/// An object's signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureRecord {
    /// Object.
    pub index: u32,
    /// 1 ed25519.
    pub algorithm: u16,
    /// Key identifier.
    pub key_id: [u8; 8],
    /// The signature.
    pub signature: [u8; 64],
    /// What it signs.
    pub digest: [u8; 32],
}

impl SignatureRecord {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; SIGNATURE_LEN] {
        let mut b = [0u8; SIGNATURE_LEN];
        put32(&mut b, 0, self.index);
        put16(&mut b, 4, self.algorithm);
        b[8..16].copy_from_slice(&self.key_id);
        b[16..80].copy_from_slice(&self.signature);
        b[80..112].copy_from_slice(&self.digest);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != SIGNATURE_LEN {
            return Err(Error::Truncated);
        }
        Ok(Self {
            index: get32(b, 0),
            algorithm: get16(b, 4),
            key_id: arr(b, 8),
            signature: arr(b, 16),
            digest: arr(b, 80),
        })
    }
}

/// A tree's entries, as a part.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeImage {
    /// Bytes per key.
    pub key_len: u16,
    /// Key and value, in key order.
    pub entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl TreeImage {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; 8];
        put16(&mut b, 0, self.key_len);
        put32(&mut b, 4, u32::try_from(self.entries.len()).expect("count"));
        for (k, v) in &self.entries {
            debug_assert_eq!(k.len(), usize::from(self.key_len));
            b.extend_from_slice(k);
            b.extend_from_slice(&u16::try_from(v.len()).expect("value").to_be_bytes());
            b.extend_from_slice(v);
        }
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < 8 {
            return Err(Error::Truncated);
        }
        let key_len = get16(b, 0);
        let n = get32(b, 4) as usize;
        let k = usize::from(key_len);
        let mut at = 8;
        let mut entries = Vec::with_capacity(n.min(b.len() / (k + 2).max(1)));
        for _ in 0..n {
            if at + k + 2 > b.len() {
                return Err(Error::Truncated);
            }
            let key = b[at..at + k].to_vec();
            let vl = usize::from(get16(b, at + k));
            at += k + 2;
            if at + vl > b.len() {
                return Err(Error::Truncated);
            }
            entries.push((key, b[at..at + vl].to_vec()));
            at += vl;
        }
        if at != b.len() {
            return Err(Error::Truncated);
        }
        Ok(Self { key_len, entries })
    }
}

/// A reference to another object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefEntry {
    /// Its ID on the saving system.
    pub original_id: [u8; 16],
    /// Its type.
    pub type_code: u16,
    /// Where it sits: 1 the attribute area, 2 the byte space.
    pub region: u16,
    /// Library name, CP037.
    pub library: [u8; 10],
    /// Name, CP037.
    pub name: [u8; 10],
    /// Member name for a `*MBR`, blank otherwise.
    pub member: [u8; 10],
    /// Byte offset of the ID within the region.
    pub offset: u64,
    /// [`RefEntry::IN_STREAM`], [`RefEntry::EARLIER`].
    pub flags: u16,
}

impl RefEntry {
    /// The object is in the same stream.
    pub const IN_STREAM: u16 = 1;
    /// It comes earlier in the stream than the object referring to it.
    pub const EARLIER: u16 = 2;
}

/// Encode a refs part.
#[must_use]
pub fn encode_refs(refs: &[RefEntry]) -> Vec<u8> {
    let mut b = vec![0u8; refs.len() * REF_LEN];
    for (i, r) in refs.iter().enumerate() {
        let at = i * REF_LEN;
        b[at..at + 16].copy_from_slice(&r.original_id);
        put16(&mut b, at + 16, r.type_code);
        put16(&mut b, at + 18, r.region);
        b[at + 20..at + 30].copy_from_slice(&r.library);
        b[at + 30..at + 40].copy_from_slice(&r.name);
        b[at + 40..at + 50].copy_from_slice(&r.member);
        put64(&mut b, at + 50, r.offset);
        put16(&mut b, at + 58, r.flags);
    }
    b
}

/// Decode a refs part.
pub fn decode_refs(b: &[u8]) -> Result<Vec<RefEntry>> {
    if !b.len().is_multiple_of(REF_LEN) {
        return Err(Error::Truncated);
    }
    Ok(b.as_chunks::<REF_LEN>()
        .0
        .iter()
        .map(|c| RefEntry {
            original_id: arr(c, 0),
            type_code: get16(c, 16),
            region: get16(c, 18),
            library: arr(c, 20),
            name: arr(c, 30),
            member: arr(c, 40),
            offset: get64(c, 50),
            flags: get16(c, 58),
        })
        .collect())
}

/// One index entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct IndexEntry {
    pub index: u32,
    pub owner_index: u32,
    pub offset: u64,
    pub type_code: u16,
    pub flags: u16,
    pub library: [u8; 10],
    pub name: [u8; 10],
    pub attribute: [u8; 10],
    pub size_bytes: u64,
}

impl IndexEntry {
    /// The entry for a descriptor written at `offset`.
    #[must_use]
    pub fn of(d: &ObjectDescriptor, offset: u64) -> Self {
        Self {
            index: d.index,
            owner_index: d.owner_index,
            offset,
            type_code: d.type_code,
            flags: d.flags,
            library: d.library,
            name: d.name,
            attribute: d.attribute,
            size_bytes: d.size_bytes,
        }
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; INDEX_LEN] {
        let mut b = [0u8; INDEX_LEN];
        put32(&mut b, 0, self.index);
        put32(&mut b, 4, self.owner_index);
        put64(&mut b, 8, self.offset);
        put16(&mut b, 16, self.type_code);
        put16(&mut b, 18, self.flags);
        b[20..30].copy_from_slice(&self.library);
        b[30..40].copy_from_slice(&self.name);
        b[40..50].copy_from_slice(&self.attribute);
        put64(&mut b, 56, self.size_bytes);
        b
    }

    /// Decode one entry.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        Self {
            index: get32(b, 0),
            owner_index: get32(b, 4),
            offset: get64(b, 8),
            type_code: get16(b, 16),
            flags: get16(b, 18),
            library: arr(b, 20),
            name: arr(b, 30),
            attribute: arr(b, 40),
            size_bytes: get64(b, 56),
        }
    }
}

/// The trailer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Trailer {
    pub objects: u32,
    pub not_saved: u32,
    pub index_offset: u64,
    pub records: u32,
    pub index_records: u32,
    pub bytes: u64,
    pub stream_id: [u8; 16],
}

impl Trailer {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; TRAILER_LEN] {
        let mut b = [0u8; TRAILER_LEN];
        put32(&mut b, 0, self.objects);
        put32(&mut b, 4, self.not_saved);
        put64(&mut b, 8, self.index_offset);
        put32(&mut b, 16, self.records);
        put32(&mut b, 20, self.index_records);
        put64(&mut b, 24, self.bytes);
        b[32..48].copy_from_slice(&self.stream_id);
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != TRAILER_LEN {
            return Err(Error::Truncated);
        }
        Ok(Self {
            objects: get32(b, 0),
            not_saved: get32(b, 4),
            index_offset: get64(b, 8),
            records: get32(b, 16),
            index_records: get32(b, 20),
            bytes: get64(b, 24),
            stream_id: arr(b, 32),
        })
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The record header in front of every record after the media header.

use crc::{CRC_32_ISCSI, Crc, Table};

use crate::be::{get32, put32};
use crate::spec::{Layout, field};

const CRC32C: Crc<u32, Table<16>> = Crc::<u32, Table<16>>::new(&CRC_32_ISCSI);

/// CRC32C of `b`.
#[must_use]
pub fn crc32c(b: &[u8]) -> u32 {
    CRC32C.checksum(b)
}

/// `RecordHeader` size.
pub const LEN: usize = 24;
/// The resynchronization mark.
pub const MARK: [u8; 2] = *b"QR";
/// Largest raw payload of one record.
pub const MAX_RAW: usize = 1 << 20;

/// `RecordHeader`.
pub static LAYOUT: Layout = Layout {
    name: "RecordHeader",
    size: Some(LEN),
    fields: &[
        field("mark", 0, 2),
        field("kind", 2, 1),
        field("codec", 3, 1),
        field("seq", 4, 4),
        field("stored_len", 8, 4),
        field("raw_len", 12, 4),
        field("crc", 16, 4),
        field("header_crc", 20, 4),
    ],
};

/// Record kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// The command and its parameters.
    Parameters,
    /// An object's descriptor.
    Object,
    /// An object's private authorities.
    Authorities,
    /// A chunk of a part.
    Data,
    /// The end of an object.
    ObjectEnd,
    /// An object's signature.
    Signature,
    /// Object index entries.
    Index,
    /// The trailer.
    Trailer,
}

impl RecordKind {
    /// The header code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Parameters => 0x01,
            Self::Object => 0x10,
            Self::Authorities => 0x11,
            Self::Data => 0x12,
            Self::ObjectEnd => 0x13,
            Self::Signature => 0x14,
            Self::Index => 0x20,
            Self::Trailer => 0x21,
        }
    }

    /// From a header code.
    #[must_use]
    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            0x01 => Self::Parameters,
            0x10 => Self::Object,
            0x11 => Self::Authorities,
            0x12 => Self::Data,
            0x13 => Self::ObjectEnd,
            0x14 => Self::Signature,
            0x20 => Self::Index,
            0x21 => Self::Trailer,
            _ => return None,
        })
    }
}

/// A decoded record header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    /// Kind.
    pub kind: RecordKind,
    /// Payload codec.
    pub codec: u8,
    /// Record number.
    pub seq: u32,
    /// Stored payload bytes.
    pub stored_len: u32,
    /// Raw payload bytes.
    pub raw_len: u32,
    /// CRC32C of the stored payload.
    pub crc: u32,
}

impl RecordHeader {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; LEN] {
        let mut b = [0u8; LEN];
        b[0..2].copy_from_slice(&MARK);
        b[2] = self.kind.code();
        b[3] = self.codec;
        put32(&mut b, 4, self.seq);
        put32(&mut b, 8, self.stored_len);
        put32(&mut b, 12, self.raw_len);
        put32(&mut b, 16, self.crc);
        let hc = crc32c(&b[..20]);
        put32(&mut b, 20, hc);
        b
    }

    /// Decode; `None` when the mark, kind, lengths or header CRC are wrong.
    #[must_use]
    pub fn decode(b: &[u8; LEN]) -> Option<Self> {
        if b[0..2] != MARK || crc32c(&b[..20]) != get32(b, 20) {
            return None;
        }
        let h = Self {
            kind: RecordKind::from_code(b[2])?,
            codec: b[3],
            seq: get32(b, 4),
            stored_len: get32(b, 8),
            raw_len: get32(b, 12),
            crc: get32(b, 16),
        };
        let limit = u32::try_from(MAX_RAW + crate::object::CHUNK_LEN).expect("fits");
        (h.raw_len <= limit && h.stored_len <= limit + limit / 8 + 64).then_some(h)
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media header: the first 128 bytes of every stream.

use crate::be::{arr, get16, get32, get64, put16, put32, put64};
use crate::codec::Compression;
use crate::error::{Error, Result};
use crate::record::crc32c;
use crate::spec::{Layout, field};

/// `MediaHeader` size.
pub const LEN: usize = 128;
/// The magic.
pub const MAGIC: &[u8; 8] = b"QS400SAV";
/// The format version this code writes and reads.
pub const VERSION: u16 = 1;

/// `MediaHeader`.
pub static LAYOUT: Layout = Layout {
    name: "MediaHeader",
    size: Some(LEN),
    fields: &[
        field("magic", 0, 8),
        field("format_version", 8, 2),
        field("flags", 10, 2),
        field("release_v", 12, 1),
        field("release_r", 13, 1),
        field("release_m", 14, 1),
        field("target_v", 15, 1),
        field("target_r", 16, 1),
        field("target_m", 17, 1),
        field("compression", 18, 1),
        field("kind", 19, 1),
        field("reserved", 20, 4),
        field("created", 24, 8),
        field("stream_id", 32, 16),
        field("system", 48, 8),
        field("serial", 56, 10),
        field("command", 66, 10),
        field("user", 76, 10),
        field("reserved2", 86, 38),
        field("crc", 124, 4),
    ],
};

/// What a stream holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKind {
    /// `SAVLIB`.
    Library,
    /// `SAVOBJ`.
    Objects,
    /// `SAVCHGOBJ`.
    Changed,
    /// `SAV`.
    Ifs,
    /// `SAVSYS`.
    System,
    /// `SAVSECDTA`.
    Security,
    /// `SAVCFG`.
    Config,
    /// `SAVLICPGM`.
    LicPgm,
    /// A PTF.
    Ptf,
    /// `SAVSAVFDTA`.
    SavfData,
}

impl SaveKind {
    const ALL: [SaveKind; 10] = [
        Self::Library,
        Self::Objects,
        Self::Changed,
        Self::Ifs,
        Self::System,
        Self::Security,
        Self::Config,
        Self::LicPgm,
        Self::Ptf,
        Self::SavfData,
    ];

    /// The code in the header.
    #[must_use]
    pub fn code(self) -> u8 {
        u8::try_from(Self::ALL.iter().position(|k| *k == self).expect("listed")).expect("small")
    }

    /// The kind with `code`.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }
}

/// The media header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaHeader {
    /// Saving release (V, R, M).
    pub release: (u8, u8, u8),
    /// `TGTRLS`.
    pub target: (u8, u8, u8),
    /// `DTACPR`.
    pub compression: Compression,
    /// What the stream holds.
    pub kind: SaveKind,
    /// Unix seconds when the save began.
    pub created: u64,
    /// Unique per save.
    pub stream_id: [u8; 16],
    /// Saving system's name.
    pub system: String,
    /// Saving system's serial number.
    pub serial: String,
    /// The save command's name.
    pub command: String,
    /// The user who saved.
    pub user: String,
}

impl MediaHeader {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; LEN] {
        let mut b = [0u8; LEN];
        b[0..8].copy_from_slice(MAGIC);
        put16(&mut b, 8, VERSION);
        b[12] = self.release.0;
        b[13] = self.release.1;
        b[14] = self.release.2;
        b[15] = self.target.0;
        b[16] = self.target.1;
        b[17] = self.target.2;
        b[18] = self.compression.code();
        b[19] = self.kind.code();
        put64(&mut b, 24, self.created);
        b[32..48].copy_from_slice(&self.stream_id);
        b[48..56].copy_from_slice(&crate::fixed::<8>(&self.system));
        b[56..66].copy_from_slice(&crate::fixed::<10>(&self.serial));
        b[66..76].copy_from_slice(&crate::fixed::<10>(&self.command));
        b[76..86].copy_from_slice(&crate::fixed::<10>(&self.user));
        let crc = crc32c(&b[..124]);
        put32(&mut b, 124, crc);
        b
    }

    /// Decode, checking magic, version and checksum.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < LEN || &b[0..8] != MAGIC {
            return Err(Error::NotSave);
        }
        if crc32c(&b[..124]) != get32(b, 124) {
            return Err(Error::HeaderDamaged);
        }
        let version = get16(b, 8);
        if version != VERSION {
            return Err(Error::Version(version));
        }
        Ok(Self {
            release: (b[12], b[13], b[14]),
            target: (b[15], b[16], b[17]),
            compression: Compression::from_code(b[18]).ok_or(Error::HeaderDamaged)?,
            kind: SaveKind::from_code(b[19]).ok_or(Error::HeaderDamaged)?,
            created: get64(b, 24),
            stream_id: arr(b, 32),
            system: crate::text(&b[48..56]),
            serial: crate::text(&b[56..66]),
            command: crate::text(&b[66..76]),
            user: crate::text(&b[76..86]),
        })
    }
}

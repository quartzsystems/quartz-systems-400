// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `FileAttr`: the attribute area of a `*FILE`. See `docs/DB_FORMAT.md` §2.

use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400DBF";
/// Version this code writes.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 128;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "FileAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("kind", 10, 1),
        field("maint", 11, 1),
        field("recover", 12, 1),
        field("share", 13, 1),
        field("flags", 14, 2),
        field("record_len", 16, 4),
        field("format_count", 20, 2),
        field("member_count", 22, 2),
        field("max_members", 24, 4),
        field("size_initial", 28, 4),
        field("size_increment", 32, 4),
        field("size_max_increments", 36, 4),
        field("frcratio", 40, 2),
        field("wait_file", 42, 2),
        field("wait_rcd", 44, 2),
        field("ccsid", 46, 2),
        field("expdate", 48, 8),
        field("def_len", 56, 8),
        field("formats_off", 64, 8),
        field("formats_len", 72, 8),
        field("members_off", 80, 8),
        field("member_cap", 88, 4),
        field("reserved", 92, 4),
        field("reserved2", 96, 32),
    ],
};

/// What kind of file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// `CRTPF`.
    Physical,
    /// `CRTLF`.
    Logical,
    /// `CRTSRCPF`.
    Source,
}

/// `MAINT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Maint {
    /// `*IMMED`.
    #[default]
    Immed,
    /// `*DLY`.
    Dly,
    /// `*REBLD`.
    Rebld,
}

/// `RECOVER`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Recover {
    /// `*NO`.
    #[default]
    No,
    /// `*AFTIPL`.
    AftIpl,
    /// `*IPL`.
    Ipl,
}

/// `WAITFILE` and `WAITRCD`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wait {
    /// `*IMMED`.
    #[default]
    Immed,
    /// `*CLS` for files, `*NOMAX` for records.
    NoMax,
    /// Seconds.
    Seconds(u16),
}

impl Wait {
    fn encode(self) -> u16 {
        match self {
            Self::Immed => 0,
            Self::NoMax => 0xFFFF,
            Self::Seconds(s) => s.min(0xFFFE),
        }
    }

    fn decode(v: u16) -> Self {
        match v {
            0 => Self::Immed,
            0xFFFF => Self::NoMax,
            s => Self::Seconds(s),
        }
    }
}

/// `SIZE(initial increment max-increments)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    /// Records before the first increment; 0 for `*NOMAX`.
    pub initial: u32,
    /// Records per increment.
    pub increment: u32,
    /// Increments allowed.
    pub max_increments: u32,
}

impl Default for Size {
    /// IBM's default: 10000 records, 1000 per increment, 3 increments.
    fn default() -> Self {
        Self {
            initial: 10_000,
            increment: 1_000,
            max_increments: 3,
        }
    }
}

impl Size {
    /// `*NOMAX`.
    pub const NOMAX: Self = Self {
        initial: 0,
        increment: 0,
        max_increments: 0,
    };

    /// Most records a member may hold, 0 for no limit.
    #[must_use]
    pub fn max_records(self) -> u64 {
        if self.initial == 0 {
            return 0;
        }
        u64::from(self.initial) + u64::from(self.increment) * u64::from(self.max_increments)
    }
}

/// The attribute area, decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct FileAttr {
    /// Kind.
    pub kind: FileKind,
    /// `MAINT`.
    pub maint: Maint,
    /// `RECOVER`.
    pub recover: Recover,
    /// `SHARE(*YES)`.
    pub share: bool,
    /// `REUSEDLT(*YES)`.
    pub reusedlt: bool,
    /// `FRCACCPTH(*YES)`.
    pub frcaccpth: bool,
    /// `LVLCHK(*YES)`.
    pub lvlchk: bool,
    /// `ALWUPD(*YES)`.
    pub alwupd: bool,
    /// `ALWDLT(*YES)`.
    pub alwdlt: bool,
    /// `ALLOCATE(*YES)`.
    pub allocate: bool,
    /// Bytes of the longest format.
    pub record_len: u32,
    /// Formats.
    pub format_count: u16,
    /// Members.
    pub member_count: u16,
    /// `MAXMBRS`, 0 for `*NOMAX`.
    pub max_members: u32,
    /// `SIZE`.
    pub size: Size,
    /// `FRCRATIO`, 0 for `*NONE`.
    pub frcratio: u16,
    /// `WAITFILE`.
    pub wait_file: Wait,
    /// `WAITRCD`.
    pub wait_rcd: Wait,
    /// CCSID.
    pub ccsid: u16,
    /// `EXPDATE`, Unix seconds, 0 for none.
    pub expdate: u64,
    /// Bytes of the byte space in use.
    pub def_len: u64,
    /// Offset of the format sections.
    pub formats_off: u64,
    /// Bytes of the format sections.
    pub formats_len: u64,
    /// Offset of the member table.
    pub members_off: u64,
    /// Capacity of the member table.
    pub member_cap: u32,
}

impl FileAttr {
    /// Encode.
    #[must_use]
    pub fn encode(&self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[..8].copy_from_slice(MAGIC);
        b[8..10].copy_from_slice(&VERSION.to_le_bytes());
        b[10] = match self.kind {
            FileKind::Physical => 0,
            FileKind::Logical => 1,
            FileKind::Source => 2,
        };
        b[11] = match self.maint {
            Maint::Immed => 0,
            Maint::Dly => 1,
            Maint::Rebld => 2,
        };
        b[12] = match self.recover {
            Recover::No => 0,
            Recover::AftIpl => 1,
            Recover::Ipl => 2,
        };
        b[13] = u8::from(self.share);
        let flags = u16::from(self.reusedlt)
            | u16::from(self.frcaccpth) << 1
            | u16::from(!self.lvlchk) << 2
            | u16::from(!self.alwupd) << 3
            | u16::from(!self.alwdlt) << 4
            | u16::from(self.allocate) << 5;
        b[14..16].copy_from_slice(&flags.to_le_bytes());
        b[16..20].copy_from_slice(&self.record_len.to_le_bytes());
        b[20..22].copy_from_slice(&self.format_count.to_le_bytes());
        b[22..24].copy_from_slice(&self.member_count.to_le_bytes());
        b[24..28].copy_from_slice(&self.max_members.to_le_bytes());
        b[28..32].copy_from_slice(&self.size.initial.to_le_bytes());
        b[32..36].copy_from_slice(&self.size.increment.to_le_bytes());
        b[36..40].copy_from_slice(&self.size.max_increments.to_le_bytes());
        b[40..42].copy_from_slice(&self.frcratio.to_le_bytes());
        b[42..44].copy_from_slice(&self.wait_file.encode().to_le_bytes());
        b[44..46].copy_from_slice(&self.wait_rcd.encode().to_le_bytes());
        b[46..48].copy_from_slice(&self.ccsid.to_le_bytes());
        b[48..56].copy_from_slice(&self.expdate.to_le_bytes());
        b[56..64].copy_from_slice(&self.def_len.to_le_bytes());
        b[64..72].copy_from_slice(&self.formats_off.to_le_bytes());
        b[72..80].copy_from_slice(&self.formats_len.to_le_bytes());
        b[80..88].copy_from_slice(&self.members_off.to_le_bytes());
        b[88..92].copy_from_slice(&self.member_cap.to_le_bytes());
        b
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| DbError::Corrupt {
            what: format!("file attributes: {what}"),
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
        let kind = match b[10] {
            0 => FileKind::Physical,
            1 => FileKind::Logical,
            2 => FileKind::Source,
            _ => return Err(bad("kind")),
        };
        let maint = match b[11] {
            0 => Maint::Immed,
            1 => Maint::Dly,
            2 => Maint::Rebld,
            _ => return Err(bad("maint")),
        };
        let recover = match b[12] {
            0 => Recover::No,
            1 => Recover::AftIpl,
            2 => Recover::Ipl,
            _ => return Err(bad("recover")),
        };
        let flags = u16_at(14);
        Ok(Self {
            kind,
            maint,
            recover,
            share: b[13] != 0,
            reusedlt: flags & 1 != 0,
            frcaccpth: flags & 2 != 0,
            lvlchk: flags & 4 == 0,
            alwupd: flags & 8 == 0,
            alwdlt: flags & 16 == 0,
            allocate: flags & 32 != 0,
            record_len: u32_at(16),
            format_count: u16_at(20),
            member_count: u16_at(22),
            max_members: u32_at(24),
            size: Size {
                initial: u32_at(28),
                increment: u32_at(32),
                max_increments: u32_at(36),
            },
            frcratio: u16_at(40),
            wait_file: Wait::decode(u16_at(42)),
            wait_rcd: Wait::decode(u16_at(44)),
            ccsid: u16_at(46),
            expdate: u64_at(48),
            def_len: u64_at(56),
            formats_off: u64_at(64),
            formats_len: u64_at(72),
            members_off: u64_at(80),
            member_cap: u32_at(88),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let a = FileAttr {
            kind: FileKind::Source,
            maint: Maint::Dly,
            recover: Recover::Ipl,
            share: true,
            reusedlt: true,
            frcaccpth: false,
            lvlchk: false,
            alwupd: true,
            alwdlt: false,
            allocate: true,
            record_len: 112,
            format_count: 1,
            member_count: 3,
            max_members: 0,
            size: Size::NOMAX,
            frcratio: 0,
            wait_file: Wait::NoMax,
            wait_rcd: Wait::Seconds(60),
            ccsid: 37,
            expdate: 0,
            def_len: 4000,
            formats_off: 0,
            formats_len: 1000,
            members_off: 1000,
            member_cap: 8,
        };
        assert_eq!(FileAttr::decode(&a.encode()).unwrap(), a);
        assert_eq!(Size::default().max_records(), 13_000);
        assert_eq!(Size::NOMAX.max_records(), 0);
        assert!(FileAttr::decode(&[0u8; ATTR_LEN]).is_err());
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The superblock and the unit table. See `docs/STORE_FORMAT.md` §5.

use release::Release;

use crate::asp::layout::MAX_UNITS;
use crate::asp::unit::UnitState;
use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::BODY_SIZE;
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400SUP";
/// Format version this code writes and reads.
pub const FORMAT_VERSION: u32 = 2;
/// Fixed part size; the unit table follows.
pub const HEADER_LEN: usize = 256;
/// Bytes per unit table entry.
pub const UNIT_ENTRY_LEN: usize = 80;
/// Bytes of a unit's serial number.
pub const SERIAL_LEN: usize = 32;
/// `mirror_partner` of an unprotected unit.
pub const NO_PARTNER: u16 = 0xFFFF;

/// `flags` bit: the WAL must be replayed.
pub const FLAG_NEEDS_RECOVERY: u32 = 1;
/// `flags` bit: RCLSTG should run.
pub const FLAG_RCLSTG_REQUIRED: u32 = 2;

/// `protection`: no unit is mirrored.
pub const PROTECTION_NONE: u8 = 0;
/// `protection`: at least one pair exists.
pub const PROTECTION_MIRRORED: u8 = 1;

const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 8;
const OFF_FLAGS: usize = 12;
const OFF_GENERATION: usize = 16;
const OFF_ASP_UUID: usize = 24;
const OFF_ASP_NUMBER: usize = 40;
const OFF_UNIT_COUNT: usize = 42;
const OFF_RELEASE_V: usize = 44;
const OFF_RELEASE_R: usize = 45;
const OFF_RELEASE_M: usize = 46;
const OFF_PROTECTION: usize = 47;
const OFF_ALLOC_ROOT: usize = 48;
const OFF_FREE_LOG_HEAD: usize = 56;
const OFF_OBJDIR_ROOT: usize = 64;
const OFF_NEXT_SERIAL: usize = 72;
const OFF_WAL_START: usize = 80;
const OFF_WAL_PAGES: usize = 88;
const OFF_WAL_HEAD: usize = 96;
const OFF_WAL_TAIL: usize = 104;
const OFF_LAST_TXN: usize = 112;
const OFF_INSTALL_TIME: usize = 120;
const OFF_LAST_RCLSTG: usize = 128;
const OFF_QSYS: usize = 136;
const OFF_SYSVAL: usize = 152;

const ENTRY_UUID: usize = 0;
const ENTRY_PAGES: usize = 16;
const ENTRY_START: usize = 24;
const ENTRY_USABLE: usize = 32;
const ENTRY_PARTNER: usize = 40;
const ENTRY_STATE: usize = 42;
const ENTRY_SERIAL: usize = 48;

/// The spec layout of the fixed part.
pub static LAYOUT: Layout = Layout {
    name: "Superblock",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", OFF_MAGIC, 8),
        field("format_version", OFF_VERSION, 4),
        field("flags", OFF_FLAGS, 4),
        field("generation", OFF_GENERATION, 8),
        field("asp_uuid", OFF_ASP_UUID, 16),
        field("asp_number", OFF_ASP_NUMBER, 2),
        field("unit_count", OFF_UNIT_COUNT, 2),
        field("release_v", OFF_RELEASE_V, 1),
        field("release_r", OFF_RELEASE_R, 1),
        field("release_m", OFF_RELEASE_M, 1),
        field("protection", OFF_PROTECTION, 1),
        field("alloc_root", OFF_ALLOC_ROOT, 8),
        field("free_log_head", OFF_FREE_LOG_HEAD, 8),
        field("objdir_root", OFF_OBJDIR_ROOT, 8),
        field("next_serial", OFF_NEXT_SERIAL, 8),
        field("wal_start", OFF_WAL_START, 8),
        field("wal_pages", OFF_WAL_PAGES, 8),
        field("wal_head", OFF_WAL_HEAD, 8),
        field("wal_tail", OFF_WAL_TAIL, 8),
        field("last_txn", OFF_LAST_TXN, 8),
        field("install_time", OFF_INSTALL_TIME, 8),
        field("last_rclstg_time", OFF_LAST_RCLSTG, 8),
        field("qsys", OFF_QSYS, 16),
        field("sysval", OFF_SYSVAL, 16),
        field("reserved2", 168, 88),
    ],
};

/// The spec layout of a unit table entry.
pub static UNIT_ENTRY_LAYOUT: Layout = Layout {
    name: "UnitEntry",
    size: Some(UNIT_ENTRY_LEN),
    fields: &[
        field("unit_uuid", ENTRY_UUID, 16),
        field("pages", ENTRY_PAGES, 8),
        field("start", ENTRY_START, 8),
        field("usable_pages", ENTRY_USABLE, 8),
        field("mirror_partner", ENTRY_PARTNER, 2),
        field("state", ENTRY_STATE, 1),
        field("reserved", 43, 5),
        field("serial", ENTRY_SERIAL, SERIAL_LEN),
    ],
};

/// One unit in the unit table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitEntry {
    /// Must match the unit header.
    pub unit_uuid: [u8; 16],
    /// Unit size in pages.
    pub pages: u64,
    /// Logical page of the unit's page 0; both halves of a pair share it.
    pub start: PageNo,
    /// Pages of the span the unit backs: `pages`, or the smaller of a pair.
    pub usable_pages: u64,
    /// The other half of the pair, or [`NO_PARTNER`].
    pub mirror_partner: u16,
    /// Protection state.
    pub state: UnitState,
    /// The block device's serial number, ASCII, zero padded.
    pub serial: [u8; SERIAL_LEN],
}

impl UnitEntry {
    /// An unprotected unit.
    #[must_use]
    pub fn unprotected(unit_uuid: [u8; 16], pages: u64, start: PageNo, serial: &str) -> Self {
        Self {
            unit_uuid,
            pages,
            start,
            usable_pages: pages,
            mirror_partner: NO_PARTNER,
            state: UnitState::Unprotected,
            serial: serial_bytes(serial),
        }
    }

    /// The serial as text; the unit UUID in hex when none was recorded.
    #[must_use]
    pub fn serial_text(&self) -> String {
        let end = self
            .serial
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(SERIAL_LEN);
        let s = String::from_utf8_lossy(&self.serial[..end])
            .trim()
            .to_string();
        if s.is_empty() {
            use std::fmt::Write as _;
            self.unit_uuid.iter().fold(String::new(), |mut out, b| {
                let _ = write!(out, "{b:02X}");
                out
            })
        } else {
            s
        }
    }

    /// Whether the unit is half of a pair.
    #[must_use]
    pub fn mirrored(&self) -> bool {
        self.mirror_partner != NO_PARTNER
    }
}

/// A serial as the table stores it: ASCII, truncated, zero padded.
#[must_use]
pub fn serial_bytes(serial: &str) -> [u8; SERIAL_LEN] {
    let mut out = [0u8; SERIAL_LEN];
    for (o, c) in out
        .iter_mut()
        .zip(serial.bytes().filter(u8::is_ascii_graphic))
    {
        *o = c;
    }
    out
}

/// The superblock contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superblock {
    /// `FLAG_*` bits.
    pub flags: u32,
    /// Checkpoint number.
    pub generation: u64,
    /// ASP identity.
    pub asp_uuid: [u8; 16],
    /// ASP number.
    pub asp_number: u16,
    /// Creating release.
    pub release: Release,
    /// `PROTECTION_*`: whether any unit is mirrored.
    pub protection: u8,
    /// Free-space tree root.
    pub alloc_root: Option<PageNo>,
    /// First free-log page.
    pub free_log_head: Option<PageNo>,
    /// Object directory root.
    pub objdir_root: Option<PageNo>,
    /// Next object serial.
    pub next_serial: u64,
    /// First WAL page.
    pub wal_start: PageNo,
    /// WAL ring length.
    pub wal_pages: u64,
    /// WAL head.
    pub wal_head: u64,
    /// WAL tail.
    pub wal_tail: u64,
    /// Newest committed transaction at this checkpoint.
    pub last_txn: u64,
    /// Unix seconds.
    pub install_time: u64,
    /// Unix seconds, 0 for never.
    pub last_rclstg_time: u64,
    /// QSYS, or `NONE` before the object layer initializes.
    pub qsys: ObjectId,
    /// The `*SYSVAL` object, or `NONE`.
    pub sysval: ObjectId,
    /// The unit table.
    pub units: Vec<UnitEntry>,
}

fn opt(v: u64) -> Option<PageNo> {
    (v != 0).then_some(v)
}

impl Superblock {
    /// Whether `flags` has `bit`.
    #[must_use]
    pub fn flag(&self, bit: u32) -> bool {
        self.flags & bit != 0
    }

    /// Recompute `protection` from the unit table.
    pub fn update_protection(&mut self) {
        self.protection = if self.units.iter().any(UnitEntry::mirrored) {
            PROTECTION_MIRRORED
        } else {
            PROTECTION_NONE
        };
    }

    /// Encode into a page body.
    pub fn encode(&self, body: &mut [u8]) {
        assert!(self.units.len() <= MAX_UNITS, "too many units");
        body[..BODY_SIZE].fill(0);
        body[OFF_MAGIC..OFF_MAGIC + 8].copy_from_slice(MAGIC);
        put_u32(body, OFF_VERSION, FORMAT_VERSION);
        put_u32(body, OFF_FLAGS, self.flags);
        put_u64(body, OFF_GENERATION, self.generation);
        body[OFF_ASP_UUID..OFF_ASP_UUID + 16].copy_from_slice(&self.asp_uuid);
        put_u16(body, OFF_ASP_NUMBER, self.asp_number);
        put_u16(body, OFF_UNIT_COUNT, u16_of(self.units.len()));
        body[OFF_RELEASE_V] = self.release.v;
        body[OFF_RELEASE_R] = self.release.r;
        body[OFF_RELEASE_M] = self.release.m;
        body[OFF_PROTECTION] = self.protection;
        put_u64(body, OFF_ALLOC_ROOT, self.alloc_root.unwrap_or(0));
        put_u64(body, OFF_FREE_LOG_HEAD, self.free_log_head.unwrap_or(0));
        put_u64(body, OFF_OBJDIR_ROOT, self.objdir_root.unwrap_or(0));
        put_u64(body, OFF_NEXT_SERIAL, self.next_serial);
        put_u64(body, OFF_WAL_START, self.wal_start);
        put_u64(body, OFF_WAL_PAGES, self.wal_pages);
        put_u64(body, OFF_WAL_HEAD, self.wal_head);
        put_u64(body, OFF_WAL_TAIL, self.wal_tail);
        put_u64(body, OFF_LAST_TXN, self.last_txn);
        put_u64(body, OFF_INSTALL_TIME, self.install_time);
        put_u64(body, OFF_LAST_RCLSTG, self.last_rclstg_time);
        body[OFF_QSYS..OFF_QSYS + 16].copy_from_slice(&self.qsys.to_bytes());
        body[OFF_SYSVAL..OFF_SYSVAL + 16].copy_from_slice(&self.sysval.to_bytes());
        for (i, u) in self.units.iter().enumerate() {
            let at = HEADER_LEN + i * UNIT_ENTRY_LEN;
            body[at + ENTRY_UUID..at + ENTRY_UUID + 16].copy_from_slice(&u.unit_uuid);
            put_u64(body, at + ENTRY_PAGES, u.pages);
            put_u64(body, at + ENTRY_START, u.start);
            put_u64(body, at + ENTRY_USABLE, u.usable_pages);
            put_u16(body, at + ENTRY_PARTNER, u.mirror_partner);
            body[at + ENTRY_STATE] = u.state.as_u8();
            body[at + ENTRY_SERIAL..at + ENTRY_SERIAL + SERIAL_LEN].copy_from_slice(&u.serial);
        }
    }

    /// Decode a page body. `page` is for errors.
    pub fn decode(body: &[u8], page: PageNo) -> Result<Self> {
        let corrupt = |what: String| StoreError::Corrupt { page, what };
        if &body[OFF_MAGIC..OFF_MAGIC + 8] != MAGIC {
            return Err(corrupt("not a superblock".into()));
        }
        let version = get_u32(body, OFF_VERSION);
        if version != FORMAT_VERSION {
            return Err(corrupt(format!("superblock format version {version}")));
        }
        let unit_count = usize::from(get_u16(body, OFF_UNIT_COUNT));
        if unit_count == 0 || unit_count > MAX_UNITS {
            return Err(corrupt(format!("{unit_count} units")));
        }
        let mut units = Vec::with_capacity(unit_count);
        for i in 0..unit_count {
            let at = HEADER_LEN + i * UNIT_ENTRY_LEN;
            let state = UnitState::from_u8(body[at + ENTRY_STATE])
                .ok_or_else(|| corrupt(format!("unit {i} state {}", body[at + ENTRY_STATE])))?;
            units.push(UnitEntry {
                unit_uuid: body[at + ENTRY_UUID..at + ENTRY_UUID + 16]
                    .try_into()
                    .expect("16"),
                pages: get_u64(body, at + ENTRY_PAGES),
                start: get_u64(body, at + ENTRY_START),
                usable_pages: get_u64(body, at + ENTRY_USABLE),
                mirror_partner: get_u16(body, at + ENTRY_PARTNER),
                state,
                serial: body[at + ENTRY_SERIAL..at + ENTRY_SERIAL + SERIAL_LEN]
                    .try_into()
                    .expect("32"),
            });
        }
        Ok(Self {
            flags: get_u32(body, OFF_FLAGS),
            generation: get_u64(body, OFF_GENERATION),
            asp_uuid: body[OFF_ASP_UUID..OFF_ASP_UUID + 16]
                .try_into()
                .expect("16"),
            asp_number: get_u16(body, OFF_ASP_NUMBER),
            release: Release {
                v: body[OFF_RELEASE_V],
                r: body[OFF_RELEASE_R],
                m: body[OFF_RELEASE_M],
            },
            protection: body[OFF_PROTECTION],
            alloc_root: opt(get_u64(body, OFF_ALLOC_ROOT)),
            free_log_head: opt(get_u64(body, OFF_FREE_LOG_HEAD)),
            objdir_root: opt(get_u64(body, OFF_OBJDIR_ROOT)),
            next_serial: get_u64(body, OFF_NEXT_SERIAL),
            wal_start: get_u64(body, OFF_WAL_START),
            wal_pages: get_u64(body, OFF_WAL_PAGES),
            wal_head: get_u64(body, OFF_WAL_HEAD),
            wal_tail: get_u64(body, OFF_WAL_TAIL),
            last_txn: get_u64(body, OFF_LAST_TXN),
            install_time: get_u64(body, OFF_INSTALL_TIME),
            last_rclstg_time: get_u64(body, OFF_LAST_RCLSTG),
            qsys: ObjectId::from_bytes(&body[OFF_QSYS..OFF_QSYS + 16]),
            sysval: ObjectId::from_bytes(&body[OFF_SYSVAL..OFF_SYSVAL + 16]),
            units,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut sb = Superblock {
            flags: FLAG_RCLSTG_REQUIRED,
            generation: 9,
            asp_uuid: [7; 16],
            asp_number: 1,
            release: release::RELEASE,
            protection: 0,
            alloc_root: Some(44),
            free_log_head: None,
            objdir_root: Some(45),
            next_serial: 100,
            wal_start: 3,
            wal_pages: 16384,
            wal_head: 5,
            wal_tail: 2,
            last_txn: 77,
            install_time: 1_767_225_600,
            last_rclstg_time: 0,
            qsys: ObjectId::NONE,
            sysval: ObjectId {
                asp: 1,
                type_code: 0x00FF,
                serial: 2,
                generation: 0,
            },
            units: vec![
                UnitEntry::unprotected([1; 16], 1000, 0, "QM00001"),
                UnitEntry {
                    unit_uuid: [2; 16],
                    pages: 600,
                    start: 1000,
                    usable_pages: 500,
                    mirror_partner: 2,
                    state: UnitState::Active,
                    serial: serial_bytes("S2"),
                },
                UnitEntry {
                    unit_uuid: [3; 16],
                    pages: 500,
                    start: 1000,
                    usable_pages: 500,
                    mirror_partner: 1,
                    state: UnitState::Resuming,
                    serial: serial_bytes(""),
                },
            ],
        };
        sb.update_protection();
        assert_eq!(sb.protection, PROTECTION_MIRRORED);
        let mut body = vec![0u8; BODY_SIZE];
        sb.encode(&mut body);
        let back = Superblock::decode(&body, 1).unwrap();
        assert_eq!(back, sb);
        assert!(back.flag(FLAG_RCLSTG_REQUIRED));
        assert!(!back.flag(FLAG_NEEDS_RECOVERY));
        assert_eq!(back.units[0].serial_text(), "QM00001");
        assert_eq!(back.units[2].serial_text(), "03".repeat(16));
        put_u16(&mut body, OFF_UNIT_COUNT, 0);
        assert!(matches!(
            Superblock::decode(&body, 1),
            Err(StoreError::Corrupt { .. })
        ));
    }
}

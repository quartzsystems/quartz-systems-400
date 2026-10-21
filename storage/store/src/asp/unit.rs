// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The unit header and the unit states. See `docs/STORE_FORMAT.md` §4.

use std::fmt;

use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64};
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400ASP";
/// Format version this code writes and reads.
pub const FORMAT_VERSION: u32 = 2;

const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 8;
const OFF_FLAGS: usize = 12;
const OFF_UNIT_UUID: usize = 16;
const OFF_ASP_UUID: usize = 32;
const OFF_ASP_NUMBER: usize = 48;
const OFF_UNIT_NUMBER: usize = 50;
const OFF_PAGES: usize = 56;
const OFF_MIRROR_OF: usize = 64;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "UnitHeader",
    size: Some(80),
    fields: &[
        field("magic", OFF_MAGIC, 8),
        field("format_version", OFF_VERSION, 4),
        field("flags", OFF_FLAGS, 4),
        field("unit_uuid", OFF_UNIT_UUID, 16),
        field("asp_uuid", OFF_ASP_UUID, 16),
        field("asp_number", OFF_ASP_NUMBER, 2),
        field("unit_number", OFF_UNIT_NUMBER, 2),
        field("reserved", 52, 4),
        field("pages", OFF_PAGES, 8),
        field("mirror_of", OFF_MIRROR_OF, 16),
    ],
};

/// What a unit is doing, named as the disk configuration displays
/// name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitState {
    /// Not mirrored.
    #[default]
    Unprotected,
    /// Half of a pair, in step with its partner.
    Active,
    /// Half of a pair, not written to; its data is stale.
    Suspended,
    /// Half of a pair being brought back in step.
    Resuming,
    /// Present but failing I/O.
    NotOperational,
    /// Not found at IPL.
    Missing,
}

impl UnitState {
    /// The byte in the unit table.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            UnitState::Unprotected => 0,
            UnitState::Active => 1,
            UnitState::Suspended => 2,
            UnitState::Resuming => 3,
            UnitState::NotOperational => 4,
            UnitState::Missing => 5,
        }
    }

    /// From the unit table byte.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => UnitState::Unprotected,
            1 => UnitState::Active,
            2 => UnitState::Suspended,
            3 => UnitState::Resuming,
            4 => UnitState::NotOperational,
            5 => UnitState::Missing,
            _ => return None,
        })
    }

    /// Whether the unit is half of a pair.
    #[must_use]
    pub fn mirrored(self) -> bool {
        self != UnitState::Unprotected
    }

    /// Whether reads may be served from the unit.
    #[must_use]
    pub fn readable(self) -> bool {
        matches!(self, UnitState::Unprotected | UnitState::Active)
    }

    /// Whether writes must go to the unit.
    #[must_use]
    pub fn writable(self) -> bool {
        matches!(
            self,
            UnitState::Unprotected | UnitState::Active | UnitState::Resuming
        )
    }

    /// Whether the unit is attached and answering.
    #[must_use]
    pub fn present(self) -> bool {
        !matches!(self, UnitState::Missing | UnitState::NotOperational)
    }
}

impl fmt::Display for UnitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            UnitState::Unprotected => "Unprotected",
            UnitState::Active => "Active",
            UnitState::Suspended => "Suspended",
            UnitState::Resuming => "Resuming",
            UnitState::NotOperational => "Not operational",
            UnitState::Missing => "Missing",
        })
    }
}

/// A unit's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitHeader {
    /// This unit.
    pub unit_uuid: [u8; 16],
    /// The ASP it belongs to.
    pub asp_uuid: [u8; 16],
    /// ASP number.
    pub asp_number: u16,
    /// Position in the ASP.
    pub unit_number: u16,
    /// Size in pages.
    pub pages: u64,
    /// The other half of the pair, `None` when unprotected.
    pub mirror_of: Option<[u8; 16]>,
}

impl UnitHeader {
    /// Encode into a page body.
    pub fn encode(&self, body: &mut [u8]) {
        body[..80].fill(0);
        body[OFF_MAGIC..OFF_MAGIC + 8].copy_from_slice(MAGIC);
        put_u32(body, OFF_VERSION, FORMAT_VERSION);
        put_u32(body, OFF_FLAGS, 0);
        body[OFF_UNIT_UUID..OFF_UNIT_UUID + 16].copy_from_slice(&self.unit_uuid);
        body[OFF_ASP_UUID..OFF_ASP_UUID + 16].copy_from_slice(&self.asp_uuid);
        put_u16(body, OFF_ASP_NUMBER, self.asp_number);
        put_u16(body, OFF_UNIT_NUMBER, self.unit_number);
        put_u64(body, OFF_PAGES, self.pages);
        if let Some(m) = self.mirror_of {
            body[OFF_MIRROR_OF..OFF_MIRROR_OF + 16].copy_from_slice(&m);
        }
    }

    /// Decode a page body. `page` is for errors.
    pub fn decode(body: &[u8], page: PageNo) -> Result<Self> {
        if &body[OFF_MAGIC..OFF_MAGIC + 8] != MAGIC {
            return Err(StoreError::Corrupt {
                page,
                what: "not a unit header".into(),
            });
        }
        let version = get_u32(body, OFF_VERSION);
        if version != FORMAT_VERSION {
            return Err(StoreError::Corrupt {
                page,
                what: format!("unit header format version {version}"),
            });
        }
        let mirror: [u8; 16] = body[OFF_MIRROR_OF..OFF_MIRROR_OF + 16]
            .try_into()
            .expect("16");
        Ok(Self {
            unit_uuid: body[OFF_UNIT_UUID..OFF_UNIT_UUID + 16]
                .try_into()
                .expect("16"),
            asp_uuid: body[OFF_ASP_UUID..OFF_ASP_UUID + 16]
                .try_into()
                .expect("16"),
            asp_number: get_u16(body, OFF_ASP_NUMBER),
            unit_number: get_u16(body, OFF_UNIT_NUMBER),
            pages: get_u64(body, OFF_PAGES),
            mirror_of: (mirror != [0; 16]).then_some(mirror),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::BODY_SIZE;

    #[test]
    fn round_trip() {
        let h = UnitHeader {
            unit_uuid: [1; 16],
            asp_uuid: [2; 16],
            asp_number: 1,
            unit_number: 3,
            pages: 1 << 20,
            mirror_of: Some([5; 16]),
        };
        let mut body = vec![0u8; BODY_SIZE];
        h.encode(&mut body);
        assert_eq!(UnitHeader::decode(&body, 0).unwrap(), h);
        body[0] = b'X';
        assert!(matches!(
            UnitHeader::decode(&body, 0),
            Err(StoreError::Corrupt { .. })
        ));
    }

    #[test]
    fn states_round_trip_and_name_themselves() {
        for v in 0..6 {
            let s = UnitState::from_u8(v).unwrap();
            assert_eq!(s.as_u8(), v);
        }
        assert!(UnitState::from_u8(6).is_none());
        assert_eq!(UnitState::NotOperational.to_string(), "Not operational");
        assert!(UnitState::Resuming.writable() && !UnitState::Resuming.readable());
        assert!(!UnitState::Suspended.writable());
        assert!(!UnitState::Missing.present());
    }
}

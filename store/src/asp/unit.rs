// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The unit header. See `docs/STORE_FORMAT.md` §4.

use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64};
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

/// Magic at offset 0.
pub const MAGIC: &[u8; 8] = b"QS400ASP";
/// Format version this code writes and reads.
pub const FORMAT_VERSION: u32 = 1;

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
    /// Mirror source; reserved, always `None` in Phase 2.
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
            mirror_of: None,
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
}

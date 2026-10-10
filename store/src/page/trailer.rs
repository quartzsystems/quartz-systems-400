// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 32-byte page trailer. See `docs/STORE_FORMAT.md` §1.

use crc::{CRC_32_ISCSI, Crc};

use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64};
use crate::obj::ObjectId;
use crate::page::{BODY_SIZE, PAGE_SIZE};
use crate::spec::{Layout, field};

const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// Trailer field offsets, relative to the trailer.
pub const OFF_CRC: usize = 0;
/// Page type.
pub const OFF_TYPE: usize = 4;
/// Flags.
pub const OFF_FLAGS: usize = 6;
/// Owner ID.
pub const OFF_OWNER: usize = 8;
/// Generation.
pub const OFF_GENERATION: usize = 24;

/// The trailer as the spec describes it.
pub static LAYOUT: Layout = Layout {
    name: "PageTrailer",
    size: Some(32),
    fields: &[
        field("crc", OFF_CRC, 4),
        field("page_type", OFF_TYPE, 2),
        field("flags", OFF_FLAGS, 2),
        field("owner", OFF_OWNER, 16),
        field("generation", OFF_GENERATION, 8),
    ],
};

/// Decoded trailer fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trailer {
    /// Page type as stored.
    pub page_type: u16,
    /// Reserved.
    pub flags: u16,
    /// Owning object, or `ObjectId::NONE` for metadata.
    pub owner: ObjectId,
    /// Transaction that wrote the page.
    pub generation: u64,
}

impl Trailer {
    /// Decode from a whole page.
    #[must_use]
    pub fn read(page: &[u8]) -> Self {
        let t = &page[BODY_SIZE..PAGE_SIZE];
        Self {
            page_type: get_u16(t, OFF_TYPE),
            flags: get_u16(t, OFF_FLAGS),
            owner: ObjectId::from_bytes(&t[OFF_OWNER..OFF_OWNER + 16]),
            generation: get_u64(t, OFF_GENERATION),
        }
    }

    /// Encode into a whole page and set its checksum.
    pub fn write(&self, page: &mut [u8]) {
        {
            let t = &mut page[BODY_SIZE..PAGE_SIZE];
            put_u16(t, OFF_TYPE, self.page_type);
            put_u16(t, OFF_FLAGS, self.flags);
            t[OFF_OWNER..OFF_OWNER + 16].copy_from_slice(&self.owner.to_bytes());
            put_u64(t, OFF_GENERATION, self.generation);
        }
        let crc = checksum(page);
        put_u32(&mut page[BODY_SIZE..], OFF_CRC, crc);
    }
}

/// CRC32C of everything but the CRC field itself.
#[must_use]
pub fn checksum(page: &[u8]) -> u32 {
    let mut d = CRC32C.digest();
    d.update(&page[..BODY_SIZE]);
    d.update(&page[BODY_SIZE + OFF_CRC + 4..PAGE_SIZE]);
    d.finalize()
}

/// The CRC stored in the trailer.
#[must_use]
pub fn stored_crc(page: &[u8]) -> u32 {
    get_u32(&page[BODY_SIZE..], OFF_CRC)
}

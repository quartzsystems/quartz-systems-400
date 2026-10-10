// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Checksummed 4 KiB pages: a body and a trailer naming the page type,
//! the owning object and the writing transaction.

pub mod pager;
pub mod trailer;

use blockdev::AlignedBuf;

use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::{Result, StoreError};
use trailer::Trailer;

/// Bytes per page.
pub const PAGE_SIZE: usize = 4096;
/// Bytes of trailer at the end of every page.
pub const TRAILER_SIZE: usize = 32;
/// Bytes of body before the trailer.
pub const BODY_SIZE: usize = PAGE_SIZE - TRAILER_SIZE;

/// What a page holds. The value is the `page_type` trailer field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum PageType {
    /// Never valid on read.
    Free = 0x0000,
    /// Unit header.
    UnitHeader = 0x0001,
    /// Superblock.
    Superblock = 0x0002,
    /// Write-ahead log page.
    Wal = 0x0003,
    /// B+tree node, leaf or internal.
    BTreeNode = 0x0010,
    /// Object header.
    ObjectHeader = 0x0020,
    /// Object data.
    ObjectData = 0x0021,
    /// Extent list overflow.
    ExtentOverflow = 0x0022,
}

impl PageType {
    /// From the trailer field.
    #[must_use]
    pub fn from_u16(v: u16) -> Option<Self> {
        Some(match v {
            0x0000 => Self::Free,
            0x0001 => Self::UnitHeader,
            0x0002 => Self::Superblock,
            0x0003 => Self::Wal,
            0x0010 => Self::BTreeNode,
            0x0020 => Self::ObjectHeader,
            0x0021 => Self::ObjectData,
            0x0022 => Self::ExtentOverflow,
            _ => return None,
        })
    }

    /// To the trailer field.
    #[must_use]
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

/// One page in memory, block aligned so it can go straight to an
/// `O_DIRECT` device.
#[derive(Clone)]
pub struct Page {
    buf: AlignedBuf,
}

impl Page {
    /// An all-zero page.
    #[must_use]
    pub fn zeroed() -> Self {
        Self {
            buf: AlignedBuf::new(1),
        }
    }

    /// A page from its raw bytes (exactly one page).
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), PAGE_SIZE, "a page is {PAGE_SIZE} bytes");
        Self {
            buf: AlignedBuf::from_bytes(bytes),
        }
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.buf[..BODY_SIZE]
    }

    /// The body, writable.
    pub fn body_mut(&mut self) -> &mut [u8] {
        &mut self.buf[..BODY_SIZE]
    }

    /// All bytes, trailer included.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    /// All bytes, writable. Used by readers filling the page from a
    /// device; `seal` must follow any change before a write.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Write the trailer and the checksum.
    pub fn seal(&mut self, page_type: PageType, owner: ObjectId, generation: u64) {
        Trailer {
            page_type: page_type.as_u16(),
            flags: 0,
            owner,
            generation,
        }
        .write(&mut self.buf);
    }

    /// The trailer as stored, without verifying anything.
    #[must_use]
    pub fn trailer(&self) -> Trailer {
        Trailer::read(&self.buf)
    }

    /// Verify the checksum. `page` is only for the error.
    pub fn verify(&self, page: PageNo) -> Result<Trailer> {
        let t = self.trailer();
        let stored = trailer::stored_crc(&self.buf);
        let computed = trailer::checksum(&self.buf);
        if stored != computed {
            return Err(StoreError::Checksum {
                page,
                owner: t.owner,
                stored,
                computed,
            });
        }
        Ok(t)
    }

    /// Verify the checksum and the page type.
    pub fn verify_type(&self, page: PageNo, expected: PageType) -> Result<Trailer> {
        let t = self.verify(page)?;
        if t.page_type != expected.as_u16() {
            return Err(StoreError::PageType {
                page,
                expected,
                found: t.page_type,
            });
        }
        Ok(t)
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Page({:?})", self.trailer())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_and_verify() {
        let owner = ObjectId {
            asp: 1,
            type_code: 2,
            serial: 3,
            generation: 4,
        };
        let mut page = Page::zeroed();
        page.body_mut()[10] = 0xAA;
        page.seal(PageType::ObjectData, owner, 77);
        let trailer = page.verify_type(5, PageType::ObjectData).unwrap();
        assert_eq!(trailer.owner, owner);
        assert_eq!(trailer.generation, 77);
        assert!(matches!(
            page.verify_type(5, PageType::Superblock),
            Err(StoreError::PageType {
                page: 5,
                expected: PageType::Superblock,
                found: 0x21
            })
        ));
        // Any flipped bit, body or trailer, fails and names page and owner.
        let mut body_hit = page.clone();
        body_hit.body_mut()[0] ^= 1;
        let e = body_hit.verify(9).unwrap_err();
        assert!(matches!(e, StoreError::Checksum { page: 9, owner: o, .. } if o == owner));
        let mut trailer_hit = page.clone();
        trailer_hit.bytes_mut()[BODY_SIZE + 24] ^= 1;
        assert!(matches!(
            trailer_hit.verify(9),
            Err(StoreError::Checksum { .. })
        ));
        assert!(matches!(
            Page::zeroed().verify(0),
            Err(StoreError::Checksum { .. })
        ));
    }

    #[test]
    fn page_types_round_trip() {
        for v in [0, 1, 2, 3, 0x10, 0x20, 0x21, 0x22] {
            assert_eq!(PageType::from_u16(v).unwrap().as_u16(), v);
        }
        assert!(PageType::from_u16(0x11).is_none());
    }
}

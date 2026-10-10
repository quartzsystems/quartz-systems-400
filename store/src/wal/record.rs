// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One transaction's record. See `docs/STORE_FORMAT.md` §6.

use crate::alloc::Extent;
use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::PAGE_SIZE;
use crate::page::pager::PageNo;
use crate::spec::{Field, Layout, field};
use crate::{Result, StoreError};

/// Record header bytes.
pub const HEADER_LEN: usize = 32;
/// Root entry bytes.
pub const ROOT_LEN: usize = 24;
/// Extent entry bytes (the `FreeLogEntry` layout).
pub const EXTENT_LEN: usize = 24;
/// Image entry bytes.
pub const IMAGE_LEN: usize = 8 + PAGE_SIZE;
/// Patch entry bytes before the data.
pub const PATCH_HEAD: usize = 12;

/// The spec layout of the record header.
pub static RECORD_LAYOUT: Layout = Layout {
    name: "WalRecord",
    size: Some(HEADER_LEN),
    fields: &[
        field("txn", 0, 8),
        field("next_serial", 8, 8),
        field("root_count", 16, 2),
        field("alloc_count", 18, 2),
        field("free_count", 20, 2),
        field("image_count", 22, 2),
        field("patch_count", 24, 4),
        field("reserved", 28, 4),
    ],
};

/// The spec layout of a root entry.
pub static ROOT_LAYOUT: Layout = Layout {
    name: "RootEntry",
    size: Some(ROOT_LEN),
    fields: &[field("owner", 0, 16), field("root", 16, 8)],
};

/// The spec layout of an image entry.
pub static IMAGE_LAYOUT: Layout = Layout {
    name: "ImageEntry",
    size: Some(IMAGE_LEN),
    fields: &[field("page", 0, 8), field("image", 8, PAGE_SIZE)],
};

/// The spec layout of a patch entry.
pub static PATCH_LAYOUT: Layout = Layout {
    name: "PatchEntry",
    size: None,
    fields: &[
        field("page", 0, 8),
        field("offset", 8, 2),
        field("len", 10, 2),
        Field {
            name: "bytes",
            offset: Some(12),
            size: None,
        },
    ],
};

/// An in-place change to part of a page body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The page.
    pub page: PageNo,
    /// Body offset.
    pub offset: u16,
    /// New bytes.
    pub bytes: Vec<u8>,
}

/// What a transaction did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalRecord {
    /// Transaction ID.
    pub txn: u64,
    /// Object serial counter after the transaction.
    pub next_serial: u64,
    /// Published tree roots.
    pub roots: Vec<(ObjectId, Option<PageNo>)>,
    /// Pages allocated.
    pub allocs: Vec<Extent>,
    /// Pages freed.
    pub frees: Vec<Extent>,
    /// Whole pages written in place (first change since a checkpoint).
    pub images: Vec<(PageNo, Vec<u8>)>,
    /// Partial in-place changes.
    pub patches: Vec<Patch>,
}

fn put_extent(out: &mut Vec<u8>, e: &Extent) {
    let mut b = [0u8; EXTENT_LEN];
    put_u16(&mut b, 0, e.unit);
    put_u64(&mut b, 8, e.lba);
    put_u64(&mut b, 16, e.len);
    out.extend_from_slice(&b);
}

fn get_extent(b: &[u8]) -> Extent {
    Extent {
        unit: get_u16(b, 0),
        lba: get_u64(b, 8),
        len: get_u64(b, 16),
    }
}

fn bad(what: &str) -> StoreError {
    StoreError::Corrupt {
        page: 0,
        what: format!("WAL record: {what}"),
    }
}

impl WalRecord {
    /// Encoded size.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        HEADER_LEN
            + self.roots.len() * ROOT_LEN
            + (self.allocs.len() + self.frees.len()) * EXTENT_LEN
            + self.images.len() * IMAGE_LEN
            + self
                .patches
                .iter()
                .map(|p| PATCH_HEAD + p.bytes.len())
                .sum::<usize>()
    }

    /// Encode. Fails when a count exceeds its field.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let count16 = |n: usize| {
            u16::try_from(n).map_err(|_| StoreError::TxnTooLarge {
                bytes: self.encoded_len(),
                ring_bytes: 0,
            })
        };
        let mut out = Vec::with_capacity(self.encoded_len());
        let mut h = [0u8; HEADER_LEN];
        put_u64(&mut h, 0, self.txn);
        put_u64(&mut h, 8, self.next_serial);
        put_u16(&mut h, 16, count16(self.roots.len())?);
        put_u16(&mut h, 18, count16(self.allocs.len())?);
        put_u16(&mut h, 20, count16(self.frees.len())?);
        put_u16(&mut h, 22, count16(self.images.len())?);
        put_u32(
            &mut h,
            24,
            u32::try_from(self.patches.len()).expect("patch count"),
        );
        out.extend_from_slice(&h);
        for (owner, root) in &self.roots {
            out.extend_from_slice(&owner.to_bytes());
            out.extend_from_slice(&root.unwrap_or(0).to_le_bytes());
        }
        for e in &self.allocs {
            put_extent(&mut out, e);
        }
        for e in &self.frees {
            put_extent(&mut out, e);
        }
        for (page, image) in &self.images {
            assert_eq!(image.len(), PAGE_SIZE, "image is one page");
            out.extend_from_slice(&page.to_le_bytes());
            out.extend_from_slice(image);
        }
        for p in &self.patches {
            out.extend_from_slice(&p.page.to_le_bytes());
            let mut b = [0u8; 4];
            put_u16(&mut b, 0, p.offset);
            put_u16(&mut b, 2, u16_of(p.bytes.len()));
            out.extend_from_slice(&b);
            out.extend_from_slice(&p.bytes);
        }
        Ok(out)
    }

    /// Decode a payload.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < HEADER_LEN {
            return Err(bad("short header"));
        }
        let mut r = Self {
            txn: get_u64(b, 0),
            next_serial: get_u64(b, 8),
            ..Self::default()
        };
        let roots = usize::from(get_u16(b, 16));
        let allocs = usize::from(get_u16(b, 18));
        let frees = usize::from(get_u16(b, 20));
        let images = usize::from(get_u16(b, 22));
        let patches = get_u32(b, 24) as usize;
        let mut at = HEADER_LEN;
        let mut take = |n: usize| -> Result<&[u8]> {
            let s = b.get(at..at + n).ok_or_else(|| bad("truncated"))?;
            at += n;
            Ok(s)
        };
        for _ in 0..roots {
            let s = take(ROOT_LEN)?;
            let root = get_u64(s, 16);
            r.roots
                .push((ObjectId::from_bytes(&s[..16]), (root != 0).then_some(root)));
        }
        for _ in 0..allocs {
            r.allocs.push(get_extent(take(EXTENT_LEN)?));
        }
        for _ in 0..frees {
            r.frees.push(get_extent(take(EXTENT_LEN)?));
        }
        for _ in 0..images {
            let s = take(IMAGE_LEN)?;
            r.images.push((get_u64(s, 0), s[8..].to_vec()));
        }
        for _ in 0..patches {
            let h = take(PATCH_HEAD)?;
            let page = get_u64(h, 0);
            let offset = get_u16(h, 8);
            let len = usize::from(get_u16(h, 10));
            let bytes = take(len)?.to_vec();
            r.patches.push(Patch {
                page,
                offset,
                bytes,
            });
        }
        if at != b.len() {
            return Err(bad("trailing bytes"));
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let r = WalRecord {
            txn: 9,
            next_serial: 42,
            roots: vec![(ObjectId::NONE, Some(77)), (ObjectId::NONE, None)],
            allocs: vec![Extent {
                unit: 0,
                lba: 10,
                len: 3,
            }],
            frees: vec![
                Extent {
                    unit: 1,
                    lba: 5,
                    len: 1,
                },
                Extent {
                    unit: 1,
                    lba: 9,
                    len: 2,
                },
            ],
            images: vec![(100, vec![3u8; PAGE_SIZE])],
            patches: vec![Patch {
                page: 101,
                offset: 16,
                bytes: vec![1, 2, 3],
            }],
        };
        let b = r.encode().unwrap();
        assert_eq!(b.len(), r.encoded_len());
        assert_eq!(WalRecord::decode(&b).unwrap(), r);
        assert!(WalRecord::decode(&b[..b.len() - 1]).is_err());
        assert!(WalRecord::decode(&[0; 10]).is_err());
        let empty = WalRecord::default();
        assert_eq!(WalRecord::decode(&empty.encode().unwrap()).unwrap(), empty);
    }
}

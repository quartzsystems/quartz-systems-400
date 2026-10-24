// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The write-ahead log: a ring of pages on unit 0, one record per
//! transaction. See `docs/STORE_FORMAT.md` §6.

pub mod record;

use crc::{CRC_32_ISCSI, Crc, Table};
use tracing::debug;

use crate::asp::io::AspIo;
use crate::bytes::{get_u16, get_u32, get_u64, put_u16, put_u32, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{BODY_SIZE, Page, PageType};
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

// Slice-by-16: the same CRC, several times faster per page.
const CRC32C: Crc<u32, Table<16>> = Crc::<u32, Table<16>>::new(&CRC_32_ISCSI);

/// Page header bytes.
pub const PAGE_HEADER_LEN: usize = 32;
/// Payload bytes per page.
pub const PAYLOAD: usize = BODY_SIZE - PAGE_HEADER_LEN;

const OFF_SEQ: usize = 0;
const OFF_TXN: usize = 8;
const OFF_TOTAL: usize = 16;
const OFF_OFFSET: usize = 20;
const OFF_LEN: usize = 24;
const OFF_FLAGS: usize = 26;
const OFF_CRC: usize = 28;
const FLAG_FIRST: u16 = 1;
const FLAG_LAST: u16 = 2;

/// The spec layout of the page header.
pub static PAGE_HEADER_LAYOUT: Layout = Layout {
    name: "WalPageHeader",
    size: Some(PAGE_HEADER_LEN),
    fields: &[
        field("seq", OFF_SEQ, 8),
        field("txn", OFF_TXN, 8),
        field("total_len", OFF_TOTAL, 4),
        field("offset", OFF_OFFSET, 4),
        field("len", OFF_LEN, 2),
        field("flags", OFF_FLAGS, 2),
        field("record_crc", OFF_CRC, 4),
    ],
};

/// The ring and its cursors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wal {
    /// First page of the ring (logical).
    pub start: PageNo,
    /// Ring length in pages.
    pub pages: u64,
    /// Next sequence to write.
    pub head: u64,
    /// First sequence recovery would read.
    pub tail: u64,
}

/// A record found by recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Transaction ID from the page headers.
    pub txn: u64,
    /// The payload.
    pub payload: Vec<u8>,
}

impl Wal {
    /// Pages a payload needs.
    #[must_use]
    pub fn pages_for(bytes: usize) -> u64 {
        bytes.div_ceil(PAYLOAD).max(1) as u64
    }

    /// Pages in use between tail and head.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.head - self.tail
    }

    /// Pages still free in the ring.
    #[must_use]
    pub fn free(&self) -> u64 {
        self.pages - self.used()
    }

    /// Ring page holding sequence `seq`.
    fn page_of(&self, seq: u64) -> PageNo {
        self.start + seq % self.pages
    }

    /// Append one record. Every page but the last is written, then every
    /// unit flushed, then the last page written and unit 0 flushed. The
    /// caller has checked that it fits.
    pub fn append(&mut self, io: &mut AspIo, txn: u64, payload: &[u8]) -> Result<()> {
        let n = Self::pages_for(payload.len());
        assert!(n <= self.free(), "WAL append does not fit");
        let crc = CRC32C.checksum(payload);
        let total = u32::try_from(payload.len()).map_err(|_| StoreError::TxnTooLarge {
            bytes: payload.len(),
            ring_bytes: usize::try_from(self.pages)
                .unwrap_or(usize::MAX)
                .saturating_mul(PAYLOAD),
        })?;
        let chunks: Vec<&[u8]> = if payload.is_empty() {
            vec![&[]]
        } else {
            payload.chunks(PAYLOAD).collect()
        };
        for (i, chunk) in chunks.iter().enumerate() {
            let seq = self.head + i as u64;
            let mut page = Page::zeroed();
            let body = page.body_mut();
            put_u64(body, OFF_SEQ, seq);
            put_u64(body, OFF_TXN, txn);
            put_u32(body, OFF_TOTAL, total);
            put_u32(body, OFF_OFFSET, u32::try_from(i * PAYLOAD).expect("fits"));
            put_u16(body, OFF_LEN, u16_of(chunk.len()));
            let mut flags = 0;
            if i == 0 {
                flags |= FLAG_FIRST;
            }
            if i + 1 == chunks.len() {
                flags |= FLAG_LAST;
            }
            put_u16(body, OFF_FLAGS, flags);
            put_u32(body, OFF_CRC, crc);
            body[PAGE_HEADER_LEN..PAGE_HEADER_LEN + chunk.len()].copy_from_slice(chunk);
            if i + 1 == chunks.len() {
                io.flush()?;
            }
            io.write_page(
                self.page_of(seq),
                &mut page,
                PageType::Wal,
                ObjectId::NONE,
                txn,
            )?;
        }
        io.flush_span(0)?;
        self.head += n;
        Ok(())
    }

    /// Read every complete record from the tail. Sets `head` to the
    /// sequence after the last complete record.
    pub fn scan(&mut self, io: &mut AspIo) -> Result<Vec<Found>> {
        let mut found = Vec::new();
        let mut seq = self.tail;
        let mut current: Option<(u64, Vec<u8>, u32, u32)> = None; // txn, payload, total, crc
        loop {
            if seq - self.tail >= self.pages {
                break;
            }
            let pno = self.page_of(seq);
            let Ok(page) = io.read_page(pno) else { break };
            if page.trailer().page_type != PageType::Wal.as_u16() {
                break;
            }
            let body = page.body();
            if get_u64(body, OFF_SEQ) != seq {
                break;
            }
            let txn = get_u64(body, OFF_TXN);
            let total = get_u32(body, OFF_TOTAL);
            let offset = get_u32(body, OFF_OFFSET);
            let len = usize::from(get_u16(body, OFF_LEN));
            let flags = get_u16(body, OFF_FLAGS);
            let crc = get_u32(body, OFF_CRC);
            if len > PAYLOAD {
                break;
            }
            let chunk = &body[PAGE_HEADER_LEN..PAGE_HEADER_LEN + len];
            if flags & FLAG_FIRST != 0 {
                if offset != 0 {
                    break;
                }
                current = Some((txn, chunk.to_vec(), total, crc));
            } else {
                let Some((t, payload, tot, c)) = current.as_mut() else {
                    break;
                };
                if *t != txn || *tot != total || *c != crc || payload.len() != offset as usize {
                    break;
                }
                payload.extend_from_slice(chunk);
            }
            seq += 1;
            if flags & FLAG_LAST != 0 {
                let (t, payload, tot, c) = current.take().expect("set above");
                if payload.len() != tot as usize || CRC32C.checksum(&payload) != c {
                    break;
                }
                found.push(Found { txn: t, payload });
                self.head = seq;
            }
        }
        if self.head < self.tail {
            self.head = self.tail;
        }
        debug!(
            "wal: {} complete record(s) from seq {}",
            found.len(),
            self.tail
        );
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asp::layout::Layout as AspLayout;
    use blockdev::{BlockDev, MemBlockDev};
    use std::sync::Arc;

    fn io(dev: &Arc<MemBlockDev>) -> AspIo {
        AspIo::new(vec![Box::new(dev.clone())], AspLayout::new(&[dev.blocks()]))
    }

    #[test]
    fn append_scan_and_wrap() {
        let dev = Arc::new(MemBlockDev::new(64));
        let mut io = io(&dev);
        let mut wal = Wal {
            start: 3,
            pages: 8,
            head: 0,
            tail: 0,
        };
        let big = vec![7u8; PAYLOAD * 2 + 5];
        wal.append(&mut io, 1, b"one").unwrap();
        wal.append(&mut io, 2, &big).unwrap();
        wal.append(&mut io, 3, &[]).unwrap();
        assert_eq!(wal.used(), 5);
        let mut again = Wal {
            head: 0,
            ..wal.clone()
        };
        let found = again.scan(&mut io).unwrap();
        assert_eq!(found.len(), 3);
        assert_eq!(
            found[0],
            Found {
                txn: 1,
                payload: b"one".to_vec()
            }
        );
        assert_eq!(found[1].payload, big);
        assert_eq!(found[2].payload, Vec::<u8>::new());
        assert_eq!(again.head, 5);
        // Advance the tail (a checkpoint), wrap around the ring.
        wal.tail = wal.head;
        wal.append(&mut io, 4, &big).unwrap();
        wal.append(&mut io, 5, b"five").unwrap();
        assert_eq!(wal.head, 9);
        let mut again = Wal {
            head: 5,
            tail: 5,
            ..wal.clone()
        };
        let found = again.scan(&mut io).unwrap();
        assert_eq!(found.iter().map(|f| f.txn).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(again.head, 9);
    }

    #[test]
    fn torn_last_page_drops_the_record_and_everything_after() {
        let dev = Arc::new(MemBlockDev::with_seed(64, 5));
        let mut io = io(&dev);
        let mut wal = Wal {
            start: 3,
            pages: 8,
            head: 0,
            tail: 0,
        };
        wal.append(&mut io, 1, b"first").unwrap();
        let big = vec![1u8; PAYLOAD + 10];
        dev.tear_at(Some(dev.stats().writes + 2));
        wal.append(&mut io, 2, &big).unwrap();
        dev.tear_at(None);
        wal.append(&mut io, 3, b"third").unwrap();
        let mut again = Wal {
            head: 0,
            ..wal.clone()
        };
        let found = again.scan(&mut io).unwrap();
        assert_eq!(found.iter().map(|f| f.txn).collect::<Vec<_>>(), vec![1]);
        assert_eq!(again.head, 1);
    }
}

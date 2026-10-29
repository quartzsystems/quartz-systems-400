// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended result data (`0x380E`): a 20-byte header, the null
//! indicators (rows × columns × indicator size), then the rows: fixed
//! `row_size` records, or with variable-length field compression a row
//! information header and an array of row offsets.

use crate::Error;
use crate::template::{Template as _, template};

template! {
    base 0;
    /// The result data header.
    pub struct DataHeader {
        /// Consistency token.
        consistency: u32,
        /// Rows in this block.
        row_count: u32,
        /// Columns per row.
        column_count: u16,
        /// Bytes per null indicator (2).
        indicator_size: u16,
        /// Reserved (byte 0 is 0x80 when the client set it).
        reserved: u32,
        /// Bytes per row (uncompressed).
        row_size: u32,
    }
}

/// Decoded result data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultData {
    /// The header.
    pub header: DataHeader,
    /// Indicators and rows, as sent.
    pub body: Vec<u8>,
}

impl ResultData {
    /// Decode a `0x380E` parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        let h = b
            .get(..DataHeader::SIZE)
            .and_then(DataHeader::decode)
            .ok_or(Error::Short {
                what: "result data",
                need: DataHeader::SIZE,
                have: b.len(),
            })?;
        let ind =
            h.row_count as usize * usize::from(h.column_count) * usize::from(h.indicator_size);
        if DataHeader::SIZE + ind > b.len() {
            return Err(Error::Short {
                what: "result indicators",
                need: DataHeader::SIZE + ind,
                have: b.len(),
            });
        }
        Ok(Self {
            header: h,
            body: b[DataHeader::SIZE..].to_vec(),
        })
    }

    /// The data.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.header.encode();
        v.extend_from_slice(&self.body);
        v
    }

    fn indicator_bytes(&self) -> usize {
        self.header.row_count as usize
            * usize::from(self.header.column_count)
            * usize::from(self.header.indicator_size)
    }

    /// Whether column `c` of row `r` is null (indicator negative).
    pub fn is_null(&self, r: usize, c: usize) -> bool {
        let size = usize::from(self.header.indicator_size);
        let at = (r * usize::from(self.header.column_count) + c) * size;
        self.body.get(at).is_some_and(|b| b & 0x80 != 0)
    }

    /// Row `r` of fixed-size rows (no variable-length compression).
    pub fn row(&self, r: usize) -> Option<&[u8]> {
        let size = self.header.row_size as usize;
        let at = self.indicator_bytes() + r * size;
        (r < self.header.row_count as usize)
            .then(|| self.body.get(at..at + size))
            .flatten()
    }

    /// Row `r` when variable-length fields are compressed: the row
    /// information header (u32 offset to the offset array, relative to
    /// itself) and the array of row offsets (relative to the header).
    pub fn compressed_row(&self, r: usize) -> Option<&[u8]> {
        if r >= self.header.row_count as usize {
            return None;
        }
        let head = self.indicator_bytes();
        let rd = |at: usize| {
            self.body
                .get(at..at + 4)
                .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
        };
        let array = head + rd(head)?;
        let start = head + rd(array + 4 * r)?;
        let end = if r + 1 < self.header.row_count as usize {
            head + rd(array + 4 * (r + 1))?
        } else {
            array
        };
        self.body.get(start..end.max(start))
    }
}

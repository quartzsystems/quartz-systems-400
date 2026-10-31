// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data part of CPYTOTAP and CPYFRMTAP: records into blocks by the
//! HDR2 record format and lengths, and back.
//!
//! - F: records padded to the record length, as many per block as the
//!   block length holds; the last block may be short.
//! - V: a 4-byte block descriptor (big-endian length including itself)
//!   then records, each behind a 4-byte record descriptor (length
//!   including itself). The record length counts the descriptor.
//! - U: one record per block.

use crate::TapeError;
use crate::label::RecordFormat;
use crate::transport::MAX_BLOCK;

fn desc(len: usize) -> [u8; 4] {
    let n = u16::try_from(len).expect("descriptor length");
    let b = n.to_be_bytes();
    [b[0], b[1], 0, 0]
}

fn check_lengths(rec_len: usize, block_len: usize) -> Result<(), TapeError> {
    if block_len == 0 || block_len > MAX_BLOCK || rec_len == 0 {
        return Err(TapeError::Parameter {
            param: "BLKLEN",
            why: format!("block length {block_len}, record length {rec_len}"),
        });
    }
    Ok(())
}

/// Records into blocks. `pad` fills short fixed records.
pub fn records_to_blocks<'r>(
    records: impl IntoIterator<Item = &'r [u8]>,
    format: RecordFormat,
    rec_len: usize,
    block_len: usize,
    pad: u8,
) -> Result<Vec<Vec<u8>>, TapeError> {
    check_lengths(rec_len, block_len)?;
    let mut blocks = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    match format {
        RecordFormat::Fixed => {
            let per = block_len / rec_len;
            if per == 0 {
                return Err(TapeError::RecordLength {
                    len: rec_len,
                    max: block_len,
                });
            }
            for r in records {
                if r.len() > rec_len {
                    return Err(TapeError::RecordLength {
                        len: r.len(),
                        max: rec_len,
                    });
                }
                cur.extend_from_slice(r);
                cur.resize(cur.len() + rec_len - r.len(), pad);
                if cur.len() == per * rec_len {
                    blocks.push(std::mem::take(&mut cur));
                }
            }
        }
        RecordFormat::Variable => {
            if block_len > usize::from(u16::MAX) || rec_len + 4 > block_len {
                return Err(TapeError::RecordLength {
                    len: rec_len,
                    max: block_len.min(usize::from(u16::MAX)),
                });
            }
            for r in records {
                let n = r.len() + 4;
                if n > rec_len {
                    return Err(TapeError::RecordLength {
                        len: r.len(),
                        max: rec_len - 4,
                    });
                }
                if !cur.is_empty() && cur.len() + n > block_len {
                    let len = cur.len();
                    cur[..4].copy_from_slice(&desc(len));
                    blocks.push(std::mem::take(&mut cur));
                }
                if cur.is_empty() {
                    cur.extend_from_slice(&[0; 4]);
                }
                cur.extend_from_slice(&desc(n));
                cur.extend_from_slice(r);
            }
            if !cur.is_empty() {
                let len = cur.len();
                cur[..4].copy_from_slice(&desc(len));
            }
        }
        RecordFormat::Undefined => {
            for r in records {
                if r.is_empty() || r.len() > block_len {
                    return Err(TapeError::RecordLength {
                        len: r.len(),
                        max: block_len,
                    });
                }
                blocks.push(r.to_vec());
            }
        }
    }
    if !cur.is_empty() {
        blocks.push(cur);
    }
    Ok(blocks)
}

/// Blocks back into records.
pub fn blocks_to_records<'b>(
    blocks: impl IntoIterator<Item = &'b [u8]>,
    format: RecordFormat,
    rec_len: usize,
) -> Result<Vec<Vec<u8>>, TapeError> {
    let bad = |len, max| TapeError::RecordLength { len, max };
    let mut out = Vec::new();
    for b in blocks {
        match format {
            RecordFormat::Fixed => {
                if rec_len == 0 || b.len() % rec_len != 0 {
                    return Err(bad(b.len(), rec_len));
                }
                out.extend(b.chunks(rec_len).map(<[u8]>::to_vec));
            }
            RecordFormat::Variable => {
                let len = |at: usize| usize::from(u16::from_be_bytes([b[at], b[at + 1]]));
                if b.len() < 4 || len(0) != b.len() {
                    return Err(bad(b.len(), rec_len));
                }
                let mut at = 4;
                while at < b.len() {
                    if at + 4 > b.len() {
                        return Err(bad(b.len(), rec_len));
                    }
                    let n = len(at);
                    if n < 4 || at + n > b.len() {
                        return Err(bad(n, rec_len));
                    }
                    out.push(b[at + 4..at + n].to_vec());
                    at += n;
                }
            }
            RecordFormat::Undefined => out.push(b.to_vec()),
        }
    }
    Ok(out)
}

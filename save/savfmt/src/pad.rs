// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A finished stream padded to whole save file records: blanks added to
//! its `Parameters` record, the index offsets and the trailer moved by as
//! many bytes. Nothing else changes, so every object digest and signature
//! still holds (`docs/SAVE_FORMAT.md` §1, "Whole records").

use crate::be::{get64, put64};
use crate::error::{Error, Result};
use crate::header::{self, MediaHeader};
use crate::object::INDEX_LEN;
use crate::record::{self, RecordHeader, RecordKind, crc32c};

/// Bytes per save file record.
pub const SAVF_RECORD: usize = 528;

/// `stream` padded to a multiple of `unit` bytes. A stream already a
/// multiple comes back unchanged.
pub fn pad(stream: &[u8], unit: usize) -> Result<Vec<u8>> {
    MediaHeader::decode(stream)?;
    let add = (unit - stream.len() % unit) % unit;
    if add == 0 {
        return Ok(stream.to_vec());
    }
    let mut out = Vec::with_capacity(stream.len() + add);
    out.extend_from_slice(&stream[..header::LEN]);
    let mut at = header::LEN;
    let mut first = true;
    let shift = add as u64;
    while at < stream.len() {
        let (h, payload) = next(stream, at)?;
        let end = at + record::LEN + payload.len();
        match h.kind {
            RecordKind::Parameters if first => {
                let mut p = payload.to_vec();
                p.resize(p.len() + add, b' ');
                put_record(&mut out, h, &p);
            }
            _ if first => return Err(Error::Damaged { offset: at as u64 }),
            RecordKind::Index => {
                let mut p = payload.to_vec();
                for e in p.as_chunks_mut::<INDEX_LEN>().0 {
                    let o = get64(e, 8);
                    put64(e, 8, o + shift);
                }
                put_record(&mut out, h, &p);
            }
            RecordKind::Trailer => {
                let mut p = payload.to_vec();
                let index = get64(&p, 8);
                if index != 0 {
                    put64(&mut p, 8, index + shift);
                }
                let bytes = get64(&p, 24);
                put64(&mut p, 24, bytes + shift);
                put_record(&mut out, h, &p);
            }
            _ => out.extend_from_slice(&stream[at..end]),
        }
        first = false;
        at = end;
    }
    Ok(out)
}

fn next(stream: &[u8], at: usize) -> Result<(RecordHeader, &[u8])> {
    let damaged = Error::Damaged { offset: at as u64 };
    let b: &[u8; record::LEN] = stream
        .get(at..at + record::LEN)
        .and_then(|s| s.try_into().ok())
        .ok_or(Error::Truncated)?;
    let h = RecordHeader::decode(b).ok_or(damaged)?;
    let start = at + record::LEN;
    let payload = stream
        .get(start..start + h.stored_len as usize)
        .ok_or(Error::Truncated)?;
    if crc32c(payload) != h.crc {
        return Err(Error::BadCrc { seq: h.seq });
    }
    Ok((h, payload))
}

fn put_record(out: &mut Vec<u8>, mut h: RecordHeader, payload: &[u8]) {
    let n = u32::try_from(payload.len()).expect("record payload");
    h.stored_len = n;
    h.raw_len = n;
    h.crc = crc32c(payload);
    out.extend_from_slice(&h.encode());
    out.extend_from_slice(payload);
}

/// Whether `stream` is whole save file records.
#[must_use]
pub fn whole(stream: &[u8]) -> bool {
    stream.len().is_multiple_of(SAVF_RECORD)
}

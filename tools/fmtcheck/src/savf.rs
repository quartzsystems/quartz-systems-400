// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A save stream's bytes against the frozen tables of
//! `docs/SAVE_FORMAT.md`, read here a second time and apart from
//! `savfmt`, so that a packager that drifts from the document is caught
//! even when the system's own reader drifted with it. The structure is
//! checked: every header, record, object group, the index and the
//! trailer. Object digests and signatures are left to the readers.

// Counts are u32 in the format; the walk reads best as one function.
#![allow(
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::many_single_char_names
)]

use std::collections::BTreeMap;

/// Format versions frozen at V1R1M0: 2 written, 1 read.
pub const VERSIONS: [u16; 2] = [1, 2];
/// Bytes per save file record.
pub const SAVF_RECORD: usize = 528;
/// The kind of a PTF stream.
pub const KIND_PTF: u8 = 8;

const HEADER: usize = 128;
const RECORD: usize = 24;
const DESCRIPTOR: usize = 512;
const CHUNK: usize = 24;
const INDEX_ENTRY: usize = 64;
const MAX_RAW: u32 = 1 << 20;

/// The first thing wrong with a stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("offset {offset}: {what}")]
pub struct Violation {
    /// Stream offset of the field or record.
    pub offset: u64,
    /// What is wrong.
    pub what: String,
}

/// What a stream holds, when it passes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// The format version.
    pub version: u16,
    /// The media header's kind.
    pub kind: u8,
    /// Objects in the stream.
    pub objects: u32,
    /// Records after the media header, the trailer's too.
    pub records: u32,
}

type Check<T> = Result<T, Violation>;

fn bad<T>(offset: usize, what: impl Into<String>) -> Check<T> {
    Err(Violation {
        offset: offset as u64,
        what: what.into(),
    })
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().expect("8 bytes"))
}

/// CRC32C (Castagnoli), bit by bit: speed does not matter here.
#[must_use]
pub fn crc32c(b: &[u8]) -> u32 {
    let mut c = !0u32;
    for &x in b {
        c ^= u32::from(x);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0x82F6_3B78 } else { c >> 1 };
        }
    }
    !c
}

fn zero(b: &[u8], base: usize, at: usize, len: usize, name: &str) -> Check<()> {
    if b[at..at + len].iter().any(|&x| x != 0) {
        return bad(base + at, format!("{name} is not zero"));
    }
    Ok(())
}

/// One record as found.
struct Rec<'a> {
    at: usize,
    kind: u8,
    codec: u8,
    seq: u32,
    raw_len: u32,
    payload: &'a [u8],
}

fn record(s: &[u8], at: usize) -> Check<Rec<'_>> {
    let Some(h) = s.get(at..at + RECORD) else {
        return bad(at, "the stream ends inside a record header");
    };
    if &h[0..2] != b"QR" {
        return bad(at, "record mark is not QR");
    }
    if crc32c(&h[..20]) != u32_at(h, 20) {
        return bad(at + 20, "record header CRC does not match");
    }
    let kind = h[2];
    if !matches!(kind, 0x01 | 0x10..=0x14 | 0x20 | 0x21) {
        return bad(at + 2, format!("unknown record kind 0x{kind:02X}"));
    }
    let codec = h[3];
    if codec > 2 || (codec != 0 && kind != 0x12) {
        return bad(at + 3, format!("codec {codec} on a record of kind 0x{kind:02X}"));
    }
    let stored = u32_at(h, 8);
    let raw_len = u32_at(h, 12);
    if raw_len > MAX_RAW + CHUNK as u32 {
        return bad(at + 12, format!("raw_len {raw_len} is over the limit"));
    }
    if codec == 0 && stored != raw_len {
        return bad(at + 8, "stored_len differs from raw_len in a stored record");
    }
    let start = at + RECORD;
    let Some(payload) = s.get(start..start + stored as usize) else {
        return bad(at + 8, "the stream ends inside the record's payload");
    };
    if crc32c(payload) != u32_at(h, 16) {
        return bad(at + 16, "payload CRC does not match");
    }
    Ok(Rec {
        at,
        kind,
        codec,
        seq: u32_at(h, 4),
        raw_len,
        payload,
    })
}

fn media_header(s: &[u8]) -> Check<(u16, u8, [u8; 16])> {
    let Some(h) = s.get(..HEADER) else {
        return bad(0, "shorter than a media header");
    };
    if &h[0..8] != b"QS400SAV" {
        return bad(0, "magic is not QS400SAV");
    }
    if crc32c(&h[..124]) != u32_at(h, 124) {
        return bad(124, "media header CRC does not match");
    }
    let version = u16_at(h, 8);
    if !VERSIONS.contains(&version) {
        return bad(8, format!("format version {version} is not frozen"));
    }
    let flags = u16_at(h, 10);
    if flags != 0 {
        return bad(10, format!("flags 0x{flags:04X} are reserved"));
    }
    if h[18] > 3 {
        return bad(18, format!("compression {} is unknown", h[18]));
    }
    if h[19] > 9 {
        return bad(19, format!("kind {} is unknown", h[19]));
    }
    zero(h, 0, 20, 4, "reserved")?;
    zero(h, 0, 86, 38, "reserved2")?;
    Ok((version, h[19], h[32..48].try_into().expect("16 bytes")))
}

/// The open object.
struct Open {
    index: u32,
    flags: u16,
    part_count: u16,
    size: u64,
    auths_due: bool,
    parts: BTreeMap<u8, (u64, u64)>,
}

/// Check `s`, the whole stream.
pub fn check(s: &[u8]) -> Check<Summary> {
    let (version, kind, stream_id) = media_header(s)?;
    let mut at = HEADER;
    let mut seq = 0u32;
    let mut objects: Vec<(usize, u16, u16)> = Vec::new();
    let mut open: Option<Open> = None;
    let mut sig_due: Option<(u32, [u8; 32])> = None;
    let mut index: Vec<&[u8]> = Vec::new();
    let mut index_at = 0usize;
    let mut index_records = 0u32;
    loop {
        let r = record(s, at)?;
        if r.seq != seq {
            return bad(r.at + 4, format!("seq {} where {seq} was due", r.seq));
        }
        let p = r.payload;
        let base = r.at + RECORD;
        if seq == 0 && r.kind != 0x01 {
            return bad(r.at + 2, "the first record is not Parameters");
        }
        if !index.is_empty() && !matches!(r.kind, 0x20 | 0x21) {
            return bad(r.at + 2, "a record after the index");
        }
        if sig_due.is_some() && r.kind != 0x14 {
            return bad(r.at + 2, "the object's signature record is missing");
        }
        match r.kind {
            0x01 => {
                if seq != 0 {
                    return bad(r.at + 2, "a second Parameters record");
                }
                if std::str::from_utf8(p).is_err() {
                    return bad(base, "Parameters is not UTF-8");
                }
            }
            0x10 => {
                if open.is_some() {
                    return bad(r.at, "an object begins before the last one ended");
                }
                if p.len() < DESCRIPTOR {
                    return bad(base, "the object record is shorter than a descriptor");
                }
                let want = objects.len() as u32;
                if u32_at(p, 0) != want {
                    return bad(base, format!("object index {} where {want} was due", u32_at(p, 0)));
                }
                let owner = u32_at(p, 4);
                if owner != u32::MAX && owner >= want {
                    return bad(base + 4, format!("owner {owner} does not come first"));
                }
                zero(p, base, 14, 2, "descriptor reserved")?;
                zero(p, base, 169, 7, "descriptor reserved2")?;
                zero(p, base, 272, 240, "descriptor reserved3")?;
                let tail = usize::from(u16_at(p, 266));
                if p.len() != DESCRIPTOR + tail {
                    return bad(base + 266, format!("tail_len {tail} does not fill the record"));
                }
                let flags = u16_at(p, 10);
                if flags >> 6 != 0 {
                    return bad(base + 10, format!("unknown descriptor flags 0x{flags:04X}"));
                }
                objects.push((r.at, u16_at(p, 8), flags));
                open = Some(Open {
                    index: want,
                    flags,
                    part_count: u16_at(p, 264),
                    size: u64_at(p, 256),
                    auths_due: flags & 1 << 3 != 0,
                    parts: BTreeMap::new(),
                });
            }
            0x11 => {
                let Some(o) = open.as_mut().filter(|o| o.auths_due && o.parts.is_empty()) else {
                    return bad(r.at + 2, "an Authorities record out of place");
                };
                if p.len() < 8 || u32_at(p, 0) != o.index {
                    return bad(base, "the Authorities record names another object");
                }
                if p.len() != 8 + 16 * u32_at(p, 4) as usize {
                    return bad(base + 4, "the authority count does not fill the record");
                }
                o.auths_due = false;
            }
            0x12 => {
                let Some(o) = open.as_mut() else {
                    return bad(r.at + 2, "a Data record outside an object");
                };
                if o.auths_due {
                    return bad(r.at + 2, "Data before the object's Authorities");
                }
                if p.len() < CHUNK || (r.raw_len as usize) < CHUNK {
                    return bad(base, "a Data record shorter than its chunk header");
                }
                if u32_at(p, 0) != o.index {
                    return bad(base, "a chunk names another object");
                }
                zero(p, base, 5, 3, "chunk reserved")?;
                let part = p[4];
                let (offset, len) = (u64_at(p, 8), u64_at(p, 16));
                let chunk = u64::from(r.raw_len) - CHUNK as u64;
                if r.codec == 0 && p.len() != r.raw_len as usize {
                    return bad(r.at + 8, "a stored chunk's lengths differ");
                }
                if let Some((&last, _)) = o.parts.last_key_value()
                    && part < last
                {
                    return bad(base + 4, format!("part {part} after part {last}"));
                }
                let e = o.parts.entry(part).or_insert((0, len));
                if e.1 != len || offset != e.0 || offset + chunk > len {
                    return bad(base + 8, format!("part {part}'s chunk at {offset} is out of order"));
                }
                e.0 += chunk;
            }
            0x13 => {
                let Some(o) = open.take() else {
                    return bad(r.at + 2, "an ObjectEnd outside an object");
                };
                if p.len() != 48 || u32_at(p, 0) != o.index {
                    return bad(base, "the ObjectEnd names another object");
                }
                zero(p, base, 6, 2, "ObjectEnd reserved")?;
                for (part, (done, len)) in &o.parts {
                    if done != len {
                        return bad(base, format!("part {part} has {done} of {len} bytes"));
                    }
                }
                let parts = o.parts.len();
                if usize::from(u16_at(p, 4)) != parts || usize::from(o.part_count) != parts {
                    return bad(base + 4, format!("{parts} parts, the descriptor says {}", o.part_count));
                }
                let bytes: u64 = o.parts.values().map(|v| v.1).sum();
                if u64_at(p, 8) != bytes || o.size != bytes {
                    return bad(base + 8, format!("{bytes} part bytes, the descriptor says {}", o.size));
                }
                if o.flags & 1 << 1 != 0 {
                    sig_due = Some((o.index, p[16..48].try_into().expect("32 bytes")));
                }
            }
            0x14 => {
                let Some((i, digest)) = sig_due.take() else {
                    return bad(r.at + 2, "a Signature record for an unsigned object");
                };
                if p.len() != 112 || u32_at(p, 0) != i {
                    return bad(base, "the Signature names another object");
                }
                if u16_at(p, 4) != 1 {
                    return bad(base + 4, format!("signature algorithm {}", u16_at(p, 4)));
                }
                zero(p, base, 6, 2, "signature reserved")?;
                if p[80..112] != digest {
                    return bad(base + 80, "the signed digest is not the object's");
                }
            }
            0x20 => {
                if open.is_some() {
                    return bad(r.at + 2, "the index begins inside an object");
                }
                if p.is_empty() || !p.len().is_multiple_of(INDEX_ENTRY) {
                    return bad(r.at + 12, "an index record is not whole entries");
                }
                if index.is_empty() {
                    index_at = r.at;
                }
                index.extend(p.chunks(INDEX_ENTRY));
                index_records += 1;
            }
            0x21 => {
                if open.is_some() {
                    return bad(r.at + 2, "the trailer comes inside an object");
                }
                trailer(p, base, r.at, seq, &objects, &index, index_at, index_records, stream_id)?;
                let end = base + p.len();
                if end != s.len() {
                    return bad(end, format!("{} bytes after the trailer", s.len() - end));
                }
                // Last, so a PTF that fails only this shows it is sound otherwise.
                if kind == KIND_PTF && !s.len().is_multiple_of(SAVF_RECORD) {
                    return bad(
                        s.len(),
                        format!(
                            "a PTF is {} bytes, not whole {SAVF_RECORD}-byte records",
                            s.len()
                        ),
                    );
                }
                return Ok(Summary {
                    version,
                    kind,
                    objects: objects.len() as u32,
                    records: seq + 1,
                });
            }
            _ => unreachable!("kinds checked"),
        }
        at = base + p.len();
        seq += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn trailer(
    p: &[u8],
    base: usize,
    at: usize,
    seq: u32,
    objects: &[(usize, u16, u16)],
    index: &[&[u8]],
    index_at: usize,
    index_records: u32,
    stream_id: [u8; 16],
) -> Check<()> {
    if p.len() != 64 {
        return bad(base, "the trailer is not 64 bytes");
    }
    if u32_at(p, 0) as usize != objects.len() {
        return bad(base, format!("trailer says {} objects, the stream has {}", u32_at(p, 0), objects.len()));
    }
    if index.len() != objects.len() {
        return bad(index_at, format!("{} index entries for {} objects", index.len(), objects.len()));
    }
    for (i, (e, (obj_at, ty, flags))) in index.iter().zip(objects).enumerate() {
        let e_at = index_at + RECORD + i * INDEX_ENTRY;
        if u32_at(e, 0) as usize != i {
            return bad(e_at, format!("index entry {i} names object {}", u32_at(e, 0)));
        }
        if u64_at(e, 8) != *obj_at as u64 {
            return bad(e_at + 8, format!("index entry {i} points at {}, the object is at {obj_at}", u64_at(e, 8)));
        }
        if u16_at(e, 16) != *ty || u16_at(e, 18) != *flags {
            return bad(e_at + 16, format!("index entry {i} differs from its descriptor"));
        }
        zero(e, e_at, 50, 6, "index reserved")?;
    }
    let want_index = if index.is_empty() { 0 } else { index_at as u64 };
    if u64_at(p, 8) != want_index {
        return bad(base + 8, format!("index_offset {}, the index is at {want_index}", u64_at(p, 8)));
    }
    if u32_at(p, 16) != seq {
        return bad(base + 16, format!("trailer counts {} records, {seq} came before it", u32_at(p, 16)));
    }
    if u32_at(p, 20) != index_records {
        return bad(base + 20, format!("trailer counts {} index records, {index_records} came", u32_at(p, 20)));
    }
    if u64_at(p, 24) != at as u64 {
        return bad(base + 24, format!("trailer says {} bytes before it, there are {at}", u64_at(p, 24)));
    }
    if p[32..48] != stream_id {
        return bad(base + 32, "the trailer's stream_id is not the media header's");
    }
    zero(p, base, 48, 16, "trailer reserved")
}

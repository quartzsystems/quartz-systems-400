// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading a stream: forward, object by object, or by its index when
//! the medium seeks.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Seek, SeekFrom};

use crate::codec::unpack;
use crate::error::{Error, Result};
use crate::header::{self, MediaHeader};
use crate::object::{
    self, AuthorityEntry, ChunkHeader, IndexEntry, ObjectDescriptor, ObjectEnd, Part,
    SignatureRecord, Trailer, flags,
};
use crate::record::{self, RecordHeader, RecordKind, crc32c};

/// One object read back whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedObject {
    /// Its descriptor.
    pub descriptor: ObjectDescriptor,
    /// The descriptor's tail (an IFS path).
    pub tail: Vec<u8>,
    /// Its private authorities.
    pub authorities: Vec<AuthorityEntry>,
    /// Its parts.
    pub parts: BTreeMap<Part, Vec<u8>>,
    /// Its digest.
    pub digest: [u8; 32],
    /// Its signature, when it has one.
    pub signature: Option<SignatureRecord>,
    /// Stream offset of its first record.
    pub offset: u64,
}

impl SavedObject {
    /// A part's bytes, empty when the part was not saved.
    #[must_use]
    pub fn part(&self, p: Part) -> &[u8] {
        self.parts.get(&p).map_or(&[], Vec::as_slice)
    }

    /// The IFS path of an IFS object.
    #[must_use]
    pub fn path(&self) -> Option<String> {
        self.descriptor
            .has(flags::IFS)
            .then(|| String::from_utf8_lossy(&self.tail).into_owned())
    }
}

/// What the next object in the stream turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scan {
    /// A whole, verified object.
    Object(Box<SavedObject>),
    /// An object that is damaged; the descriptor when it could be read.
    Damaged(Option<Box<ObjectDescriptor>>),
}

struct Raw {
    h: RecordHeader,
    offset: u64,
    stored: Vec<u8>,
    ok: bool,
}

/// A forward reader over any `Read`.
pub struct Reader<R: Read> {
    r: R,
    header: MediaHeader,
    offset: u64,
    next_seq: Option<u32>,
    parameters: Option<String>,
    trailer: Option<Trailer>,
    pending: Option<Raw>,
}

impl<R: Read> Reader<R> {
    /// Open a stream at its start: reads the media header.
    pub fn new(mut r: R) -> Result<Self> {
        let mut b = [0u8; header::LEN];
        read_full(&mut r, &mut b).map_err(|e| match e {
            Error::Truncated => Error::NotSave,
            e => e,
        })?;
        let header = MediaHeader::decode(&b)?;
        Ok(Self {
            r,
            header,
            offset: header::LEN as u64,
            next_seq: Some(0),
            parameters: None,
            trailer: None,
            pending: None,
        })
    }

    /// Continue a stream whose medium was positioned at `offset` (a
    /// record boundary from the index).
    pub fn positioned(r: R, header: MediaHeader, offset: u64) -> Self {
        Self {
            r,
            header,
            offset,
            next_seq: None,
            parameters: None,
            trailer: None,
            pending: None,
        }
    }

    /// The media header.
    #[must_use]
    pub fn header(&self) -> &MediaHeader {
        &self.header
    }

    /// The save command, once its record was read.
    #[must_use]
    pub fn parameters(&self) -> Option<&str> {
        self.parameters.as_deref()
    }

    /// The trailer, once reached.
    #[must_use]
    pub fn trailer(&self) -> Option<&Trailer> {
        self.trailer.as_ref()
    }

    /// The medium.
    pub fn into_inner(self) -> R {
        self.r
    }

    fn read_raw(&mut self) -> Result<Raw> {
        if let Some(p) = self.pending.take() {
            return Ok(p);
        }
        let offset = self.offset;
        let mut hb = [0u8; record::LEN];
        read_full(&mut self.r, &mut hb)?;
        let h = RecordHeader::decode(&hb).ok_or(Error::Damaged { offset })?;
        if self.next_seq.is_some_and(|s| s != h.seq) {
            return Err(Error::Damaged { offset });
        }
        let mut stored = vec![0u8; h.stored_len as usize];
        read_full(&mut self.r, &mut stored)?;
        self.offset += (record::LEN + stored.len()) as u64;
        self.next_seq = Some(h.seq.wrapping_add(1));
        let ok = crc32c(&stored) == h.crc;
        Ok(Raw {
            h,
            offset,
            stored,
            ok,
        })
    }

    /// The next object, or `None` at the trailer. An error is damage
    /// that stops the stream (a bad record header, a missing volume).
    pub fn next_object(&mut self) -> Result<Option<Scan>> {
        loop {
            if self.trailer.is_some() {
                return Ok(None);
            }
            let raw = self.read_raw()?;
            match raw.h.kind {
                RecordKind::Parameters => {
                    if raw.ok {
                        self.parameters = Some(String::from_utf8_lossy(&raw.stored).into_owned());
                    }
                }
                RecordKind::Index => {}
                RecordKind::Trailer => {
                    let t = Trailer::decode(&raw.stored)
                        .ok()
                        .filter(|_| raw.ok)
                        .ok_or(Error::Damaged { offset: raw.offset })?;
                    let consistent = t.stream_id == self.header.stream_id
                        && t.bytes == raw.offset
                        && self.next_seq.is_none_or(|s| s == t.records + 1);
                    if !consistent {
                        return Err(Error::Damaged { offset: raw.offset });
                    }
                    self.trailer = Some(t);
                }
                RecordKind::Object => return self.object(&raw).map(Some),
                _ => return Err(Error::Damaged { offset: raw.offset }),
            }
        }
    }

    fn object(&mut self, raw: &Raw) -> Result<Scan> {
        let first = raw.offset;
        let mut desc = None;
        let mut damaged = !raw.ok;
        let mut tail = Vec::new();
        if raw.ok {
            match ObjectDescriptor::decode(&raw.stored) {
                Ok(d) if raw.stored.len() == object::DESCRIPTOR_LEN + usize::from(d.tail_len) => {
                    tail = raw.stored[object::DESCRIPTOR_LEN..].to_vec();
                    desc = Some(d);
                }
                _ => damaged = true,
            }
        }
        let mut auths = Vec::new();
        let mut parts: BTreeMap<Part, Vec<u8>> = BTreeMap::new();
        let mut lens: BTreeMap<Part, u64> = BTreeMap::new();
        loop {
            let raw = self.read_raw()?;
            match raw.h.kind {
                RecordKind::Authorities if raw.ok => match object::decode_auths(&raw.stored) {
                    Ok((i, a)) if desc.as_ref().is_some_and(|d| d.index == i) => auths = a,
                    _ => damaged = true,
                },
                RecordKind::Data if raw.ok => {
                    if !Self::chunk(&raw, desc.as_ref(), &mut parts, &mut lens) {
                        damaged = true;
                    }
                }
                RecordKind::Authorities | RecordKind::Data | RecordKind::Signature => {
                    damaged = true;
                }
                RecordKind::ObjectEnd => {
                    let end = ObjectEnd::decode(&raw.stored).ok().filter(|_| raw.ok);
                    let signature = self.signature_after()?;
                    let Some((end, d)) = end.zip(desc.clone()).filter(|_| !damaged) else {
                        return Ok(Scan::Damaged(desc.map(Box::new)));
                    };
                    let whole = end.index == d.index
                        && usize::from(end.parts) == parts.len()
                        && end.parts == d.part_count
                        && parts.values().map(|p| p.len() as u64).sum::<u64>() == end.bytes
                        && end.bytes == d.size_bytes
                        && parts.iter().all(|(k, p)| lens.get(k) == Some(&(p.len() as u64)))
                        && crate::digest(&d, &parts) == end.digest
                        && signature.is_none_or(|s| s.index == d.index && s.digest == end.digest);
                    if !whole {
                        return Ok(Scan::Damaged(Some(Box::new(d))));
                    }
                    return Ok(Scan::Object(Box::new(SavedObject {
                        descriptor: d,
                        tail,
                        authorities: auths,
                        parts,
                        digest: end.digest,
                        signature,
                        offset: first,
                    })));
                }
                _ => {
                    // The next object or the end began without this
                    // one's end record.
                    self.pending = Some(raw);
                    return Ok(Scan::Damaged(desc.map(Box::new)));
                }
            }
        }
    }

    fn chunk(
        raw: &Raw,
        desc: Option<&ObjectDescriptor>,
        parts: &mut BTreeMap<Part, Vec<u8>>,
        lens: &mut BTreeMap<Part, u64>,
    ) -> bool {
        let (Some(d), Ok(ch)) = (desc, ChunkHeader::decode(&raw.stored)) else {
            return false;
        };
        if ch.index != d.index || ch.part_len > d.size_bytes {
            return false;
        }
        let raw_len = (raw.h.raw_len as usize).saturating_sub(object::CHUNK_LEN);
        let Ok(body) = unpack(raw.h.codec, &raw.stored[object::CHUNK_LEN..], raw_len) else {
            return false;
        };
        let buf = parts.entry(ch.part).or_insert_with(|| {
            Vec::with_capacity(usize::try_from(ch.part_len).unwrap_or(0).min(record::MAX_RAW))
        });
        if *lens.entry(ch.part).or_insert(ch.part_len) != ch.part_len
            || buf.len() as u64 != ch.offset
            || ch.offset + body.len() as u64 > ch.part_len
        {
            return false;
        }
        buf.extend_from_slice(&body);
        true
    }

    fn signature_after(&mut self) -> Result<Option<SignatureRecord>> {
        let raw = match self.read_raw() {
            Ok(r) => r,
            Err(Error::Truncated) => return Ok(None),
            Err(e) => return Err(e),
        };
        if raw.h.kind == RecordKind::Signature {
            return Ok(SignatureRecord::decode(&raw.stored).ok().filter(|_| raw.ok));
        }
        self.pending = Some(raw);
        Ok(None)
    }
}

fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<()> {
    match r.read_exact(buf) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Err(Error::Truncated),
        Err(e) => Err(Error::Io(e)),
    }
}

/// The media header, trailer and object index of a stream on a medium
/// that seeks, read without scanning the objects.
pub fn read_index<R: Read + Seek>(r: &mut R) -> Result<(MediaHeader, Trailer, Vec<IndexEntry>)> {
    r.seek(SeekFrom::Start(0))?;
    let mut hb = [0u8; header::LEN];
    read_full(r, &mut hb).map_err(|_| Error::NotSave)?;
    let header = MediaHeader::decode(&hb)?;
    let tail = (record::LEN + object::TRAILER_LEN) as u64;
    let end = r.seek(SeekFrom::End(0))?;
    if end < header::LEN as u64 + tail {
        return Err(Error::Truncated);
    }
    let at = end - tail;
    r.seek(SeekFrom::Start(at))?;
    // Anything but a trailer at the end means the stream did not finish.
    let (h, b) = record_at(r, at).map_err(|_| Error::Truncated)?;
    let trailer = Trailer::decode(&b).ok().filter(|_| h.kind == RecordKind::Trailer);
    let trailer = trailer
        .filter(|t| t.stream_id == header.stream_id && t.bytes == at)
        .ok_or(Error::Truncated)?;
    let mut index = Vec::with_capacity(trailer.objects as usize);
    if trailer.index_offset != 0 {
        r.seek(SeekFrom::Start(trailer.index_offset))?;
        let mut off = trailer.index_offset;
        for _ in 0..trailer.index_records {
            let (h, b) = record_at(r, off)?;
            if h.kind != RecordKind::Index || b.len() % object::INDEX_LEN != 0 {
                return Err(Error::Damaged { offset: off });
            }
            off += (record::LEN + b.len()) as u64;
            index.extend(
                b.as_chunks::<{ object::INDEX_LEN }>()
                    .0
                    .iter()
                    .map(|c| IndexEntry::decode(c)),
            );
        }
    }
    if index.len() != trailer.objects as usize {
        return Err(Error::Damaged {
            offset: trailer.index_offset,
        });
    }
    Ok((header, trailer, index))
}

fn record_at<R: Read>(r: &mut R, offset: u64) -> Result<(RecordHeader, Vec<u8>)> {
    let mut hb = [0u8; record::LEN];
    read_full(r, &mut hb)?;
    let h = RecordHeader::decode(&hb).ok_or(Error::Damaged { offset })?;
    let mut b = vec![0u8; h.stored_len as usize];
    read_full(r, &mut b)?;
    if crc32c(&b) != h.crc {
        return Err(Error::BadCrc { seq: h.seq });
    }
    Ok((h, b))
}

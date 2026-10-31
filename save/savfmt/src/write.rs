// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing a stream: header, objects part by part, index, trailer.

use std::io::Write;

use sha2::{Digest, Sha256};

use crate::codec::{Compression, STORED};
use crate::error::{Error, Result};
use crate::header::{self, MediaHeader};
use crate::object::{
    self, AuthorityEntry, ChunkHeader, IndexEntry, NO_OWNER, ObjectDescriptor, ObjectEnd, Part,
    SignatureRecord, Trailer, flags,
};
use crate::record::{self, MAX_RAW, RecordHeader, RecordKind, crc32c};

/// What a finished stream holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    /// Objects written.
    pub objects: u32,
    /// Objects the save could not write.
    pub not_saved: u32,
    /// Stream bytes, trailer included.
    pub bytes: u64,
}

struct Open {
    desc: ObjectDescriptor,
    hasher: Sha256,
    last_part: Option<Part>,
    parts: u16,
    bytes: u64,
    // The part being written: kind, its length, bytes so far, buffer.
    part: Option<(Part, u64, u64, Vec<u8>)>,
}

/// A stream writer over any `Write`.
pub struct Writer<W: Write> {
    w: W,
    compression: Compression,
    stream_id: [u8; 16],
    offset: u64,
    seq: u32,
    index: Vec<IndexEntry>,
    open: Option<Open>,
    signable: Option<(u32, [u8; 32])>,
}

impl<W: Write> Writer<W> {
    /// Start a stream: writes the media header.
    pub fn new(mut w: W, header: &MediaHeader) -> Result<Self> {
        w.write_all(&header.encode())?;
        Ok(Self {
            w,
            compression: header.compression,
            stream_id: header.stream_id,
            offset: header::LEN as u64,
            seq: 0,
            index: Vec::new(),
            open: None,
            signable: None,
        })
    }

    /// Bytes written so far.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    fn record(&mut self, kind: RecordKind, codec: u8, raw_len: usize, stored: &[u8]) -> Result<()> {
        let h = RecordHeader {
            kind,
            codec,
            seq: self.seq,
            stored_len: u32::try_from(stored.len()).map_err(|_| Error::Misuse("record size"))?,
            raw_len: u32::try_from(raw_len).map_err(|_| Error::Misuse("record size"))?,
            crc: crc32c(stored),
        };
        self.w.write_all(&h.encode())?;
        self.w.write_all(stored)?;
        self.offset += (record::LEN + stored.len()) as u64;
        self.seq += 1;
        Ok(())
    }

    /// The command and its parameters, as typed with defaults filled in.
    pub fn parameters(&mut self, command: &str) -> Result<()> {
        if self.open.is_some() {
            return Err(Error::Misuse("parameters inside an object"));
        }
        let b = command.as_bytes();
        if b.len() > MAX_RAW {
            return Err(Error::Misuse("parameters too long"));
        }
        self.record(RecordKind::Parameters, STORED, b.len(), b)
    }

    /// Begin an object. `desc.size_bytes` and `desc.part_count` must
    /// describe the parts that follow; `tail` is the IFS path, if any.
    /// Returns the object's index.
    pub fn begin_object(&mut self, mut desc: ObjectDescriptor, tail: &[u8]) -> Result<u32> {
        if self.open.is_some() {
            return Err(Error::Misuse("object not ended"));
        }
        let index = u32::try_from(self.index.len()).map_err(|_| Error::Misuse("objects"))?;
        if desc.owner_index != NO_OWNER && desc.owner_index >= index {
            return Err(Error::Misuse("owner must come first"));
        }
        desc.index = index;
        desc.tail_len = u16::try_from(tail.len()).map_err(|_| Error::Misuse("tail"))?;
        self.signable = None;
        self.index.push(IndexEntry::of(&desc, self.offset));
        let mut payload = desc.encode().to_vec();
        payload.extend_from_slice(tail);
        self.record(RecordKind::Object, STORED, payload.len(), &payload)?;
        let mut hasher = Sha256::new();
        hasher.update(b"QS400OBJ");
        hasher.update(desc.type_code.to_be_bytes());
        hasher.update(desc.attribute);
        hasher.update([desc.domain, desc.state, u8::from(desc.has(flags::ADOPTS))]);
        self.open = Some(Open {
            desc,
            hasher,
            last_part: None,
            parts: 0,
            bytes: 0,
            part: None,
        });
        Ok(index)
    }

    /// The open object's private authorities (before its parts).
    pub fn authorities(&mut self, auths: &[AuthorityEntry]) -> Result<()> {
        let open = self.open.as_ref().ok_or(Error::Misuse("no object"))?;
        if open.last_part.is_some() || !open.desc.has(flags::PRIVATE_AUTH) {
            return Err(Error::Misuse("authorities out of place"));
        }
        let b = object::encode_auths(open.desc.index, auths);
        if b.len() > MAX_RAW {
            return Err(Error::Misuse("too many authorities"));
        }
        self.record(RecordKind::Authorities, STORED, b.len(), &b)
    }

    /// Begin a part of `len` bytes. Parts go in ascending kind order.
    pub fn begin_part(&mut self, part: Part, len: u64) -> Result<()> {
        let open = self.open.as_mut().ok_or(Error::Misuse("no object"))?;
        if open.part.is_some() || open.last_part.is_some_and(|p| p >= part) {
            return Err(Error::Misuse("parts out of order"));
        }
        if len == 0 {
            return Err(Error::Misuse("empty parts are not written"));
        }
        open.hasher.update([part.0]);
        open.hasher.update(len.to_be_bytes());
        open.last_part = Some(part);
        open.parts += 1;
        open.part = Some((part, len, 0, Vec::new()));
        Ok(())
    }

    /// More bytes of the open part.
    pub fn write_part(&mut self, mut bytes: &[u8]) -> Result<()> {
        while !bytes.is_empty() {
            let open = self.open.as_mut().ok_or(Error::Misuse("no object"))?;
            let (_, len, done, buf) = open.part.as_mut().ok_or(Error::Misuse("no part"))?;
            let left = *len - *done - buf.len() as u64;
            let room = MAX_RAW - buf.len();
            let take = usize::try_from(left).unwrap_or(usize::MAX).min(room).min(bytes.len());
            if take == 0 {
                return Err(Error::Misuse("part longer than declared"));
            }
            buf.extend_from_slice(&bytes[..take]);
            open.hasher.update(&bytes[..take]);
            bytes = &bytes[take..];
            if buf.len() == MAX_RAW || *done + buf.len() as u64 == *len {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    /// A whole part at once.
    pub fn part(&mut self, part: Part, bytes: &[u8]) -> Result<()> {
        self.begin_part(part, bytes.len() as u64)?;
        self.write_part(bytes)
    }

    fn flush_chunk(&mut self) -> Result<()> {
        let open = self.open.as_mut().expect("open");
        let (part, len, done, buf) = open.part.as_mut().expect("part");
        let raw = std::mem::take(buf);
        let ch = ChunkHeader {
            index: open.desc.index,
            part: *part,
            offset: *done,
            part_len: *len,
        };
        *done += raw.len() as u64;
        open.bytes += raw.len() as u64;
        if *done == *len {
            open.part = None;
        }
        let (codec, body) = self.compression.pack(&raw);
        let mut stored = ch.encode().to_vec();
        stored.extend_from_slice(&body);
        self.record(RecordKind::Data, codec, object::CHUNK_LEN + raw.len(), &stored)
    }

    /// End the open object: checks its parts and writes the end record.
    /// Returns the digest.
    pub fn end_object(&mut self) -> Result<[u8; 32]> {
        let open = self.open.take().ok_or(Error::Misuse("no object"))?;
        if open.part.is_some() {
            return Err(Error::Misuse("part not complete"));
        }
        if open.parts != open.desc.part_count || open.bytes != open.desc.size_bytes {
            return Err(Error::Misuse("parts differ from the descriptor"));
        }
        let digest: [u8; 32] = open.hasher.finalize().into();
        let end = ObjectEnd {
            index: open.desc.index,
            parts: open.parts,
            bytes: open.bytes,
            digest,
        };
        self.record(RecordKind::ObjectEnd, STORED, object::END_LEN, &end.encode())?;
        if open.desc.has(flags::SIGNED) {
            self.signable = Some((open.desc.index, digest));
        }
        Ok(digest)
    }

    /// The signature of the object just ended (its descriptor said
    /// `SIGNED`).
    pub fn signature(&mut self, key_id: [u8; 8], signature: [u8; 64]) -> Result<()> {
        let (index, digest) = self.signable.take().ok_or(Error::Misuse("nothing to sign"))?;
        let s = SignatureRecord {
            index,
            algorithm: 1,
            key_id,
            signature,
            digest,
        };
        self.record(RecordKind::Signature, STORED, object::SIGNATURE_LEN, &s.encode())
    }

    /// Finish: index and trailer. Returns the summary and the medium.
    pub fn finish(mut self, not_saved: u32) -> Result<(Summary, W)> {
        if self.open.is_some() {
            return Err(Error::Misuse("object not ended"));
        }
        let index_offset = if self.index.is_empty() { 0 } else { self.offset };
        let per = MAX_RAW / object::INDEX_LEN;
        let entries = std::mem::take(&mut self.index);
        let mut index_records = 0u32;
        for chunk in entries.chunks(per) {
            let mut b = Vec::with_capacity(chunk.len() * object::INDEX_LEN);
            for e in chunk {
                b.extend_from_slice(&e.encode());
            }
            self.record(RecordKind::Index, STORED, b.len(), &b)?;
            index_records += 1;
        }
        let objects = u32::try_from(entries.len()).map_err(|_| Error::Misuse("objects"))?;
        let t = Trailer {
            objects,
            not_saved,
            index_offset,
            records: self.seq,
            index_records,
            bytes: self.offset,
            stream_id: self.stream_id,
        };
        self.record(RecordKind::Trailer, STORED, object::TRAILER_LEN, &t.encode())?;
        self.w.flush()?;
        Ok((
            Summary {
                objects,
                not_saved,
                bytes: self.offset,
            },
            self.w,
        ))
    }
}

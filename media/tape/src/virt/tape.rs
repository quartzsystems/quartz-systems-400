// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`VirtualTape`]: one virtual volume in one file.
//!
//! Opening scans the frames from the start of the data area; the chain
//! ends at the first frame that is incomplete, fails its checksum, or
//! carries an older write epoch than the frame before it. That frame's
//! offset is the end of data, so a torn last block is simply not there.
//! Every write session (and every overwrite in the middle of the data)
//! takes a new epoch, so stale frames left past the end never rejoin.

use std::io::{Read, Seek, SeekFrom, Write};

use super::format::{
    self, FRAME_LEN, HEADER_LEN, Header, KIND_BLOCK, KIND_MARK, decode_frame, encode_frame,
};
use crate::transport::{
    MAX_BLOCK, Position, ReadOutcome, Status, TapeTransport, TransportError, Written,
};

/// The density name of a new virtual volume.
pub const DEFAULT_DENSITY: &str = "*VRT256K";

/// How to create a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeOptions {
    /// Bytes in the data area, frames included.
    pub capacity: u64,
    /// Density name.
    pub density: String,
    /// Start write protected.
    pub write_protect: bool,
    /// Volume tag (the catalog entry's volume name).
    pub tag: String,
}

impl VolumeOptions {
    /// A volume of IMGSIZ megabytes (MiB), with the default density.
    #[must_use]
    pub fn imgsiz(megabytes: u64, tag: &str) -> Self {
        Self {
            capacity: megabytes * 1024 * 1024,
            density: DEFAULT_DENSITY.to_string(),
            write_protect: false,
            tag: tag.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Rec {
    offset: u64,
    kind: u8,
    len: u32,
}

impl Rec {
    fn end(self) -> u64 {
        self.offset + FRAME_LEN as u64 + u64::from(self.len)
    }
}

/// A virtual tape volume over any seekable byte store.
#[derive(Debug)]
pub struct VirtualTape<B> {
    io: B,
    header: Header,
    recs: Vec<Rec>,
    pos: usize,
    loaded: bool,
    max_epoch: u32,
    session: Option<u32>,
    block_size: u32,
}

impl<B: Read + Write + Seek> VirtualTape<B> {
    /// Format `io` as an empty volume and load it.
    pub fn create(mut io: B, opts: &VolumeOptions) -> Result<Self, TransportError> {
        if opts.capacity < (FRAME_LEN as u64) * 8 {
            return Err(TransportError::Format("capacity too small".into()));
        }
        let header = Header {
            version: format::VERSION,
            write_protect: opts.write_protect,
            capacity: opts.capacity,
            early_warning: format::early_warning_for(opts.capacity),
            density: opts.density.clone(),
            tag: opts.tag.clone(),
        };
        io.seek(SeekFrom::Start(0))?;
        io.write_all(&header.encode()?)?;
        // An invalid frame header ends any stale chain.
        io.write_all(&[0u8; FRAME_LEN])?;
        io.flush()?;
        Ok(Self::with(io, header, Vec::new(), 0))
    }

    /// Open an existing volume and load it.
    pub fn open(mut io: B) -> Result<Self, TransportError> {
        io.seek(SeekFrom::Start(0))?;
        let mut hb = [0u8; HEADER_LEN];
        read_full(&mut io, &mut hb)?
            .then_some(())
            .ok_or_else(|| TransportError::Format("no volume header".into()))?;
        let header = Header::decode(&hb)?;
        let (recs, max_epoch) = scan(&mut io, header.capacity)?;
        Ok(Self::with(io, header, recs, max_epoch))
    }

    fn with(io: B, header: Header, recs: Vec<Rec>, max_epoch: u32) -> Self {
        Self {
            io,
            header,
            recs,
            pos: 0,
            loaded: true,
            max_epoch,
            session: None,
            block_size: 0,
        }
    }

    /// The header as it is now.
    #[must_use]
    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The volume tag.
    #[must_use]
    pub fn tag(&self) -> &str {
        &self.header.tag
    }

    /// Bytes of the data area in use.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.end()
    }

    /// Whether the volume is loaded (not unloaded).
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Load it again after [`TapeTransport::unload`], at the beginning.
    pub fn load(&mut self) {
        self.loaded = true;
        self.pos = 0;
    }

    /// Set or clear the write-protect flag.
    pub fn set_write_protect(&mut self, on: bool) -> Result<(), TransportError> {
        self.header.write_protect = on;
        self.write_header()
    }

    /// The byte store.
    pub fn into_inner(self) -> B {
        self.io
    }

    fn write_header(&mut self) -> Result<(), TransportError> {
        self.io.seek(SeekFrom::Start(0))?;
        self.io.write_all(&self.header.encode()?)?;
        self.io.flush()?;
        Ok(())
    }

    fn end(&self) -> u64 {
        self.recs.last().map_or(0, |r| r.end())
    }

    fn offset_at(&self, pos: usize) -> u64 {
        self.recs.get(pos).map_or_else(|| self.end(), |r| r.offset)
    }

    fn ready(&self) -> Result<(), TransportError> {
        if self.loaded {
            Ok(())
        } else {
            Err(TransportError::NotLoaded)
        }
    }

    fn writable(&self) -> Result<(), TransportError> {
        self.ready()?;
        if self.header.write_protect {
            return Err(TransportError::WriteProtected);
        }
        Ok(())
    }

    fn epoch(&mut self) -> u32 {
        match self.session {
            Some(e) if self.pos == self.recs.len() => e,
            _ => {
                self.max_epoch += 1;
                self.session = Some(self.max_epoch);
                self.max_epoch
            }
        }
    }

    fn put(&mut self, kind: u8, payload: &[u8]) -> Result<Written, TransportError> {
        self.writable()?;
        let offset = self.offset_at(self.pos);
        let end = offset + (FRAME_LEN + payload.len()) as u64;
        if end > self.header.capacity {
            return Err(TransportError::EndOfMedium);
        }
        let epoch = self.epoch();
        let head = encode_frame(kind, epoch, payload);
        self.io.seek(SeekFrom::Start(HEADER_LEN as u64 + offset))?;
        self.io.write_all(&head)?;
        self.io.write_all(payload)?;
        self.io.flush()?;
        self.recs.truncate(self.pos);
        self.recs.push(Rec {
            offset,
            kind,
            len: u32::try_from(payload.len()).expect("block length"),
        });
        self.pos += 1;
        Ok(Written {
            early_warning: end > self.header.early_warning,
        })
    }

    fn is_mark(&self, i: usize) -> bool {
        self.recs[i].kind == KIND_MARK
    }
}

/// Fill `buf`; `false` when the store ends first.
fn read_full<R: Read>(io: &mut R, buf: &mut [u8]) -> std::io::Result<bool> {
    let mut got = 0;
    while got < buf.len() {
        match io.read(&mut buf[got..]) {
            Ok(0) => return Ok(false),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

/// Walk the frame chain from the start of the data area.
fn scan<B: Read + Seek>(io: &mut B, capacity: u64) -> Result<(Vec<Rec>, u32), TransportError> {
    io.seek(SeekFrom::Start(HEADER_LEN as u64))?;
    let mut recs = Vec::new();
    let mut offset = 0u64;
    let mut epoch = 0u32;
    let mut payload = vec![0u8; MAX_BLOCK];
    loop {
        let mut raw = [0u8; FRAME_LEN];
        if !read_full(io, &mut raw)? {
            break;
        }
        let Some(head) = decode_frame(&raw) else {
            break;
        };
        let rec = Rec {
            offset,
            kind: head.kind,
            len: head.len,
        };
        if head.epoch < epoch || rec.end() > capacity {
            break;
        }
        let body = &mut payload[..head.len as usize];
        if !read_full(io, body)? || !format::frame_ok(&raw, &head, body) {
            break;
        }
        epoch = head.epoch;
        offset = rec.end();
        recs.push(rec);
    }
    Ok((recs, epoch))
}

impl<B: Read + Write + Seek> TapeTransport for VirtualTape<B> {
    fn rewind(&mut self) -> Result<(), TransportError> {
        self.ready()?;
        self.pos = 0;
        Ok(())
    }

    fn fsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.ready()?;
        for _ in 0..n {
            if let Some(i) = (self.pos..self.recs.len()).find(|&i| self.is_mark(i)) {
                self.pos = i + 1;
            } else {
                self.pos = self.recs.len();
                return Err(TransportError::EndOfData);
            }
        }
        Ok(())
    }

    fn bsf(&mut self, n: u32) -> Result<(), TransportError> {
        self.ready()?;
        for _ in 0..n {
            if let Some(i) = (0..self.pos).rev().find(|&i| self.is_mark(i)) {
                self.pos = i;
            } else {
                self.pos = 0;
                return Err(TransportError::BeginningOfTape);
            }
        }
        Ok(())
    }

    fn fsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.ready()?;
        for _ in 0..n {
            if self.pos == self.recs.len() {
                return Err(TransportError::EndOfData);
            }
            self.pos += 1;
            if self.is_mark(self.pos - 1) {
                return Err(TransportError::TapeMark);
            }
        }
        Ok(())
    }

    fn bsr(&mut self, n: u32) -> Result<(), TransportError> {
        self.ready()?;
        for _ in 0..n {
            if self.pos == 0 {
                return Err(TransportError::BeginningOfTape);
            }
            if self.is_mark(self.pos - 1) {
                return Err(TransportError::TapeMark);
            }
            self.pos -= 1;
        }
        Ok(())
    }

    fn eom(&mut self) -> Result<(), TransportError> {
        self.ready()?;
        self.pos = self.recs.len();
        Ok(())
    }

    fn write_eof(&mut self, n: u32) -> Result<(), TransportError> {
        for _ in 0..n {
            self.put(KIND_MARK, &[])?;
        }
        Ok(())
    }

    fn erase(&mut self) -> Result<(), TransportError> {
        self.writable()?;
        let from = self.offset_at(self.pos);
        let to = self
            .end()
            .max(from + FRAME_LEN as u64)
            .min(self.header.capacity);
        self.io.seek(SeekFrom::Start(HEADER_LEN as u64 + from))?;
        let zeros = vec![0u8; 64 * 1024];
        let mut left = to.saturating_sub(from);
        while left > 0 {
            let n = usize::try_from(left.min(zeros.len() as u64)).expect("chunk");
            self.io.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        self.io.flush()?;
        self.recs.truncate(self.pos);
        self.session = None;
        Ok(())
    }

    fn unload(&mut self) -> Result<(), TransportError> {
        self.ready()?;
        self.pos = 0;
        self.loaded = false;
        Ok(())
    }

    fn set_block_size(&mut self, size: u32) -> Result<(), TransportError> {
        if size as usize > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: size as usize,
                max: MAX_BLOCK,
            });
        }
        self.block_size = size;
        Ok(())
    }

    fn set_density(&mut self, density: &str) -> Result<(), TransportError> {
        self.writable()?;
        let old = std::mem::replace(&mut self.header.density, density.to_string());
        if let Err(e) = self.header.encode() {
            self.header.density = old;
            return Err(TransportError::Density(format!("{density} ({e})")));
        }
        self.write_header()
    }

    fn status(&mut self) -> Result<Status, TransportError> {
        if !self.loaded {
            return Ok(Status {
                density: self.header.density.clone(),
                block_size: self.block_size,
                ..Status::default()
            });
        }
        let last_mark = (0..self.pos).rev().find(|&i| self.is_mark(i));
        let file = self.recs[..self.pos]
            .iter()
            .filter(|r| r.kind == KIND_MARK)
            .count();
        let block = self.pos - last_mark.map_or(0, |i| i + 1);
        Ok(Status {
            loaded: true,
            bot: self.pos == 0,
            eot: self.offset_at(self.pos) > self.header.early_warning,
            eod: self.pos == self.recs.len(),
            write_protected: self.header.write_protect,
            density: self.header.density.clone(),
            block_size: self.block_size,
            position: Position {
                file: u32::try_from(file).unwrap_or(u32::MAX),
                block: u32::try_from(block).unwrap_or(u32::MAX),
            },
        })
    }

    fn read_block(&mut self, buf: &mut [u8]) -> Result<ReadOutcome, TransportError> {
        self.ready()?;
        let Some(&rec) = self.recs.get(self.pos) else {
            return Ok(ReadOutcome::EndOfData);
        };
        self.pos += 1;
        if rec.kind == KIND_MARK {
            return Ok(ReadOutcome::TapeMark);
        }
        let len = rec.len as usize;
        if len > buf.len() {
            return Err(TransportError::BlockTooLarge {
                len,
                max: buf.len(),
            });
        }
        self.io.seek(SeekFrom::Start(
            HEADER_LEN as u64 + rec.offset + FRAME_LEN as u64,
        ))?;
        self.io.read_exact(&mut buf[..len])?;
        Ok(ReadOutcome::Block(len))
    }

    fn write_block(&mut self, data: &[u8]) -> Result<Written, TransportError> {
        if data.is_empty() || data.len() > MAX_BLOCK {
            return Err(TransportError::BlockTooLarge {
                len: data.len(),
                max: MAX_BLOCK,
            });
        }
        if self.block_size != 0 && data.len() != self.block_size as usize {
            return Err(TransportError::BlockSize {
                len: data.len(),
                size: self.block_size,
            });
        }
        self.put(KIND_BLOCK, data)
    }
}

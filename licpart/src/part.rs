// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading and writing the two copies.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use crate::error::LicError;
use crate::format::header::Slot;
use crate::format::{CRC32C, Record, SLOT_BYTES};

/// Offset of the resume journal area: well past the two record copies.
pub const RESUME_OFFSET: u64 = 1024 * 1024;
/// Most bytes the resume journal area holds, header included.
pub const RESUME_MAX: usize = 4 * 1024 * 1024;
const RESUME_MAGIC: [u8; 8] = *b"QS400RSJ";
const RESUME_HEADER: usize = 16;

/// A block device or anything standing in for one.
pub trait Device: Read + Write + Seek {
    /// Make earlier writes durable. Files call `sync_all`.
    fn sync(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl Device for File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}

impl Device for Cursor<Vec<u8>> {}

impl<D: Device + ?Sized> Device for Box<D> {
    fn sync(&mut self) -> io::Result<()> {
        (**self).sync()
    }
}

impl<D: Device> Device for &mut D {
    fn sync(&mut self) -> io::Result<()> {
        (**self).sync()
    }
}

/// The partition, open on a device.
pub struct LicPart<D> {
    dev: D,
}

fn slot_offset(slot: Slot) -> u64 {
    match slot {
        Slot::A => 0,
        Slot::B => SLOT_BYTES as u64,
    }
}

impl<D: Device> LicPart<D> {
    /// Wrap a device. No I/O happens until `read` or `write`.
    pub fn open(dev: D) -> Self {
        Self { dev }
    }

    /// Give the device back.
    pub fn into_device(self) -> D {
        self.dev
    }

    /// First boot: both copies must be invalid. Writes defaults to copy A.
    pub fn init(dev: D, install_time: u64) -> Result<(Self, Record), LicError> {
        let mut part = Self::open(dev);
        if part.read_slots().iter().any(Option::is_some) {
            return Err(LicError::AlreadyInitialized);
        }
        let mut rec = Record::new(install_time);
        part.write(&mut rec)?;
        Ok((part, rec))
    }

    /// Decode both copies. `None` is an invalid or empty copy.
    pub fn read_slots(&mut self) -> [Option<Record>; 2] {
        [self.read_slot(Slot::A), self.read_slot(Slot::B)]
    }

    /// The newest valid copy.
    pub fn read(&mut self) -> Result<Record, LicError> {
        let [a, b] = self.read_slots();
        match (a, b) {
            (Some(a), Some(b)) => Ok(if b.sequence > a.sequence { b } else { a }),
            (Some(a), None) => Ok(a),
            (None, Some(b)) => Ok(b),
            (None, None) => Err(LicError::NoValidCopy),
        }
    }

    /// Write `rec` to the older (or invalid) copy with the next sequence
    /// number, then sync. Returns the copy written. The newest good copy is
    /// never overwritten, so a torn write cannot lose it.
    pub fn write(&mut self, rec: &mut Record) -> Result<Slot, LicError> {
        let [a, b] = self.read_slots();
        let (target, newest) = match (&a, &b) {
            (None, _) => (Slot::A, b.as_ref().map_or(0, |r| r.sequence)),
            (Some(_), None) => (Slot::B, a.as_ref().map_or(0, |r| r.sequence)),
            (Some(a), Some(b)) => {
                if a.sequence <= b.sequence {
                    (Slot::A, b.sequence)
                } else {
                    (Slot::B, a.sequence)
                }
            }
        };
        rec.sequence = newest + 1;
        let bytes = rec.encode()?;
        self.dev.seek(SeekFrom::Start(slot_offset(target)))?;
        self.dev.write_all(&bytes)?;
        self.dev.sync()?;
        Ok(target)
    }

    /// The resume journal, if a valid one is recorded: a mirrored half
    /// being copied keeps its progress here across a crash.
    pub fn read_resume(&mut self) -> Option<Vec<u8>> {
        self.dev.seek(SeekFrom::Start(RESUME_OFFSET)).ok()?;
        let mut header = [0u8; RESUME_HEADER];
        self.dev.read_exact(&mut header).ok()?;
        if header[..8] != RESUME_MAGIC {
            return None;
        }
        let len = usize::try_from(u32::from_le_bytes(header[8..12].try_into().ok()?)).ok()?;
        let crc = u32::from_le_bytes(header[12..16].try_into().ok()?);
        if len == 0 || len > RESUME_MAX - RESUME_HEADER {
            return None;
        }
        let mut data = vec![0u8; len];
        self.dev.read_exact(&mut data).ok()?;
        (CRC32C.checksum(&data) == crc).then_some(data)
    }

    /// Record the resume journal.
    pub fn write_resume(&mut self, data: &[u8]) -> Result<(), LicError> {
        if data.is_empty() || data.len() > RESUME_MAX - RESUME_HEADER {
            return Err(LicError::TooLarge {
                len: data.len(),
                max: RESUME_MAX - RESUME_HEADER,
            });
        }
        let mut out = Vec::with_capacity(RESUME_HEADER + data.len());
        out.extend_from_slice(&RESUME_MAGIC);
        out.extend_from_slice(&u32::try_from(data.len()).expect("checked").to_le_bytes());
        out.extend_from_slice(&CRC32C.checksum(data).to_le_bytes());
        out.extend_from_slice(data);
        self.dev.seek(SeekFrom::Start(RESUME_OFFSET))?;
        self.dev.write_all(&out)?;
        self.dev.sync()?;
        Ok(())
    }

    /// Forget the resume journal.
    pub fn clear_resume(&mut self) -> Result<(), LicError> {
        self.dev.seek(SeekFrom::Start(RESUME_OFFSET))?;
        self.dev.write_all(&[0u8; RESUME_HEADER])?;
        self.dev.sync()?;
        Ok(())
    }

    fn read_slot(&mut self, slot: Slot) -> Option<Record> {
        self.dev.seek(SeekFrom::Start(slot_offset(slot))).ok()?;
        let mut buf = vec![0u8; SLOT_BYTES];
        let mut got = 0;
        while got < SLOT_BYTES {
            match self.dev.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        Record::decode(&buf[..got]).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_area_round_trips_and_clears() {
        let dev = Cursor::new(vec![0u8; usize::try_from(RESUME_OFFSET).unwrap() + 4096]);
        let (mut part, _) = LicPart::init(dev, 7).unwrap();
        assert!(part.read_resume().is_none());
        part.write_resume(b"chunks done: 3").unwrap();
        assert_eq!(part.read_resume().unwrap(), b"chunks done: 3");
        // The records are untouched.
        assert_eq!(part.read().unwrap().header.install_time, 7);
        part.clear_resume().unwrap();
        assert!(part.read_resume().is_none());
        assert!(part.write_resume(&[]).is_err());
    }
}

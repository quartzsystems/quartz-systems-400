// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading and writing the two copies.

use std::io::SeekFrom;

use abrec::AbPart;
pub use abrec::Device;

use crate::error::LicError;
use crate::format::header::Slot;
use crate::format::{CRC32C, Record, SPEC};

/// Offset of the resume journal area: well past the two record copies.
pub const RESUME_OFFSET: u64 = 1024 * 1024;
/// Most bytes the resume journal area holds, header included.
pub const RESUME_MAX: usize = 4 * 1024 * 1024;
const RESUME_MAGIC: [u8; 8] = *b"QS400RSJ";
const RESUME_HEADER: usize = 16;

/// TLVs the hardware resource service writes from its own process
/// (`0x010A`, the resource names): every other writer keeps the newest
/// copy's.
pub const SERVICE_TAGS: &[u16] = &[0x010A];

/// The partition, open on a device.
pub struct LicPart<D> {
    ab: AbPart<D>,
}

impl<D: Device> LicPart<D> {
    /// Wrap a device. No I/O happens until `read` or `write`.
    pub fn open(dev: D) -> Self {
        Self {
            ab: AbPart::open(SPEC, dev),
        }
    }

    /// Give the device back.
    pub fn into_device(self) -> D {
        self.ab.into_device()
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
        let mut slots = self.read_slots();
        abrec::newest(sequences(&slots))
            .and_then(|s| slots[s.index()].take())
            .ok_or(LicError::NoValidCopy)
    }

    /// Replace `rec` with the newest copy on disk, so a change made from
    /// it keeps what another process wrote (the storage service's SST
    /// writes the record while `slic` holds a copy). Kept as it is when
    /// no copy reads.
    pub fn refresh(&mut self, rec: &mut Record) {
        if let Ok(fresh) = self.read() {
            *rec = fresh;
        }
    }

    /// Write `rec` to the older (or invalid) copy with the next sequence
    /// number, then sync. Returns the copy written. The newest good copy is
    /// never overwritten, so a torn write cannot lose it. The TLVs another
    /// process owns ([`SERVICE_TAGS`]) are taken from the newest copy, so
    /// a writer holding an older record never undoes them.
    pub fn write(&mut self, rec: &mut Record) -> Result<Slot, LicError> {
        self.write_keeping(rec, true)
    }

    /// [`LicPart::write`] by the owner of the [`SERVICE_TAGS`]: `rec`'s
    /// own values are written.
    pub fn write_service(&mut self, rec: &mut Record) -> Result<Slot, LicError> {
        self.write_keeping(rec, false)
    }

    fn write_keeping(&mut self, rec: &mut Record, keep: bool) -> Result<Slot, LicError> {
        let slots = self.read_slots();
        let (target, sequence) = abrec::target(sequences(&slots));
        // The copy that stays is the newest.
        if keep && let Some(n) = &slots[target.other().index()] {
            for &tag in SERVICE_TAGS {
                match n.tlv(tag) {
                    Some(data) => rec.set_tlv(tag, data.to_vec())?,
                    None => {
                        rec.remove_tlv(tag);
                    }
                }
            }
        }
        rec.sequence = sequence;
        self.ab.write_slot(target, sequence, &rec.payload()?)?;
        Ok(target)
    }

    /// The resume journal, if a valid one is recorded: a mirrored half
    /// being copied keeps its progress here across a crash.
    pub fn read_resume(&mut self) -> Option<Vec<u8>> {
        self.ab.device().seek(SeekFrom::Start(RESUME_OFFSET)).ok()?;
        let mut header = [0u8; RESUME_HEADER];
        self.ab.device().read_exact(&mut header).ok()?;
        if header[..8] != RESUME_MAGIC {
            return None;
        }
        let len = usize::try_from(u32::from_le_bytes(header[8..12].try_into().ok()?)).ok()?;
        let crc = u32::from_le_bytes(header[12..16].try_into().ok()?);
        if len == 0 || len > RESUME_MAX - RESUME_HEADER {
            return None;
        }
        let mut data = vec![0u8; len];
        self.ab.device().read_exact(&mut data).ok()?;
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
        self.ab.device().seek(SeekFrom::Start(RESUME_OFFSET))?;
        self.ab.device().write_all(&out)?;
        self.ab.device().sync()?;
        Ok(())
    }

    /// Forget the resume journal.
    pub fn clear_resume(&mut self) -> Result<(), LicError> {
        self.ab.device().seek(SeekFrom::Start(RESUME_OFFSET))?;
        self.ab.device().write_all(&[0u8; RESUME_HEADER])?;
        self.ab.device().sync()?;
        Ok(())
    }

    fn read_slot(&mut self, slot: Slot) -> Option<Record> {
        let copy = self.ab.read_slot(slot)?;
        Record::from_payload(copy.sequence, &copy.payload).ok()
    }
}

/// The copies' sequence numbers, for the choice between them.
fn sequences(slots: &[Option<Record>; 2]) -> [Option<u64>; 2] {
    [0, 1].map(|i| slots[i].as_ref().map(|r| r.sequence))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::format::SLOT_BYTES;

    #[test]
    fn a_stale_writer_keeps_the_service_tlvs() {
        let dev = Cursor::new(vec![0u8; 2 * SLOT_BYTES]);
        let (mut part, mut stale) = LicPart::init(dev, 7).unwrap();
        let tag = SERVICE_TAGS[0];
        let mut svc = part.read().unwrap();
        svc.set_tlv(tag, b"names".to_vec()).unwrap();
        part.write_service(&mut svc).unwrap();
        // A writer still holding the record from before keeps them.
        stale.header.ipl_count = 5;
        part.write(&mut stale).unwrap();
        let now = part.read().unwrap();
        assert_eq!(now.header.ipl_count, 5);
        assert_eq!(now.tlv(tag), Some(&b"names"[..]));
        // Only the owner changes them.
        let mut svc = part.read().unwrap();
        svc.remove_tlv(tag);
        part.write_service(&mut svc).unwrap();
        stale.set_tlv(tag, b"old".to_vec()).unwrap();
        part.write(&mut stale).unwrap();
        assert_eq!(part.read().unwrap().tlv(tag), None);
    }

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

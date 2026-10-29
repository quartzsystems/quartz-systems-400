// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! On-disk record format, little-endian throughout.
//!
//! ```text
//! 0   magic "QS400LIC"            8
//! 8   format_version   u32        4
//! 12  record_len       u32        4   bytes from offset 0 to end of TLVs
//! 16  crc32c           u32        4   over [20, record_len)
//! 20  sequence         u64        8
//! 28  header                      24  see header.rs
//! 52  tlv_count        u32        4
//! 56  TLVs: tag u16, len u32, data
//! ```
//!
//! IPL history records, activity log entries and key slots are stored as
//! TLVs with reserved tags.

pub mod header;
pub mod history;
pub mod keyslot;
pub mod pal;
pub mod tlv;

use crc::{CRC_32_ISCSI, Crc};

use crate::error::LicError;
use header::Header;
use history::IplHistory;
use keyslot::{KeyPurpose, KeySlot};
use pal::{PAL_MAX, PalEntry};
use tlv::{TAG_HISTORY, TAG_KEYSLOT, TAG_PAL, TAG_USER_MIN, Tlv};

/// Record magic.
pub const MAGIC: [u8; 8] = *b"QS400LIC";
/// Current format version.
pub const FORMAT_VERSION: u32 = 1;
/// Bytes reserved for each copy. Copy A is at 0, copy B at `SLOT_BYTES`.
pub const SLOT_BYTES: usize = 64 * 1024;
/// IPL history records kept.
pub const HISTORY_MAX: usize = 16;

const CRC_OFFSET: usize = 16;
const CRC_START: usize = 20;
const FIXED_LEN: usize = 56;

pub(crate) const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// One decoded LIC record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Monotonic; set by `LicPart::write`.
    pub sequence: u64,
    /// Fixed IPL state.
    pub header: Header,
    /// Oldest first, at most `HISTORY_MAX`.
    pub history: Vec<IplHistory>,
    /// The product activity log, oldest first, at most `PAL_MAX`.
    pub pal: Vec<PalEntry>,
    /// Key slots, at most one per purpose.
    pub keys: Vec<KeySlot>,
    /// Everything else, by tag. Tags below `TAG_USER_MIN` are reserved.
    pub tlvs: Vec<Tlv>,
}

impl Record {
    /// A first-boot record with defaults.
    pub fn new(install_time: u64) -> Self {
        Self {
            sequence: 0,
            header: Header::new(install_time),
            history: Vec::new(),
            pal: Vec::new(),
            keys: Vec::new(),
            tlvs: Vec::new(),
        }
    }

    /// The key slot for `purpose`, if one exists.
    pub fn key_slot(&self, purpose: KeyPurpose) -> Option<&KeySlot> {
        self.keys.iter().find(|k| k.purpose == purpose)
    }

    /// Replace or add the slot for `slot.purpose`.
    pub fn set_key_slot(&mut self, slot: KeySlot) {
        match self.keys.iter_mut().find(|k| k.purpose == slot.purpose) {
            Some(k) => *k = slot,
            None => self.keys.push(slot),
        }
    }

    /// Remove the slot for `purpose`. Returns whether one existed.
    pub fn remove_key_slot(&mut self, purpose: KeyPurpose) -> bool {
        let before = self.keys.len();
        self.keys.retain(|k| k.purpose != purpose);
        before != self.keys.len()
    }

    /// Append an activity log entry, dropping the oldest beyond `PAL_MAX`.
    pub fn push_pal(&mut self, entry: PalEntry) {
        self.pal.push(entry);
        if self.pal.len() > PAL_MAX {
            let excess = self.pal.len() - PAL_MAX;
            self.pal.drain(..excess);
        }
    }

    /// Append a history record, dropping the oldest beyond `HISTORY_MAX`.
    pub fn push_history(&mut self, entry: IplHistory) {
        self.history.push(entry);
        if self.history.len() > HISTORY_MAX {
            let excess = self.history.len() - HISTORY_MAX;
            self.history.drain(..excess);
        }
    }

    /// Data of the TLV with `tag`, if present.
    pub fn tlv(&self, tag: u16) -> Option<&[u8]> {
        self.tlvs
            .iter()
            .find(|t| t.tag == tag)
            .map(|t| t.data.as_slice())
    }

    /// Replace or append the TLV with `tag`.
    pub fn set_tlv(&mut self, tag: u16, data: Vec<u8>) -> Result<(), LicError> {
        if tag < TAG_USER_MIN {
            return Err(LicError::ReservedTag(tag));
        }
        match self.tlvs.iter_mut().find(|t| t.tag == tag) {
            Some(t) => t.data = data,
            None => self.tlvs.push(Tlv { tag, data }),
        }
        Ok(())
    }

    /// Remove the TLV with `tag`. Returns whether one existed.
    pub fn remove_tlv(&mut self, tag: u16) -> bool {
        let before = self.tlvs.len();
        self.tlvs.retain(|t| t.tag != tag);
        before != self.tlvs.len()
    }

    /// Serialize with CRC. Fails if the result does not fit a slot.
    pub fn encode(&self) -> Result<Vec<u8>, LicError> {
        let mut out = Vec::with_capacity(FIXED_LEN + 256);
        out.extend_from_slice(&MAGIC);
        put_u32(&mut out, FORMAT_VERSION);
        put_u32(&mut out, 0); // record_len, patched below
        put_u32(&mut out, 0); // crc, patched below
        put_u64(&mut out, self.sequence);
        self.header.encode(&mut out);

        let count = self.history.len() + self.pal.len() + self.keys.len() + self.tlvs.len();
        put_u32(
            &mut out,
            u32::try_from(count).map_err(|_| LicError::TooLarge {
                len: usize::MAX,
                max: SLOT_BYTES,
            })?,
        );
        for h in &self.history {
            Tlv {
                tag: TAG_HISTORY,
                data: h.encode(),
            }
            .encode(&mut out);
        }
        for e in &self.pal {
            Tlv {
                tag: TAG_PAL,
                data: e.encode(),
            }
            .encode(&mut out);
        }
        for k in &self.keys {
            Tlv {
                tag: TAG_KEYSLOT,
                data: k.encode(),
            }
            .encode(&mut out);
        }
        for t in &self.tlvs {
            t.encode(&mut out);
        }

        let len = out.len();
        if len > SLOT_BYTES {
            return Err(LicError::TooLarge {
                len,
                max: SLOT_BYTES,
            });
        }
        let len32 = u32::try_from(len).map_err(|_| LicError::TooLarge {
            len,
            max: SLOT_BYTES,
        })?;
        out[12..16].copy_from_slice(&len32.to_le_bytes());
        let crc = CRC32C.checksum(&out[CRC_START..]);
        out[CRC_OFFSET..CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
        Ok(out)
    }

    /// Parse and verify one copy. Any defect is `Malformed`.
    pub fn decode(bytes: &[u8]) -> Result<Self, LicError> {
        if bytes.len() < FIXED_LEN {
            return Err(LicError::Malformed("shorter than the fixed part"));
        }
        if bytes[..8] != MAGIC {
            return Err(LicError::Malformed("bad magic"));
        }
        let mut r = Reader::new(&bytes[8..]);
        let version = r.u32()?;
        if version != FORMAT_VERSION {
            return Err(LicError::Malformed("unsupported format version"));
        }
        let len = usize::try_from(r.u32()?).map_err(|_| LicError::Malformed("record_len"))?;
        if len < FIXED_LEN || len > bytes.len() || len > SLOT_BYTES {
            return Err(LicError::Malformed("record_len out of range"));
        }
        let crc = r.u32()?;
        if CRC32C.checksum(&bytes[CRC_START..len]) != crc {
            return Err(LicError::Malformed("crc mismatch"));
        }
        let sequence = r.u64()?;
        let header = Header::decode(&mut r)?;
        let count = r.u32()?;

        let mut rec = Self {
            sequence,
            header,
            history: Vec::new(),
            pal: Vec::new(),
            keys: Vec::new(),
            tlvs: Vec::new(),
        };
        let mut r = Reader::new(&bytes[FIXED_LEN..len]);
        for _ in 0..count {
            let t = Tlv::decode(&mut r)?;
            if t.tag == TAG_HISTORY {
                rec.history.push(IplHistory::decode(&t.data)?);
            } else if t.tag == TAG_PAL {
                rec.pal.push(PalEntry::decode(&t.data)?);
            } else if t.tag == TAG_KEYSLOT {
                let k = KeySlot::decode(&t.data)?;
                if rec.key_slot(k.purpose).is_some() {
                    return Err(LicError::Malformed("duplicate key slot"));
                }
                rec.keys.push(k);
            } else {
                rec.tlvs.push(t);
            }
        }
        if !r.at_end() {
            return Err(LicError::Malformed("trailing bytes inside record_len"));
        }
        if rec.history.len() > HISTORY_MAX {
            return Err(LicError::Malformed("too many history records"));
        }
        if rec.pal.len() > PAL_MAX {
            return Err(LicError::Malformed("too many activity log entries"));
        }
        Ok(rec)
    }
}

pub(crate) fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub(crate) fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Bounds-checked little-endian reader.
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub(crate) fn bytes(&mut self, n: usize) -> Result<&'a [u8], LicError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(LicError::Malformed("length overflow"))?;
        let s = self
            .buf
            .get(self.pos..end)
            .ok_or(LicError::Malformed("truncated"))?;
        self.pos = end;
        Ok(s)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, LicError> {
        Ok(self.bytes(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, LicError> {
        Ok(u16::from_le_bytes(
            self.bytes(2)?.try_into().expect("2 bytes"),
        ))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, LicError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("4 bytes"),
        ))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, LicError> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?.try_into().expect("8 bytes"),
        ))
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos == self.buf.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::header::{IplMode, IplType, Slot};
    use crate::format::history::Outcome;

    fn sample() -> Record {
        let mut r = Record::new(1_700_000_000);
        r.sequence = 7;
        r.header.ipl_count = 3;
        r.header.next_mode = Some(IplMode::Manual);
        r.push_history(IplHistory {
            start_time: 1,
            end_time: 2,
            ipl_type: IplType::B,
            mode: IplMode::Normal,
            slot: Slot::A,
            final_src: *b"C6001090",
            outcome: Outcome::Normal,
        });
        r.set_tlv(0x0100, b"hello".to_vec()).unwrap();
        r.push_pal(PalEntry {
            time: 5,
            code: *b"A6001255",
            severity: pal::Severity::Attention,
        });
        r
    }

    #[test]
    fn roundtrip() {
        let r = sample();
        let bytes = r.encode().unwrap();
        assert_eq!(Record::decode(&bytes).unwrap(), r);
    }

    #[test]
    fn crc_catches_a_flipped_bit() {
        let mut bytes = sample().encode().unwrap();
        bytes[30] ^= 1;
        assert!(matches!(
            Record::decode(&bytes),
            Err(LicError::Malformed("crc mismatch"))
        ));
    }

    #[test]
    fn truncation_is_malformed() {
        let bytes = sample().encode().unwrap();
        assert!(Record::decode(&bytes[..bytes.len() - 3]).is_err());
    }

    #[test]
    fn user_tlvs_round_trip() {
        let mut r = Record::new(0);
        r.set_tlv(0x0105, vec![2]).unwrap();
        let back = Record::decode(&r.encode().unwrap()).unwrap();
        assert_eq!(back.tlv(0x0105), Some(&[2u8][..]));
        assert_eq!(back.tlv(0x0106), None);
    }

    #[test]
    fn reserved_tags_are_refused() {
        let mut r = Record::new(0);
        assert!(matches!(
            r.set_tlv(2, vec![]),
            Err(LicError::ReservedTag(2))
        ));
    }

    #[test]
    fn key_slots_round_trip_and_replace() {
        let mut r = sample();
        assert!(r.key_slot(KeyPurpose::CertStash).is_none());
        let slot = KeySlot {
            purpose: KeyPurpose::CertStash,
            generation: 1,
            created: 99,
            key: [7u8; 32],
        };
        r.set_key_slot(slot.clone());
        let back = Record::decode(&r.encode().unwrap()).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.key_slot(KeyPurpose::CertStash), Some(&slot));
        // The key never shows in debug output.
        assert!(!format!("{slot:?}").contains("7, 7"));

        let mut next = slot.clone();
        next.generation = 2;
        next.key = [8u8; 32];
        r.set_key_slot(next);
        assert_eq!(r.keys.len(), 1);
        assert_eq!(r.key_slot(KeyPurpose::CertStash).unwrap().generation, 2);
        assert!(r.remove_key_slot(KeyPurpose::CertStash));
        assert!(!r.remove_key_slot(KeyPurpose::CertStash));
    }

    #[test]
    fn bad_key_slots_are_malformed() {
        let mut r = Record::new(0);
        r.set_key_slot(KeySlot {
            purpose: KeyPurpose::CertStash,
            generation: 1,
            created: 0,
            key: [1u8; 32],
        });
        let good = r.encode().unwrap();
        // Slot data starts after the fixed part and the 6-byte TLV head.
        for (at, what) in [(FIXED_LEN + 6, "version"), (FIXED_LEN + 7, "purpose")] {
            let mut bytes = good.clone();
            bytes[at] = 0xEE;
            let crc = CRC32C.checksum(&bytes[CRC_START..]);
            bytes[CRC_OFFSET..CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
            assert!(Record::decode(&bytes).is_err(), "{what}");
        }
        // Two slots for one purpose.
        r.keys.push(r.keys[0].clone());
        assert!(Record::decode(&r.encode().unwrap()).is_err());
    }

    #[test]
    fn reserved_key_slot_tag_is_refused_as_a_user_tlv() {
        let mut r = Record::new(0);
        assert!(r.set_tlv(tlv::TAG_KEYSLOT, vec![]).is_err());
    }

    #[test]
    fn history_is_capped() {
        let mut r = Record::new(0);
        for i in 0..20 {
            let mut h = sample().history[0].clone();
            h.start_time = i;
            r.push_history(h);
        }
        assert_eq!(r.history.len(), HISTORY_MAX);
        assert_eq!(r.history[0].start_time, 4);
    }
}

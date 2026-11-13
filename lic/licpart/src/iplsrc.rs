// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The side of the next IPL where no firmware keeps it (TLV
//! [`TAG_IPL_SOURCE`]): the slot to IPL from once, which is what UEFI's
//! `BootNext` is, and the slot IPLs come from otherwise, which is what
//! `BootOrder` is. A system in a partition writes them; the hypervisor
//! reads the record before each IPL, takes the once-only slot when
//! there is one, clears it and IPLs from it, and with none IPLs from
//! the default slot. A record without the TLV, or with no default,
//! means slot A.
//!
//! The data is four bytes: the layout's version (1), the once-only
//! slot, the default slot, and a zero. A slot is 0 for none, 1 for A,
//! 2 for B. Longer data of a later version reads by its first four
//! bytes; anything else reads as no choice made.

use crate::error::LicError;
use crate::format::Record;
use crate::format::header::Slot;

/// The record's TLV tag for [`IplSource`].
pub const TAG_IPL_SOURCE: u16 = 0x010B;
/// The layout's version, the data's first byte.
pub const IPL_SOURCE_VERSION: u8 = 1;
/// The data's length at version 1.
pub const IPL_SOURCE_BYTES: usize = 4;

/// The sides the next IPLs come from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IplSource {
    /// The slot of the next IPL only (byte 1).
    pub next: Option<Slot>,
    /// The slot of every IPL after it (byte 2); `None` is slot A.
    pub default: Option<Slot>,
}

fn byte(s: Option<Slot>) -> u8 {
    match s {
        None => 0,
        Some(Slot::A) => 1,
        Some(Slot::B) => 2,
    }
}

fn slot(b: u8) -> Option<Slot> {
    match b {
        1 => Some(Slot::A),
        2 => Some(Slot::B),
        _ => None,
    }
}

impl IplSource {
    /// The TLV's data.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        vec![IPL_SOURCE_VERSION, byte(self.next), byte(self.default), 0]
    }

    /// From the TLV's data; short or of version 0 is no choice made.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        match b {
            [v, next, default, _, ..] if *v >= IPL_SOURCE_VERSION => Self {
                next: slot(*next),
                default: slot(*default),
            },
            _ => Self::default(),
        }
    }

    /// The record's; a record without the TLV has made no choice.
    #[must_use]
    pub fn of(record: &Record) -> Self {
        record
            .tlv(TAG_IPL_SOURCE)
            .map(Self::decode)
            .unwrap_or_default()
    }

    /// Keep them in the record.
    ///
    /// # Errors
    /// The record has no room.
    pub fn store(self, record: &mut Record) -> Result<(), LicError> {
        record.set_tlv(TAG_IPL_SOURCE, self.encode())
    }

    /// The slot this IPL comes from: the once-only slot, else the
    /// default, else A.
    #[must_use]
    pub fn slot(self) -> Slot {
        self.next.or(self.default).unwrap_or(Slot::A)
    }

    /// What a hypervisor does before an IPL: the slot to IPL from, the
    /// once-only slot cleared in the record. The second value says
    /// whether the record changed and so must be written.
    ///
    /// # Errors
    /// The record has no room.
    pub fn take(record: &mut Record) -> Result<(Slot, bool), LicError> {
        let mut src = Self::of(record);
        let slot = src.slot();
        if src.next.take().is_none() {
            return Ok((slot, false));
        }
        src.store(record)?;
        Ok((slot, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_without_them_means_slot_a() {
        let r = Record::new(1);
        assert_eq!(r.tlv(TAG_IPL_SOURCE), None);
        assert_eq!(IplSource::of(&r), IplSource::default());
        assert_eq!(IplSource::of(&r).slot(), Slot::A);
    }

    #[test]
    fn they_round_trip_in_the_record() {
        for next in [None, Some(Slot::A), Some(Slot::B)] {
            for default in [None, Some(Slot::A), Some(Slot::B)] {
                let mut r = Record::new(1);
                let src = IplSource { next, default };
                src.store(&mut r).unwrap();
                assert_eq!(IplSource::of(&r), src);
            }
        }
    }

    #[test]
    fn the_bytes_are_version_next_default_zero() {
        let src = IplSource {
            next: Some(Slot::B),
            default: Some(Slot::A),
        };
        assert_eq!(src.encode(), [1, 2, 1, 0]);
        assert_eq!(src.encode().len(), IPL_SOURCE_BYTES);
        assert_eq!(IplSource::decode(&[1, 2, 1, 0]), src);
        // A later version's longer data reads by its first four bytes.
        assert_eq!(IplSource::decode(&[2, 2, 1, 0, 9, 9]), src);
        // Damaged or unknown data is no choice made.
        assert_eq!(IplSource::decode(&[]), IplSource::default());
        assert_eq!(IplSource::decode(&[1, 2]), IplSource::default());
        assert_eq!(IplSource::decode(&[0, 2, 2, 0]), IplSource::default());
        assert_eq!(IplSource::decode(&[1, 7, 9, 0]), IplSource::default());
    }

    #[test]
    fn the_once_only_slot_is_taken_once() {
        let mut r = Record::new(1);
        IplSource {
            next: Some(Slot::B),
            default: Some(Slot::A),
        }
        .store(&mut r)
        .unwrap();
        assert_eq!(IplSource::take(&mut r).unwrap(), (Slot::B, true));
        assert_eq!(IplSource::take(&mut r).unwrap(), (Slot::A, false));
        assert_eq!(IplSource::of(&r).next, None);
        assert_eq!(IplSource::of(&r).default, Some(Slot::A));
    }

    #[test]
    fn the_other_values_of_the_record_stay() {
        let mut r = Record::new(1);
        r.set_tlv(0x0105, vec![2]).unwrap();
        IplSource {
            next: Some(Slot::B),
            default: None,
        }
        .store(&mut r)
        .unwrap();
        assert_eq!(r.tlv(0x0105), Some(&[2u8][..]));
    }
}

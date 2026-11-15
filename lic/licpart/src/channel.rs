// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The partition service channel the system has (`docs/QSID.md`
//! section 11): TLV [`TAG_SERVICE_CHANNEL`], written by the system at
//! install and at each IPL, so a hypervisor reading the record before
//! it enters the kernel knows whether to wait for the system's hello,
//! and which version it will be. A record without it is a system that
//! has no channel.
//!
//! The data, little-endian: `layout u8` (1), `version u16` (the channel
//! protocol's highest), `service u64` and `console u64` (its
//! capabilities on each port). A reader takes what it knows and passes
//! over what a later layout adds after it.

use crate::error::LicError;
use crate::format::Record;

/// The record's TLV tag.
pub const TAG_SERVICE_CHANNEL: u16 = 0x010C;

/// The data's layout.
pub const LAYOUT: u8 = 1;

/// The data's length at layout 1.
const LEN: usize = 19;

/// What the system's channel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceChannel {
    /// The channel protocol's version, the highest the system speaks.
    pub version: u16,
    /// Its capabilities on the service port.
    pub service: u64,
    /// Its capabilities on the console port.
    pub console: u64,
}

impl ServiceChannel {
    /// The TLV's data.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(LEN);
        out.push(LAYOUT);
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.service.to_le_bytes());
        out.extend_from_slice(&self.console.to_le_bytes());
        out
    }

    /// From the TLV's data; `None` when it is too short or of layout 0.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < LEN || bytes[0] == 0 {
            return None;
        }
        let u64_at = |i: usize| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap_or([0; 8]));
        Some(Self {
            version: u16::from_le_bytes([bytes[1], bytes[2]]),
            service: u64_at(3),
            console: u64_at(11),
        })
    }

    /// The record's channel; `None` when the system says it has none.
    #[must_use]
    pub fn of(record: &Record) -> Option<Self> {
        record.tlv(TAG_SERVICE_CHANNEL).and_then(Self::decode)
    }

    /// Keep it in the record; unchanged data is not rewritten.
    ///
    /// # Errors
    /// The record has no room.
    pub fn store(&self, record: &mut Record) -> Result<(), LicError> {
        let data = self.encode();
        if record.tlv(TAG_SERVICE_CHANNEL) == Some(&data[..]) {
            return Ok(());
        }
        record.set_tlv(TAG_SERVICE_CHANNEL, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bytes_and_a_later_layout() {
        let c = ServiceChannel {
            version: 1,
            service: 0x1f,
            console: 0x60,
        };
        let bytes = c.encode();
        assert_eq!(
            bytes,
            [
                1, 1, 0, 0x1f, 0, 0, 0, 0, 0, 0, 0, 0x60, 0, 0, 0, 0, 0, 0, 0
            ]
        );
        assert_eq!(ServiceChannel::decode(&bytes), Some(c));
        let mut later = bytes.clone();
        later[0] = 2;
        later.extend_from_slice(b"more");
        assert_eq!(ServiceChannel::decode(&later), Some(c));
        assert_eq!(ServiceChannel::decode(&bytes[..18]), None);
        assert_eq!(ServiceChannel::decode(&[0; 19]), None);

        let mut r = Record::new(1);
        assert_eq!(ServiceChannel::of(&r), None);
        c.store(&mut r).unwrap();
        assert_eq!(ServiceChannel::of(&r), Some(c));
        c.store(&mut r).unwrap();
        assert_eq!(r.tlv(TAG_SERVICE_CHANNEL), Some(&bytes[..]));
    }
}

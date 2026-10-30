// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NTP header (RFC 5905 figure 8): 48 bytes, big-endian. What
//! follows it (extension fields, a MAC) is ignored on input and never
//! sent.

mod stamp;

pub use stamp::{Timestamp, UNIX_OFFSET, nanos_to_short, short_to_nanos};

use thiserror::Error;

/// The header's length.
pub const LEN: usize = 48;

/// The leap indicator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Leap {
    /// No warning.
    #[default]
    None,
    /// The last minute of the day has 61 seconds.
    Insert,
    /// The last minute of the day has 59 seconds.
    Delete,
    /// The clock is not synchronized.
    Unknown,
}

impl Leap {
    fn bits(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Insert => 1,
            Self::Delete => 2,
            Self::Unknown => 3,
        }
    }

    fn of(b: u8) -> Self {
        match b & 3 {
            0 => Self::None,
            1 => Self::Insert,
            2 => Self::Delete,
            _ => Self::Unknown,
        }
    }
}

/// The association mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 1: symmetric active.
    SymmetricActive,
    /// 2: symmetric passive.
    SymmetricPassive,
    /// 3: a client's request.
    Client,
    /// 4: a server's answer.
    Server,
    /// 5: broadcast.
    Broadcast,
    /// 6, 7 and 0: control, private, reserved.
    Other(u8),
}

impl Mode {
    fn bits(self) -> u8 {
        match self {
            Self::SymmetricActive => 1,
            Self::SymmetricPassive => 2,
            Self::Client => 3,
            Self::Server => 4,
            Self::Broadcast => 5,
            Self::Other(n) => n & 7,
        }
    }

    fn of(b: u8) -> Self {
        match b & 7 {
            1 => Self::SymmetricActive,
            2 => Self::SymmetricPassive,
            3 => Self::Client,
            4 => Self::Server,
            5 => Self::Broadcast,
            n => Self::Other(n),
        }
    }
}

/// Why bytes are not an NTP header.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PacketError {
    /// Fewer than 48 bytes.
    #[error("an NTP packet is 48 bytes or more, not {0}")]
    Short(usize),
    /// A version this codec does not speak.
    #[error("NTP version {0} is not supported")]
    Version(u8),
}

/// One NTP header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet {
    /// Leap indicator.
    pub leap: Leap,
    /// Version, 1 to 4.
    pub version: u8,
    /// Mode.
    pub mode: Mode,
    /// Stratum: 0 kiss-o'-death, 1 primary, 2-15 secondary, 16 unsynchronized.
    pub stratum: u8,
    /// Poll interval, log2 seconds.
    pub poll: i8,
    /// Precision, log2 seconds.
    pub precision: i8,
    /// Root delay, short format.
    pub root_delay: u32,
    /// Root dispersion, short format.
    pub root_dispersion: u32,
    /// Reference identifier: a source code (`LOCL`), an address or a
    /// kiss code.
    pub reference_id: [u8; 4],
    /// When the clock was last set.
    pub reference: Timestamp,
    /// The request's transmit time, echoed (T1).
    pub origin: Timestamp,
    /// When the request arrived (T2).
    pub receive: Timestamp,
    /// When this packet left (T3, or T1 in a request).
    pub transmit: Timestamp,
}

impl Default for Packet {
    fn default() -> Self {
        Self {
            leap: Leap::None,
            version: 4,
            mode: Mode::Client,
            stratum: 0,
            poll: 0,
            precision: 0,
            root_delay: 0,
            root_dispersion: 0,
            reference_id: [0; 4],
            reference: Timestamp::ZERO,
            origin: Timestamp::ZERO,
            receive: Timestamp::ZERO,
            transmit: Timestamp::ZERO,
        }
    }
}

impl Packet {
    /// A client's request sent at `transmit` (RFC 4330 section 5: only
    /// the version, mode and transmit time are set).
    #[must_use]
    pub fn request(transmit: Timestamp) -> Self {
        Self {
            transmit,
            ..Self::default()
        }
    }

    /// The 48 bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; LEN] {
        let mut b = [0u8; LEN];
        b[0] = (self.leap.bits() << 6) | ((self.version & 7) << 3) | self.mode.bits();
        b[1] = self.stratum;
        b[2] = self.poll.to_be_bytes()[0];
        b[3] = self.precision.to_be_bytes()[0];
        b[4..8].copy_from_slice(&self.root_delay.to_be_bytes());
        b[8..12].copy_from_slice(&self.root_dispersion.to_be_bytes());
        b[12..16].copy_from_slice(&self.reference_id);
        b[16..24].copy_from_slice(&self.reference.0.to_be_bytes());
        b[24..32].copy_from_slice(&self.origin.0.to_be_bytes());
        b[32..40].copy_from_slice(&self.receive.0.to_be_bytes());
        b[40..48].copy_from_slice(&self.transmit.0.to_be_bytes());
        b
    }

    /// From received bytes; anything after the header is ignored.
    pub fn decode(b: &[u8]) -> Result<Self, PacketError> {
        if b.len() < LEN {
            return Err(PacketError::Short(b.len()));
        }
        let version = (b[0] >> 3) & 7;
        if !(1..=4).contains(&version) {
            return Err(PacketError::Version(version));
        }
        let u32_at = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let u64_at = |i: usize| {
            let mut w = [0u8; 8];
            w.copy_from_slice(&b[i..i + 8]);
            Timestamp(u64::from_be_bytes(w))
        };
        Ok(Self {
            leap: Leap::of(b[0] >> 6),
            version,
            mode: Mode::of(b[0]),
            stratum: b[1],
            poll: i8::from_be_bytes([b[2]]),
            precision: i8::from_be_bytes([b[3]]),
            root_delay: u32_at(4),
            root_dispersion: u32_at(8),
            reference_id: [b[12], b[13], b[14], b[15]],
            reference: u64_at(16),
            origin: u64_at(24),
            receive: u64_at(32),
            transmit: u64_at(40),
        })
    }

    /// The kiss code of a kiss-o'-death packet (stratum 0), as text.
    #[must_use]
    pub fn kiss_code(&self) -> Option<String> {
        (self.stratum == 0 && self.mode == Mode::Server)
            .then(|| String::from_utf8_lossy(&self.reference_id).trim_end_matches('\0').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_bytes() {
        let p = Packet::request(Timestamp(0x0102_0304_0506_0708));
        let b = p.encode();
        // LI 0, VN 4, mode 3.
        assert_eq!(b[0], 0x23);
        assert_eq!(&b[1..40], &[0u8; 39][..]);
        assert_eq!(&b[40..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn round_trip() {
        let p = Packet {
            leap: Leap::Unknown,
            version: 3,
            mode: Mode::Server,
            stratum: 2,
            poll: 6,
            precision: -20,
            root_delay: 0x0000_1234,
            root_dispersion: 0x0000_0042,
            reference_id: *b"LOCL",
            reference: Timestamp(1),
            origin: Timestamp(2),
            receive: Timestamp(3),
            transmit: Timestamp(4),
        };
        let b = p.encode();
        assert_eq!(b[0], 0xDC);
        assert_eq!(b[3], 0xEC);
        let mut long = b.to_vec();
        long.extend_from_slice(&[0xAA; 20]);
        assert_eq!(Packet::decode(&long).unwrap(), p);
    }

    #[test]
    fn refusals() {
        assert_eq!(Packet::decode(&[0u8; 47]), Err(PacketError::Short(47)));
        let mut b = Packet::default().encode();
        b[0] = 0x3B; // version 7
        assert_eq!(Packet::decode(&b), Err(PacketError::Version(7)));
        let kod = Packet {
            mode: Mode::Server,
            stratum: 0,
            reference_id: *b"RATE",
            ..Packet::default()
        };
        assert_eq!(kod.kiss_code().as_deref(), Some("RATE"));
        assert_eq!(Packet::default().kiss_code(), None);
    }
}

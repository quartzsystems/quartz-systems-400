// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console mode: the local console (the display and its keyboard)
//! or the serial console, kept in the record (TLV [`TAG_CONSOLE_MODE`])
//! and honoured at the next IPL. A hypervisor that IPLs the system in a
//! partition writes it before the IPL.

use crate::error::LicError;
use crate::format::Record;

/// The record's TLV tag for the mode: one byte, [`ConsoleMode::as_u8`].
pub const TAG_CONSOLE_MODE: u16 = 0x0105;

/// Where the console is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsoleMode {
    /// The framebuffer and the keyboards; screens are echoed on serial.
    #[default]
    Local,
    /// The serial port alone: screens as text, keys as typed lines.
    Serial,
}

impl ConsoleMode {
    /// The byte in the record (1 local, 2 serial), also DST's option.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Local => 1,
            Self::Serial => 2,
        }
    }

    /// From the record byte or an option number.
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Local),
            2 => Some(Self::Serial),
            _ => None,
        }
    }

    /// From the record's TLV data.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        bytes.first().copied().and_then(Self::from_u8)
    }

    /// The record's TLV data.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        vec![self.as_u8()]
    }

    /// The record's mode; `None` when it holds none (the local console).
    #[must_use]
    pub fn of(record: &Record) -> Option<Self> {
        record.tlv(TAG_CONSOLE_MODE).and_then(Self::decode)
    }

    /// Keep the mode in the record.
    ///
    /// # Errors
    /// The record has no room.
    pub fn store(self, record: &mut Record) -> Result<(), LicError> {
        record.set_tlv(TAG_CONSOLE_MODE, self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_round_trip() {
        for m in [ConsoleMode::Local, ConsoleMode::Serial] {
            assert_eq!(ConsoleMode::decode(&m.encode()), Some(m));
        }
        assert_eq!(ConsoleMode::decode(&[]), None);
        assert_eq!(ConsoleMode::decode(&[9]), None);
    }

    #[test]
    fn the_record_keeps_the_bytes_it_always_had() {
        let mut r = Record::new(1);
        assert_eq!(ConsoleMode::of(&r), None);
        ConsoleMode::Serial.store(&mut r).unwrap();
        assert_eq!(r.tlv(0x0105), Some(&[2u8][..]));
        ConsoleMode::Local.store(&mut r).unwrap();
        assert_eq!(r.tlv(0x0105), Some(&[1u8][..]));
        assert_eq!(ConsoleMode::of(&r), Some(ConsoleMode::Local));
    }
}

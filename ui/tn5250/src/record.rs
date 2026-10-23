// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 5250 record (RFC 1205): a ten-byte General Data Stream header
//! (length, type `0x12A0`, a four-byte variable part holding the flags
//! and the operation code) and the data, ended on the wire by `IAC
//! EOR`.

use crate::Error;

/// The record type of every 5250 record.
pub const GDS: u16 = 0x12A0;
/// The fixed header length.
pub const HEADER_LEN: usize = 10;

/// Operation codes.
pub mod opcode {
    /// No operation.
    pub const NOP: u8 = 0x00;
    /// Invite: the host waits for input.
    pub const INVITE: u8 = 0x01;
    /// Output only.
    pub const OUTPUT_ONLY: u8 = 0x02;
    /// Put/Get: output, then the host waits for input.
    pub const PUT_GET: u8 = 0x03;
    /// Save screen.
    pub const SAVE_SCREEN: u8 = 0x04;
    /// Restore screen.
    pub const RESTORE_SCREEN: u8 = 0x05;
    /// Read immediate.
    pub const READ_IMMEDIATE: u8 = 0x06;
    /// Read screen.
    pub const READ_SCREEN: u8 = 0x08;
    /// Cancel invite.
    pub const CANCEL_INVITE: u8 = 0x0A;
    /// Message light on.
    pub const MESSAGE_ON: u8 = 0x0B;
    /// Message light off.
    pub const MESSAGE_OFF: u8 = 0x0C;

    /// The name for traces.
    pub fn name(op: u8) -> &'static str {
        match op {
            NOP => "no-op",
            INVITE => "invite",
            OUTPUT_ONLY => "output-only",
            PUT_GET => "put/get",
            SAVE_SCREEN => "save-screen",
            RESTORE_SCREEN => "restore-screen",
            READ_IMMEDIATE => "read-immediate",
            READ_SCREEN => "read-screen",
            CANCEL_INVITE => "cancel-invite",
            MESSAGE_ON => "message-on",
            MESSAGE_OFF => "message-off",
            _ => "?",
        }
    }
}

/// Flag bits of the first flag byte.
pub mod flags {
    /// Data stream output error.
    pub const ERR: u16 = 0x8000;
    /// The Attention key.
    pub const ATN: u16 = 0x4000;
    /// The System Request key.
    pub const SRQ: u16 = 0x0400;
    /// The Test Request key.
    pub const TRQ: u16 = 0x0200;
    /// Help pressed in the error state.
    pub const HLP: u16 = 0x0100;
}

/// A record off the wire or about to go on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The flags (two bytes).
    pub flags: u16,
    /// The operation code.
    pub opcode: u8,
    /// The data after the header.
    pub data: Vec<u8>,
}

impl Record {
    /// A record with no flags.
    pub fn new(opcode: u8, data: Vec<u8>) -> Self {
        Self {
            flags: 0,
            opcode,
            data,
        }
    }

    /// The bytes before `IAC EOR`: the header and the data.
    pub fn encode(&self) -> Vec<u8> {
        let len = HEADER_LEN + self.data.len();
        let mut out = Vec::with_capacity(len);
        out.extend((len as u16).to_be_bytes());
        out.extend(GDS.to_be_bytes());
        out.extend([0, 0, 4]);
        out.extend(self.flags.to_be_bytes());
        out.push(self.opcode);
        out.extend(&self.data);
        out
    }

    /// A record from the bytes between the previous `IAC EOR` and the
    /// next. The length field is taken as the record's own claim and
    /// checked against what arrived.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::Truncated { at: bytes.len() });
        }
        let kind = u16::from_be_bytes([bytes[2], bytes[3]]);
        if kind != GDS {
            return Err(Error::RecordType(kind));
        }
        let var = usize::from(bytes[6]);
        let header = 6 + var;
        if bytes.len() < header || var < 4 {
            return Err(Error::Truncated { at: bytes.len() });
        }
        let flags = u16::from_be_bytes([bytes[7], bytes[8]]);
        let opcode = bytes[9];
        Ok(Self {
            flags,
            opcode,
            data: bytes[header..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let r = Record {
            flags: flags::SRQ,
            opcode: opcode::INVITE,
            data: vec![1, 2, 3],
        };
        let bytes = r.encode();
        assert_eq!(bytes.len(), 13);
        assert_eq!(&bytes[..2], &[0, 13]);
        assert_eq!(Record::decode(&bytes).unwrap(), r);
    }
}

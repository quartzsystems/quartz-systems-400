// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The remote command and distributed program call server (`0xE008`,
//! port 8475): exchange attributes, run a command, call a program with
//! parameters, the job log messages that come back.

use crate::cp::rmtcmd as cp;
use crate::frame::Param;
use crate::message::HostMessage;
use crate::template::{template, templates};
use crate::{Error, rle};

/// Request and reply IDs.
pub mod id {
    crate::consts! {
        /// Exchange attributes.
        EXCHANGE_ATTRIBUTES = 0x1001,
        /// Its reply.
        EXCHANGE_ATTRIBUTES_REPLY = 0x8001,
        /// Run a command.
        RUN_COMMAND = 0x1002,
        /// Its reply.
        RUN_COMMAND_REPLY = 0x8002,
        /// Call a program.
        CALL_PROGRAM = 0x1003,
        /// Its reply when the program ran.
        CALL_PROGRAM_REPLY = 0x8003,
        /// End the conversation (no reply).
        END = 0x1004,
        /// The call program reply when the call failed (messages only).
        CALL_PROGRAM_FAILED = 0x9003,
    }
}

/// Message options (the template byte of run command and call program).
pub mod message_option {
    /// Return up to ten messages.
    pub const UP_TO_TEN: u8 = 0x00;
    /// Return no messages.
    pub const NONE: u8 = 0x01;
    /// Return all messages (datastream level 7 and up).
    pub const ALL: u8 = 0x02;
    /// Up to ten, extended layout (`0x1106`; level 10).
    pub const UP_TO_TEN_EXTENDED: u8 = 0x03;
    /// All, extended layout (level 10).
    pub const ALL_EXTENDED: u8 = 0x04;
    /// Up to ten, extended layout (level 11 and up).
    pub const UP_TO_TEN_LEVEL_11: u8 = 0x05;
    /// All, extended layout (level 11 and up).
    pub const ALL_LEVEL_11: u8 = 0x06;
}

/// Return codes (u16, the first word of each reply's template).
pub mod rc {
    /// Success.
    pub const OK: u16 = 0x0000;
    /// The command failed (messages follow).
    pub const COMMAND_FAILED: u16 = 0x0400;
    /// The program call failed (messages follow).
    pub const PROGRAM_FAILED: u16 = 0x0500;
}

/// Parameter usage codes (`0x1103`): the base usage plus 10 for plain
/// data, 20 for RLE data, 30 for input-output at datastream level 5 and
/// up; `0xFF` passes a null pointer.
pub mod usage {
    /// Input, plain.
    pub const INPUT: u16 = 11;
    /// Output, plain.
    pub const OUTPUT: u16 = 12;
    /// Input-output, plain.
    pub const INOUT: u16 = 13;
    /// Input, RLE compressed.
    pub const INPUT_RLE: u16 = 21;
    /// Output, RLE compressed.
    pub const OUTPUT_RLE: u16 = 22;
    /// Input-output, RLE compressed.
    pub const INOUT_RLE: u16 = 23;
    /// Input-output at datastream level 5 and up.
    pub const INOUT_V5: u16 = 33;
    /// A null parameter.
    pub const NULL: u16 = 0xFF;
}

template! {
    /// Exchange attributes request (`0x1001`).
    pub struct ExchangeAttributes {
        /// Client CCSID (13488 from `JTOpen`).
        ccsid: u32,
        /// National language version, four EBCDIC digits (`2924`).
        nlv: [u8; 4],
        /// Client version (1).
        client_version: u32,
        /// Client datastream level.
        level: u16,
    }
}

template! {
    /// Exchange attributes reply (`0x8001`).
    pub struct ExchangeAttributesReply {
        /// Return code.
        rc: u16,
        /// Server job CCSID.
        ccsid: u32,
        /// Server national language version.
        nlv: [u8; 4],
        /// Server version, `0x00VVRRMM`.
        server_version: u32,
        /// Server datastream level.
        level: u16,
    }
}

template! {
    /// Run command request (`0x1002`): the command in `0x1104` (CCSID u32
    /// then text) or `0x1101`.
    pub struct RunCommand {
        /// Message option.
        message_option: u8,
    }
}

template! {
    /// A reply with messages: run command (`0x8002`), call program
    /// (`0x8003`, the count is of output parameters) and its failure
    /// (`0x9003`).
    pub struct ReplyCount {
        /// Return code.
        rc: u16,
        /// How many messages (or output parameters) follow.
        count: u16,
    }
}

template! {
    /// Call program request (`0x1003`); each parameter is a `0x1103`.
    pub struct CallProgram {
        /// Program name (EBCDIC, 10).
        program: [u8; 10],
        /// Library name (EBCDIC, 10).
        library: [u8; 10],
        /// Message option.
        message_option: u8,
        /// Number of parameters.
        parameter_count: u16,
    }
}

templates! {
    /// The remote command server's templates.
    pub enum Template {
        /// `0x1001`.
        ExchangeAttributes(ExchangeAttributes) = Request id::EXCHANGE_ATTRIBUTES,
        /// `0x8001`.
        ExchangeAttributesReply(ExchangeAttributesReply) = Reply id::EXCHANGE_ATTRIBUTES_REPLY,
        /// `0x1002`.
        RunCommand(RunCommand) = Request id::RUN_COMMAND,
        /// `0x8002`.
        RunCommandReply(ReplyCount) = Reply id::RUN_COMMAND_REPLY,
        /// `0x1003`.
        CallProgram(CallProgram) = Request id::CALL_PROGRAM,
        /// `0x8003`.
        CallProgramReply(ReplyCount) = Reply id::CALL_PROGRAM_REPLY,
        /// `0x9003`.
        CallProgramFailed(ReplyCount) = Reply id::CALL_PROGRAM_FAILED,
        /// `0x1004`.
        End(crate::common::Empty) = Request id::END,
    }
}

/// A program parameter (`0x1103`). In a request: the maximum length,
/// the usage and the input bytes (trailing zeros trimmed, or RLE); in a
/// reply: the output length, the usage and the output bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramParameter {
    /// Maximum length (request) or output data length (reply).
    pub length: u32,
    /// Usage (`usage`).
    pub usage: u16,
    /// The bytes as sent.
    pub data: Vec<u8>,
}

impl ProgramParameter {
    /// Decode a `0x1103` parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 6 {
            return Err(Error::Short {
                what: "program parameter",
                need: 6,
                have: b.len(),
            });
        }
        Ok(Self {
            length: u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
            usage: u16::from_be_bytes([b[4], b[5]]),
            data: b[6..].to_vec(),
        })
    }

    /// The parameter.
    pub fn to_param(&self) -> Param {
        let mut d = self.length.to_be_bytes().to_vec();
        d.extend_from_slice(&self.usage.to_be_bytes());
        d.extend_from_slice(&self.data);
        Param::new(cp::PARAMETER, d)
    }

    /// Whether the data is RLE compressed.
    pub fn is_compressed(&self) -> bool {
        (21..=23).contains(&self.usage)
    }

    /// The data as the program sees it: expanded, padded with zeros to
    /// `length`.
    pub fn value(&self) -> Result<Vec<u8>, Error> {
        let mut v = if self.is_compressed() {
            rle::decompress(&self.data)?
        } else {
            self.data.clone()
        };
        if v.len() < self.length as usize {
            v.resize(self.length as usize, 0);
        }
        Ok(v)
    }
}

/// The command text of a run command request: its CCSID (when sent in
/// `0x1104`) and bytes.
pub fn command(params: &[Param]) -> Option<(Option<u32>, Vec<u8>)> {
    params.iter().find_map(|p| match p.cp {
        cp::COMMAND_CCSID if p.data.len() >= 4 => Some((
            Some(u32::from_be_bytes([
                p.data[0], p.data[1], p.data[2], p.data[3],
            ])),
            p.data[4..].to_vec(),
        )),
        cp::COMMAND => Some((None, p.data.clone())),
        _ => None,
    })
}

/// The program parameters of a call or its reply.
pub fn parameters(params: &[Param]) -> Result<Vec<ProgramParameter>, Error> {
    params
        .iter()
        .filter(|p| p.cp == cp::PARAMETER)
        .map(|p| ProgramParameter::decode(&p.data))
        .collect()
}

/// The extended messages of a reply.
pub fn messages(params: &[Param]) -> Result<Vec<HostMessage>, Error> {
    params
        .iter()
        .filter(|p| p.cp == cp::MESSAGE_EXTENDED)
        .map(|p| HostMessage::decode(&p.data))
        .collect()
}

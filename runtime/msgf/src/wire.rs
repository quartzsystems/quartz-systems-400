// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message descriptions on the wire between a job and the storage
//! service.

use ipc::{Dec, Wire, WireError};

use crate::{MsgDesc, VarType};

impl Wire for VarType {
    fn put(&self, out: &mut Vec<u8>) {
        match *self {
            Self::Char(n) => (0u8, n).put(out),
            Self::VaryChar(n) => (1u8, n).put(out),
            Self::Dec(d, s) => (2u8, d, s).put(out),
            Self::Hex(n) => (3u8, n).put(out),
            Self::QtdChar(n) => (4u8, n).put(out),
            Self::Bin(n) => (5u8, n).put(out),
            Self::Dts => 6u8.put(out),
            Self::CChar(n) => (7u8, n).put(out),
            Self::IType(n) => (8u8, n).put(out),
            Self::Utc => 9u8.put(out),
            Self::Spp => 10u8.put(out),
            Self::Syp => 11u8.put(out),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Char(Wire::get(d)?),
            1 => Self::VaryChar(Wire::get(d)?),
            2 => Self::Dec(Wire::get(d)?, Wire::get(d)?),
            3 => Self::Hex(Wire::get(d)?),
            4 => Self::QtdChar(Wire::get(d)?),
            5 => Self::Bin(Wire::get(d)?),
            6 => Self::Dts,
            7 => Self::CChar(Wire::get(d)?),
            8 => Self::IType(Wire::get(d)?),
            9 => Self::Utc,
            10 => Self::Spp,
            11 => Self::Syp,
            tag => {
                return Err(WireError::Tag {
                    what: "VarType",
                    tag,
                });
            }
        })
    }
}

ipc::wire_struct!(MsgDesc {
    id,
    severity,
    text,
    help,
    vars,
    dft_reply,
    valid_replies,
    dump
});

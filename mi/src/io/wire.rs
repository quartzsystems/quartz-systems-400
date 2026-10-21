// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The I/O types on the wire between a job and the storage service.
//! A request borrows its record and key; [`OwnedRequest`] carries them.

use ipc::{Dec, Wire, WireError, wire_enum, wire_struct};

use super::{
    DataAreaKind, DataAreaValue, FileInfo, IoFail, IoOp, IoRequest, IoResult, KeyRef, OpenSpec,
};

impl Wire for IoOp {
    fn put(&self, out: &mut Vec<u8>) {
        let code: u8 = match self {
            Self::Read => 1,
            Self::ReadP => 2,
            Self::ReadE => 3,
            Self::ReadPE => 4,
            Self::Chain => 5,
            Self::SetLL => 6,
            Self::SetGT => 7,
            Self::Write => 8,
            Self::Update => 9,
            Self::Delete => 10,
            Self::Unlock => 11,
            Self::Feod => 12,
        };
        code.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let tag = u8::get(d)?;
        Self::from_code(i64::from(tag)).ok_or(WireError::Tag { what: "IoOp", tag })
    }
}

/// [`qir::FileUsage`] as a byte.
struct Usage(qir::FileUsage);

impl Wire for Usage {
    fn put(&self, out: &mut Vec<u8>) {
        let b: u8 = match self.0 {
            qir::FileUsage::Input => 0,
            qir::FileUsage::Output => 1,
            qir::FileUsage::Update => 2,
            qir::FileUsage::All => 3,
        };
        b.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(Self(match u8::get(d)? {
            0 => qir::FileUsage::Input,
            1 => qir::FileUsage::Output,
            2 => qir::FileUsage::Update,
            3 => qir::FileUsage::All,
            tag => {
                return Err(WireError::Tag {
                    what: "FileUsage",
                    tag,
                });
            }
        }))
    }
}

impl Wire for OpenSpec {
    fn put(&self, out: &mut Vec<u8>) {
        self.file.put(out);
        self.lib.put(out);
        Usage(self.usage).put(out);
        self.keyed.put(out);
        self.lvlchk.put(out);
        self.member.put(out);
        self.actgrp.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(Self {
            file: Wire::get(d)?,
            lib: Wire::get(d)?,
            usage: Usage::get(d)?.0,
            keyed: Wire::get(d)?,
            lvlchk: Wire::get(d)?,
            member: Wire::get(d)?,
            actgrp: Wire::get(d)?,
        })
    }
}

wire_struct!(IoFail {
    status,
    msgid,
    data
});
wire_struct!(FileInfo {
    file,
    lib,
    member,
    format,
    rec_len
});
wire_struct!(IoResult {
    status,
    msgid,
    eof,
    found,
    equal,
    rrn,
    key,
    format,
    member,
    record
});
wire_enum!(DataAreaKind {
    0 => Char,
    1 => Dec,
    2 => Lgl,
});
wire_struct!(DataAreaValue {
    kind,
    len,
    decimals,
    value
});

/// A key that owns its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedKey {
    /// Key field bytes.
    Fields {
        /// The bytes.
        bytes: Vec<u8>,
    },
    /// A relative record number.
    Rrn {
        /// The number.
        rrn: u64,
    },
    /// No key.
    None,
}

wire_enum!(OwnedKey {
    0 => Fields { bytes },
    1 => Rrn { rrn },
    2 => None,
});

/// An [`IoRequest`] that owns its record and key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedRequest {
    /// The operation.
    pub op: IoOp,
    /// The record to write or update.
    pub record: Option<Vec<u8>>,
    /// The key or record number.
    pub key: OwnedKey,
    /// The record format.
    pub format: Option<String>,
    /// Read without a lock.
    pub nolock: bool,
}

wire_struct!(OwnedRequest {
    op,
    record,
    key,
    format,
    nolock
});

impl OwnedRequest {
    /// Copy a borrowed request.
    #[must_use]
    pub fn from_request(r: &IoRequest<'_>) -> Self {
        Self {
            op: r.op,
            record: r.record.map(<[u8]>::to_vec),
            key: match r.key {
                KeyRef::Fields(b) => OwnedKey::Fields { bytes: b.to_vec() },
                KeyRef::Rrn(n) => OwnedKey::Rrn { rrn: n },
                KeyRef::None => OwnedKey::None,
            },
            format: r.format.map(str::to_string),
            nolock: r.nolock,
        }
    }

    /// Borrow it as a request.
    #[must_use]
    pub fn as_request(&self) -> IoRequest<'_> {
        IoRequest {
            op: self.op,
            record: self.record.as_deref(),
            key: match &self.key {
                OwnedKey::Fields { bytes } => KeyRef::Fields(bytes),
                OwnedKey::Rrn { rrn } => KeyRef::Rrn(*rrn),
                OwnedKey::None => KeyRef::None,
            },
            format: self.format.as_deref(),
            nolock: self.nolock,
        }
    }
}

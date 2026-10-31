// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*SAVF`: its attributes and the save stream it holds. The byte
//! space is an eight-byte big-endian length, then the stream; the
//! stream counts as 528-byte records the way `MAXRCDS` and `DSPFD` see
//! a save file.

use crate::obj::ops::{self, CreateParams};
use crate::obj::space::Space;
use crate::obj::{ObjectId, types};
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes per save file record.
pub const RECORD_LEN: u64 = 528;
/// The stream's length prefix.
const LEN: u64 = 8;

/// `SavfAttr` size.
pub const ATTR_LEN: usize = 16;

/// `SavfAttr`.
pub static LAYOUT: Layout = Layout {
    name: "SavfAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("max_records", 0, 4),
        field("wait_file", 4, 4),
        field("share", 8, 1),
        field("reserved", 9, 7),
    ],
};

/// A save file's attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SavfAttr {
    /// `MAXRCDS`, 0 for `*NOMAX`.
    pub max_records: u32,
    /// `WAITFILE` seconds; 0 `*IMMED`, -1 `*CLS`.
    pub wait_file: i32,
    /// `SHARE(*YES)`.
    pub share: bool,
}

impl SavfAttr {
    fn encode(self) -> [u8; ATTR_LEN] {
        let mut b = [0u8; ATTR_LEN];
        b[0..4].copy_from_slice(&self.max_records.to_le_bytes());
        b[4..8].copy_from_slice(&self.wait_file.to_le_bytes());
        b[8] = u8::from(self.share);
        b
    }

    fn decode(b: &[u8]) -> Self {
        Self {
            max_records: u32::from_le_bytes(b[0..4].try_into().expect("4")),
            wait_file: i32::from_le_bytes(b[4..8].try_into().expect("4")),
            share: b[8] != 0,
        }
    }
}

/// Records a stream of `bytes` takes.
#[must_use]
pub fn records(bytes: u64) -> u64 {
    bytes.div_ceil(RECORD_LEN)
}

/// `CRTSAVF`.
pub fn create(txn: &mut Txn<'_>, params: &CreateParams, attr: SavfAttr) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &CreateParams {
            type_code: types::SAVF,
            attribute: String::new(),
            ..params.clone()
        },
    )?;
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())?;
    Ok(id)
}

/// A save file's attributes.
pub fn attr(txn: &mut Txn<'_>, id: ObjectId) -> Result<SavfAttr> {
    let (page, _) = ops::read_header(txn, id)?;
    Ok(SavfAttr::decode(&ops::read_attr(txn, page)?))
}

/// Change a save file's attributes.
pub fn set_attr(txn: &mut Txn<'_>, id: ObjectId, attr: SavfAttr) -> Result<()> {
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())
}

/// Bytes of the stream it holds; 0 when empty.
pub fn len(txn: &mut Txn<'_>, id: ObjectId) -> Result<u64> {
    let (s, _) = Space::load(txn, id)?;
    if s.bytes() < LEN {
        return Ok(0);
    }
    let b = s.read(txn, 0, 8)?;
    let n = u64::from_be_bytes(b.try_into().expect("8"));
    if n > s.bytes() - LEN {
        return Err(StoreError::Corrupt {
            page: s.header_page,
            what: "save file length".into(),
        });
    }
    Ok(n)
}

/// The stream.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    let n = len(txn, id)?;
    if n == 0 {
        return Ok(Vec::new());
    }
    let (s, _) = Space::load(txn, id)?;
    s.read(txn, LEN, usize::try_from(n).expect("stream fits memory"))
}

/// Replace the stream. Refused when it needs more records than
/// `MAXRCDS` allows.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, data: &[u8]) -> Result<()> {
    let a = attr(txn, id)?;
    if a.max_records != 0 && records(data.len() as u64) > u64::from(a.max_records) {
        return Err(StoreError::BadValue {
            what: format!("save file needs {} records", records(data.len() as u64)),
        });
    }
    let total = LEN + data.len() as u64;
    let s = Space::ensure(txn, id, total, 0)?;
    s.write(txn, 0, &(data.len() as u64).to_be_bytes())?;
    s.write(txn, LEN, data)?;
    Space::truncate(txn, id, total)?;
    Ok(())
}

/// `CLRSAVF`.
pub fn clear(txn: &mut Txn<'_>, id: ObjectId) -> Result<()> {
    Space::truncate(txn, id, 0)?;
    Ok(())
}

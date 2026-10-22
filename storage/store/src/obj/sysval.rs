// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*SYSVAL`: one object in QSYS holding the system values as the
//! `sysval` crate's TLV blob. See the format spec §10.

use crate::bytes::{get_u64, put_u64};
use crate::obj::ObjectId;
use crate::obj::ops::{self, CreateParams};
use crate::obj::space::Space;
use crate::obj::types;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 16;
/// Largest blob.
pub const MAX_BLOB: usize = 1 << 20;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "SysvalAttr",
    size: Some(ATTR_LEN),
    fields: &[field("blob_len", 0, 8), field("reserved", 8, 8)],
};

/// Create the `*SYSVAL` object in `qsys` with an initial blob.
pub fn create(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    now: u64,
    system: &str,
    blob: &[u8],
) -> Result<ObjectId> {
    let params = CreateParams {
        library: qsys,
        name: qsname::Name::parse("QSYSVAL").expect("valid"),
        type_code: types::SYSVAL,
        attribute: String::new(),
        text: "System values".into(),
        owner: "QSYS".into(),
        system: system.into(),
        now,
        by_system: true,
    };
    let id = ops::create(txn, &params)?;
    write(txn, id, blob)?;
    Ok(id)
}

fn check(txn: &mut Txn<'_>, id: ObjectId) -> Result<u64> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::SYSVAL {
        return Err(StoreError::ObjectNotFound {
            name: h.name.to_string(),
            library: String::new(),
        });
    }
    let a = ops::read_attr(txn, page)?;
    Ok(get_u64(&a, 0))
}

/// The blob.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    let len = check(txn, id)?;
    if len == 0 {
        return Ok(Vec::new());
    }
    let (space, _) = Space::load(txn, id)?;
    space.read(txn, 0, usize::try_from(len).expect("blob fits"))
}

/// Replace the blob.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, blob: &[u8]) -> Result<()> {
    if blob.len() > MAX_BLOB {
        return Err(StoreError::BadValue {
            what: format!("system value blob of {} bytes", blob.len()),
        });
    }
    check(txn, id)?;
    let mut space = Space::ensure(txn, id, blob.len() as u64, 0)?;
    if !blob.is_empty() {
        space.write_relocating(txn, 0, blob)?;
    }
    let (page, _) = ops::read_header(txn, id)?;
    let mut a = [0u8; ATTR_LEN];
    put_u64(&mut a, 0, blob.len() as u64);
    ops::write_attr(txn, page, 0, &a)
}

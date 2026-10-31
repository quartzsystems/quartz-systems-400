// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*SAVF` as the byte stream `/QSYS.LIB` shows: the save stream the
//! store keeps in it (`store::obj::savf`).

use ifs::{Errno, Result};
use store::obj::ObjectId;
use store::obj::savf;
use store::txn::Txn;

use crate::access::store_errno;

/// The stream's length.
pub fn len(txn: &mut Txn<'_>, id: ObjectId) -> Result<u64> {
    savf::len(txn, id).map_err(|e| match e {
        store::StoreError::Corrupt { .. } => Errno::EDAMAGE,
        e => store_errno(&e),
    })
}

/// The stream.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    savf::read(txn, id).map_err(|e| store_errno(&e))
}

/// Replace the stream.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, data: &[u8]) -> Result<()> {
    savf::write(txn, id, data).map_err(|e| store_errno(&e))
}

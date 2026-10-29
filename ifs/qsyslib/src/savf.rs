// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*SAVF` as the byte stream `/QSYS.LIB` shows: its object space
//! holds an eight-byte big-endian length, then the bytes.
//! MIGRATE(phase11): the save file's record format replaces this when
//! `SAVOBJ`/`RSTOBJ` write save files; the stream stays the records.

use ifs::{Errno, Result};
use store::obj::ObjectId;
use store::obj::space::Space;
use store::txn::Txn;

use crate::access::store_errno;

const LEN: u64 = 8;

/// The stream's length.
pub fn len(txn: &mut Txn<'_>, id: ObjectId) -> Result<u64> {
    let (s, _) = Space::load(txn, id).map_err(|e| store_errno(&e))?;
    if s.bytes() < LEN {
        return Ok(0);
    }
    let b = s.read(txn, 0, 8).map_err(|e| store_errno(&e))?;
    let n = u64::from_be_bytes(b.try_into().map_err(|_| Errno::EDAMAGE)?);
    if n > s.bytes() - LEN {
        return Err(Errno::EDAMAGE);
    }
    Ok(n)
}

/// The stream.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    let n = len(txn, id)?;
    if n == 0 {
        return Ok(Vec::new());
    }
    let (s, _) = Space::load(txn, id).map_err(|e| store_errno(&e))?;
    let n = usize::try_from(n).map_err(|_| Errno::EOVERFLOW)?;
    s.read(txn, LEN, n).map_err(|e| store_errno(&e))
}

/// Replace the stream.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, data: &[u8]) -> Result<()> {
    let total = LEN + data.len() as u64;
    let s = Space::ensure(txn, id, total, 0).map_err(|e| store_errno(&e))?;
    s.write(txn, 0, &(data.len() as u64).to_be_bytes())
        .map_err(|e| store_errno(&e))?;
    s.write(txn, LEN, data).map_err(|e| store_errno(&e))?;
    Space::truncate(txn, id, total).map_err(|e| store_errno(&e))?;
    Ok(())
}

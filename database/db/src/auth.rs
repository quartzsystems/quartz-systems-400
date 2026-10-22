// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Authority for database operations: each names its entry in
//! `security/sec/required.toml` and the store's guard decides.

use store::obj::ObjectId;
use store::obj::guard;
use store::txn::Txn;

use crate::Result;
use crate::odp::Usage;

/// Check `op` against `object`.
pub fn check(txn: &mut Txn<'_>, op: &'static str, object: ObjectId) -> Result<()> {
    guard::check_id(txn, op, object)?;
    Ok(())
}

/// Check an open of `file` for `usage`: `*OBJOPR` and the data
/// authority of each use.
pub fn check_open(txn: &mut Txn<'_>, file: ObjectId, usage: Usage) -> Result<()> {
    if usage.input {
        check(txn, "file.open-input", file)?;
    }
    if usage.output {
        check(txn, "file.open-output", file)?;
    }
    if usage.update {
        check(txn, "file.open-update", file)?;
    }
    if usage.delete {
        check(txn, "file.open-delete", file)?;
    }
    Ok(())
}

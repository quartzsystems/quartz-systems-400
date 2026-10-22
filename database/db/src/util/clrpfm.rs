// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CLRPFM`: remove every record of a physical member.

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{Hold, member_of, physical};
use crate::error::Result;
use crate::file;
use crate::member::Member;

/// Clear the member; the records it held are returned. The member is
/// allocated `*EXCL`, so an open data path on it refuses the clear.
pub fn clrpfm(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
) -> Result<u64> {
    crate::auth::check(txn, "file.clear", file)?;
    let desc = file::describe(txn, file)?;
    physical(&desc, "CLRPFM")?;
    let entry = member_of(&desc, member)?;
    let hold = Hold::take(
        txn,
        job,
        &[
            (file, LockState::SharedUpdate),
            (entry.object, LockState::Exclusive),
        ],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        let n = m.attr().record_count;
        m.clear(txn)?;
        Ok(n)
    });
    hold.release(txn);
    result
}

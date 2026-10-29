// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Replace a data member's records with raw fixed-length records, as
//! the stream file systems write a member opened as a byte stream.

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{Hold, member_of, physical};
use crate::error::Result;
use crate::file;
use crate::member::Member;

/// Clear the member and write `records` (each padded with `pad` or cut
/// to the record length) with no null fields. The member is allocated
/// `*EXCL` as `CLRPFM` allocates it. Returns the record count.
pub fn put_records(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    records: &[Vec<u8>],
    pad: u8,
    now: u64,
) -> Result<u64> {
    crate::auth::check(txn, "file.open-output", file)?;
    let desc = file::describe(txn, file)?;
    physical(&desc, "PUTRCDS")?;
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
        m.clear(txn)?;
        let len = usize::try_from(m.format().format.record_len).expect("fits");
        let nulls = vec![0u8; m.core.null_map_len()];
        let mut n = 0u64;
        for r in records {
            let mut rec = r[..r.len().min(len)].to_vec();
            rec.resize(len, pad);
            m.write_new(txn, &rec, &nulls)?;
            n += 1;
        }
        Ok(n)
    });
    hold.release(txn);
    let n = result?;
    file::chgpfm(txn, file, entry.name, None, None, now)?;
    Ok(n)
}

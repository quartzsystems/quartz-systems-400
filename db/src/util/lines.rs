// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lines of text as records: each line in CP037 from the first byte of
//! the record, blank-padded or cut to its length, whatever the fields.
//! `CPYSPLF` copies a spooled file this way.

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{Hold, member_of, physical};
use crate::error::Result;
use crate::file;
use crate::member::Member;

/// Write `lines` to the member, after clearing it when `replace`; the
/// records written.
pub fn put_lines(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    lines: &[String],
    replace: bool,
) -> Result<u64> {
    let desc = file::describe(txn, file)?;
    physical(&desc, "CPYSPLF")?;
    let entry = member_of(&desc, member)?;
    let lock = if replace {
        LockState::Exclusive
    } else {
        LockState::SharedUpdate
    };
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::SharedUpdate), (entry.object, lock)],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        if replace {
            m.clear(txn)?;
        }
        let fmt = m.format().clone();
        let len = usize::try_from(fmt.format.record_len).unwrap_or(0);
        let nulls = vec![0u8; usize::from(fmt.null_map_len)];
        let mut n = 0;
        for l in lines {
            let mut record = vec![0x40u8; len];
            for (slot, ch) in record.iter_mut().zip(l.chars()) {
                *slot = ccsid::byte_of(ch).unwrap_or(0x40);
            }
            m.write_new(txn, &record, &nulls)?;
            n += 1;
        }
        Ok(n)
    });
    hold.release(txn);
    result
}

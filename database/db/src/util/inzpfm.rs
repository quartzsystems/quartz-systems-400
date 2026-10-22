// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INZPFM`: records added to a member, each holding every field's
//! `DFT` (`RECORDS(*DFT)`) or written and deleted at once
//! (`RECORDS(*DLT)`), so the member's slots exist ahead of use.

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use crate::error::Result;
use crate::file;
use crate::format::FormatDef;
use crate::member::Member;
use crate::record::{self, Value, set_null};
use crate::util::{Hold, member_of, physical};

/// Add `records` records to the member; how many were added.
pub fn inzpfm(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    records: u64,
    deleted: bool,
) -> Result<u64> {
    crate::auth::check(txn, "file.open-output", file)?;
    let desc = file::describe(txn, file)?;
    physical(&desc, "INZPFM")?;
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
        let (data, nulls) = default_record(m.format())?;
        for _ in 0..records {
            let rrn = m.write_new(txn, &data, &nulls)?;
            if deleted {
                m.delete(txn, rrn)?;
            }
        }
        Ok(records)
    });
    hold.release(txn);
    result
}

/// A record of every field's `DFT`, or the type's zero (blanks, 0);
/// `DFT(*NULL)` sets the null map. The record bytes and the null map.
pub fn default_record(f: &FormatDef) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut data = vec![ccsid::SPACE; usize::try_from(f.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(f.null_map_len)];
    for (i, field) in f.format.fields.iter().enumerate() {
        let (v, null) = crate::util::cpyf::map::default_of(field)?;
        let v = if null { Value::Null } else { v };
        record::write_field(field, &mut data, &v)?;
        if null && let Some(bit) = f.null_index(i) {
            set_null(&mut nulls, bit, true);
        }
    }
    Ok((data, nulls))
}

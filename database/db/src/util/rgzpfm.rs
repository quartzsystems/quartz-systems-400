// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RGZPFM`: squeeze the deleted records out of a physical member,
//! renumbering the rest, in arrival or `KEYFILE` order.

use std::collections::HashSet;

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{Hold, Order, each_record, member_of, physical};
use crate::error::{DbError, Result};
use crate::file;
use crate::member::{Member, lob};

/// `KEYFILE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyFile {
    /// `*NONE`: arrival sequence.
    None,
    /// `*FILE`: the member's own access path.
    File,
    /// A logical file member whose path orders the records; records its
    /// select/omit leaves out follow in arrival sequence.
    Logical {
        /// The logical file.
        file: ObjectId,
        /// Its member.
        member: MemberSpec,
    },
}

/// What a reorganization did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RgzReport {
    /// Records kept.
    pub records: u64,
    /// Deleted slots removed.
    pub deleted_removed: u64,
    /// LOB values freed: no record named them.
    pub lobs_freed: u64,
}

/// Reorganize the member. It is allocated `*EXCL` for the duration.
pub fn rgzpfm(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    keyfile: &KeyFile,
) -> Result<RgzReport> {
    crate::auth::check(txn, "file.reorganize", file)?;
    let desc = file::describe(txn, file)?;
    physical(&desc, "RGZPFM")?;
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
        let r = reorganize(txn, &mut m, keyfile, &desc)?;
        crate::journal::member_event(txn, &m, "MR")?;
        Ok(r)
    });
    hold.release(txn);
    result
}

fn reorganize(txn: &mut Txn<'_>, m: &mut Member, keyfile: &KeyFile, desc: &file::FileDesc) -> Result<RgzReport> {
    let mut records: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut seen = HashSet::new();
    match keyfile {
        KeyFile::None => {}
        KeyFile::File => {
            if m.attr().key_len == 0 {
                return Err(DbError::Copy {
                    what: format!("KEYFILE(*FILE): member {} is in arrival sequence", m.name()),
                });
            }
            let keyed = Order {
                keyed: true,
                ..Order::default()
            };
            each_record(txn, m, &keyed, &mut |_, s| {
                seen.insert(s.rrn);
                records.push((s.record, s.null_map));
                Ok(true)
            })?;
        }
        KeyFile::Logical { file, member } => {
            let mut lf = Member::open_by(txn, *file, *member)?;
            if !lf.is_logical() || lf.attr().key_len == 0 {
                return Err(DbError::Copy {
                    what: format!("KEYFILE: {} is not a keyed logical member", lf.name()),
                });
            }
            lf.ensure_path(txn, &mut |_, _| {})?;
            let p = lf.path().expect("keyed");
            let mine = m.id();
            for k in p.all(txn)? {
                let base = lf.bases.get(usize::from(k.format)).map(|b| b.member);
                if base != Some(mine) || !seen.insert(k.rrn) {
                    continue;
                }
                if let Some(s) = m.read(txn, k.rrn)?.filter(|s| !s.deleted) {
                    records.push((s.record, s.null_map));
                }
            }
        }
    }
    // Arrival sequence for the rest (everything, for *NONE).
    for s in m.scan(txn, 1, usize::MAX)? {
        if !seen.contains(&s.rrn) {
            records.push((s.record, s.null_map));
        }
    }
    let deleted_removed = m.attr().deleted_count;
    // The records keep their LOB references; the values no record names
    // any more (updated or deleted away) are freed.
    let lob_fields: Vec<_> = desc
        .formats
        .first()
        .map(|f| f.format.fields.iter().filter(|f| f.data_type.is_lob()).cloned().collect())
        .unwrap_or_default();
    if lob_fields.is_empty() {
        m.clear(txn)?;
    } else {
        m.clear_keeping_lobs(txn)?;
    }
    let mut named = HashSet::new();
    for (record, nulls) in &records {
        for f in &lob_fields {
            if let lob::LobRef::Tree { id, .. } = lob::LobRef::decode(crate::record::field_bytes(f, record))? {
                named.insert(id);
            }
        }
        m.write_new(txn, record, nulls)?;
    }
    let lobs_freed = if lob_fields.is_empty() { 0 } else { lob::sweep(txn, m.id(), &named)? };
    Ok(RgzReport {
        records: records.len() as u64,
        deleted_removed,
        lobs_freed,
    })
}

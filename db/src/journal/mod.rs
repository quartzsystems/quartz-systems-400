// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journaling of physical files: `STRJRNPF`/`ENDJRNPF`, and the entries
//! the members, open data paths and utilities write through here. The
//! journal itself is the `jrn` crate's.

use jrn::{Draft, Journal, JrnObjEntry, ObjectRef, RecordImage, code};
use qsname::Name;
use store::obj::{JobHandle, LockState, ObjectId, types};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{self, FileDesc, FileKind};
use crate::member::Member;
use crate::util::Hold;

/// Who is changing a member and under which commit cycle; carried by
/// every `Member` so its entries say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JrnCtx {
    /// The job.
    pub job: JobHandle,
    /// The commit cycle, 0 outside commitment control.
    pub commit_cycle: u64,
    /// The time to stamp entries with; the clock when `None`.
    pub now: Option<u64>,
}

impl Default for JrnCtx {
    fn default() -> Self {
        Self {
            job: JobHandle(0),
            commit_cycle: 0,
            now: None,
        }
    }
}

/// The object reference of a file member.
#[must_use]
pub fn object_ref(desc: &FileDesc, member: Option<Name>) -> ObjectRef {
    ObjectRef {
        library: desc.library_name,
        object: desc.name(),
        member,
        type_code: types::FILE,
    }
}

fn draft(desc: &FileDesc, member: Option<Name>, code: u8, kind: &str, ctx: JrnCtx) -> Draft {
    let mut d = Draft::new(code, kind, ctx.job)
        .object(object_ref(desc, member))
        .cycle(ctx.commit_cycle);
    if let Some(t) = ctx.now {
        d = d.at(t);
    }
    d
}

/// Append to the file's journal, if it has one.
fn append(txn: &mut Txn<'_>, desc: &FileDesc, d: Draft) -> Result<Option<u64>> {
    if desc.attr.journal.is_none() {
        return Ok(None);
    }
    let mut j = Journal::load(txn, desc.attr.journal)?;
    Ok(Some(j.append(txn, d)?))
}

/// A record-level entry (`R PT`, `R UB`, `R UP`, `R DL`) for `rrn`.
pub fn record(
    txn: &mut Txn<'_>,
    m: &Member,
    kind: &str,
    rrn: u64,
    record: &[u8],
    null_map: &[u8],
) -> Result<()> {
    if m.file.attr.journal.is_none() {
        return Ok(());
    }
    let image = RecordImage {
        record: record.to_vec(),
        null_map: null_map.to_vec(),
    };
    let d = draft(&m.file, Some(m.name()), code::RECORD, kind, m.jrn)
        .count(rrn)
        .data(image.encode());
    append(txn, &m.file, d).map(|_| ())
}

/// A file-level entry about a member (`F OP`, `F CL`, `F MC`, ...).
/// Open and close entries obey `OMTJRNE(*OPNCLO)`.
pub fn member_event(txn: &mut Txn<'_>, m: &Member, kind: &str) -> Result<()> {
    if m.file.attr.journal.is_none() {
        return Ok(());
    }
    if m.file.attr.jrn_omit_opnclo && (kind == "OP" || kind == "CL") {
        return Ok(());
    }
    let d = draft(&m.file, Some(m.name()), code::FILE, kind, m.jrn);
    append(txn, &m.file, d).map(|_| ())
}

/// A file-level entry about a member named, without a `Member` open
/// (`F MA`, `F MD`, `F JM`, `F EJ`).
pub fn member_entry(
    txn: &mut Txn<'_>,
    desc: &FileDesc,
    member: Name,
    kind: &str,
    ctx: JrnCtx,
) -> Result<()> {
    let d = draft(desc, Some(member), code::FILE, kind, ctx);
    append(txn, desc, d).map(|_| ())
}

/// `STRJRNPF`: journal the physical file's members to `journal`.
pub fn strjrnpf(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    journal: ObjectId,
    both_images: bool,
    omit_open_close: bool,
    now: u64,
) -> Result<()> {
    let desc = file::describe(txn, file)?;
    if desc.attr.kind == FileKind::Logical {
        return Err(DbError::Definition {
            what: format!("STRJRNPF: {} is a logical file", desc.name()),
        });
    }
    let mut j = Journal::load(txn, journal)?;
    if !desc.attr.journal.is_none() {
        let current = Journal::load(txn, desc.attr.journal)
            .map_or_else(|_| desc.attr.journal.to_string(), |j| j.name().to_string());
        return Err(jrn::JrnError::AlreadyJournaled {
            name: desc.name().to_string(),
            journal: current,
        }
        .into());
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = (|| {
        j.add_object(
            txn,
            JrnObjEntry {
                object: file,
                object_type: types::FILE,
                both_images,
                omit_open_close,
                started: now,
            },
            &desc.name().to_string(),
        )?;
        let desc = file::change_attr(txn, file, &mut |a| {
            a.journal = journal;
            a.jrn_both_images = both_images;
            a.jrn_omit_opnclo = omit_open_close;
        })?;
        let ctx = JrnCtx {
            job,
            commit_cycle: 0,
            now: Some(now),
        };
        for m in &desc.members {
            member_entry(txn, &desc, m.name, "JM", ctx)?;
        }
        Ok(())
    })();
    hold.release(txn);
    r
}

/// `ENDJRNPF`: stop journaling the file.
pub fn endjrnpf(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId, now: u64) -> Result<()> {
    let desc = file::describe(txn, file)?;
    if desc.attr.journal.is_none() {
        return Err(jrn::JrnError::NotJournaled {
            name: desc.name().to_string(),
        }
        .into());
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = (|| {
        let ctx = JrnCtx {
            job,
            commit_cycle: 0,
            now: Some(now),
        };
        for m in &desc.members {
            member_entry(txn, &desc, m.name, "EJ", ctx)?;
        }
        let mut j = Journal::load(txn, desc.attr.journal)?;
        j.remove_object(txn, file, &desc.name().to_string())?;
        file::change_attr(txn, file, &mut |a| {
            a.journal = ObjectId::NONE;
            a.jrn_both_images = false;
            a.jrn_omit_opnclo = false;
        })?;
        Ok(())
    })();
    hold.release(txn);
    r
}

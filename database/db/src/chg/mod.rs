// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGPF` and `CHGLF`: attributes after the fact, and a re-created
//! physical file format that keeps or converts the data.

use dds::PfDefinition;
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId, ops};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{self, FileDesc, FileKind, Maint, Recover, Size, Wait};
use crate::format::FormatDef;
use crate::file::MemberEntry;
use crate::member::{Member, rebuild};
use crate::util::Hold;
use crate::util::cpyf::{FmtOpt, copy_lobs, map};

/// `CHGPF` parameters; `None` leaves the attribute as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChgPf {
    /// `SIZE`.
    pub size: Option<Size>,
    /// `MAXMBRS`, 0 for `*NOMAX`.
    pub max_members: Option<u32>,
    /// `REUSEDLT`.
    pub reusedlt: Option<bool>,
    /// `SHARE`.
    pub share: Option<bool>,
    /// `LVLCHK`.
    pub lvlchk: Option<bool>,
    /// `MAINT`.
    pub maint: Option<Maint>,
    /// `RECOVER`.
    pub recover: Option<Recover>,
    /// `FRCRATIO`, 0 for `*NONE`.
    pub frcratio: Option<u16>,
    /// `WAITFILE`.
    pub wait_file: Option<Wait>,
    /// `WAITRCD`.
    pub wait_rcd: Option<Wait>,
    /// `FRCACCPTH`.
    pub frcaccpth: Option<bool>,
    /// `ALWUPD`.
    pub alwupd: Option<bool>,
    /// `ALWDLT`.
    pub alwdlt: Option<bool>,
    /// `CCSID`.
    pub ccsid: Option<u16>,
    /// `EXPDATE`, Unix seconds, 0 for `*NONE`.
    pub expdate: Option<u64>,
    /// `TEXT`.
    pub text: Option<String>,
    /// Change time.
    pub now: u64,
}

/// `CHGLF` parameters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChgLf {
    /// `MAINT`.
    pub maint: Option<Maint>,
    /// `RECOVER`.
    pub recover: Option<Recover>,
    /// `SHARE`.
    pub share: Option<bool>,
    /// `LVLCHK`.
    pub lvlchk: Option<bool>,
    /// `WAITFILE`.
    pub wait_file: Option<Wait>,
    /// `WAITRCD`.
    pub wait_rcd: Option<Wait>,
    /// `FRCACCPTH`.
    pub frcaccpth: Option<bool>,
    /// `TEXT`.
    pub text: Option<String>,
    /// Change time.
    pub now: u64,
}

fn set<T>(slot: &mut T, v: Option<T>) {
    if let Some(v) = v {
        *slot = v;
    }
}

/// Change a physical file's attributes. `REUSEDLT` rebuilds or drops
/// every member's free-slot tree; `SIZE` applies to every member.
pub fn chgpf(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId, c: &ChgPf) -> Result<()> {
    crate::auth::check(txn, "file.change", file)?;
    let desc = file::describe(txn, file)?;
    if desc.attr.kind == FileKind::Logical {
        return Err(DbError::Definition {
            what: format!("CHGPF: {} is a logical file", desc.name()),
        });
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = apply_pf(txn, file, c);
    hold.release(txn);
    r
}

fn apply_pf(txn: &mut Txn<'_>, file: ObjectId, c: &ChgPf) -> Result<()> {
    let desc = file::change_attr(txn, file, &mut |a| {
        set(&mut a.size, c.size);
        set(&mut a.max_members, c.max_members);
        set(&mut a.reusedlt, c.reusedlt);
        set(&mut a.share, c.share);
        set(&mut a.lvlchk, c.lvlchk);
        set(&mut a.maint, c.maint);
        set(&mut a.recover, c.recover);
        set(&mut a.frcratio, c.frcratio);
        set(&mut a.wait_file, c.wait_file);
        set(&mut a.wait_rcd, c.wait_rcd);
        set(&mut a.frcaccpth, c.frcaccpth);
        set(&mut a.alwupd, c.alwupd);
        set(&mut a.alwdlt, c.alwdlt);
        set(&mut a.ccsid, c.ccsid);
        set(&mut a.expdate, c.expdate);
    })?;
    if let Some(t) = &c.text {
        ops::change_text(txn, file, t, c.now)?;
    }
    if c.reusedlt.is_some() || c.size.is_some() {
        for e in &desc.members {
            let mut m = Member::open(txn, &desc, e)?;
            if let Some(yes) = c.reusedlt {
                m.set_reuse_deleted(txn, yes)?;
            }
            if let Some(s) = c.size {
                m.set_limits(txn, s.max_records(), s.increment)?;
            }
        }
    }
    Ok(())
}

/// Change a logical file's attributes. Going to `MAINT(*IMMED)` rebuilds
/// every invalid path now.
pub fn chglf(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId, c: &ChgLf) -> Result<()> {
    crate::auth::check(txn, "file.change", file)?;
    let desc = file::describe(txn, file)?;
    if desc.attr.kind != FileKind::Logical {
        return Err(DbError::Definition {
            what: format!("CHGLF: {} is not a logical file", desc.name()),
        });
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = apply_lf(txn, file, c);
    hold.release(txn);
    r
}

fn apply_lf(txn: &mut Txn<'_>, file: ObjectId, c: &ChgLf) -> Result<()> {
    let desc = file::change_attr(txn, file, &mut |a| {
        set(&mut a.maint, c.maint);
        set(&mut a.recover, c.recover);
        set(&mut a.share, c.share);
        set(&mut a.lvlchk, c.lvlchk);
        set(&mut a.wait_file, c.wait_file);
        set(&mut a.wait_rcd, c.wait_rcd);
        set(&mut a.frcaccpth, c.frcaccpth);
    })?;
    if let Some(t) = &c.text {
        ops::change_text(txn, file, t, c.now)?;
    }
    if c.maint == Some(Maint::Immed) {
        for e in &desc.members {
            Member::open(txn, &desc, e)?.ensure_path(txn, &mut |_, _| {})?;
        }
    }
    Ok(())
}

/// `CHGPF SRCFILE(...)`: the file takes the new DDS. When the record
/// format kept its level identifier and keys the data stays as it is;
/// otherwise every member's records are converted through a copy, field
/// by field as `CPYF FMTOPT(*MAP *DROP)` maps them. A file with
/// dependent logical files refuses a conversion.
pub fn chgpf_srcfile(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    def: &PfDefinition,
) -> Result<()> {
    chgpf_srcfile_paused(txn, job, file, def, &mut crate::no_pause)
}

/// [`chgpf_srcfile`] that pauses between batches of the records it
/// reads to convert, the file held `*EXCL` throughout. What it writes
/// (the members cleared, the format replaced, the records written in
/// the new one) is one step: a part of it committed alone would lose
/// records.
pub fn chgpf_srcfile_paused(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    def: &PfDefinition,
    pause: crate::Pause<'_>,
) -> Result<()> {
    crate::auth::check(txn, "file.change", file)?;
    let desc = file::describe(txn, file)?;
    if desc.attr.kind != FileKind::Physical {
        return Err(DbError::Definition {
            what: format!("CHGPF SRCFILE: {} is not a data physical file", desc.name()),
        });
    }
    let new = FormatDef::new(def.format.clone(), def.unique, def.dup_order)?;
    let old = &desc.formats[0];
    let key_of = |k: &dds::KeyField| (k.name, k.field, k.descend, k.transform, k.noaltseq);
    let same_keys = new.format.keys.len() == old.format.keys.len()
        && new
            .format
            .keys
            .iter()
            .zip(&old.format.keys)
            .all(|(a, b)| key_of(a) == key_of(b));
    let same = new.level_id == old.level_id
        && same_keys
        && new.unique == old.unique
        && new.dup_order == old.dup_order;
    if !same && !desc.dependents.is_empty() {
        return Err(DbError::Definition {
            what: format!(
                "record format {} changed and {} has dependent logical files; delete them first",
                new.format.name,
                desc.name()
            ),
        });
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = if same {
        file::replace_formats(txn, file, &[new])
    } else {
        convert(txn, job, &desc, old, new, pause)
    };
    hold.release(txn);
    r?;
    // Every member must still open and read.
    let desc = file::describe(txn, file)?;
    for e in &desc.members {
        Member::open(txn, &desc, e)?;
    }
    let _ = MemberSpec::First;
    Ok(())
}

/// Every member's records into the new format, by field name, each
/// member rebuilt beside the old one (`member::rebuild`): the records
/// read, converted and written in batches, pausing between them, the
/// old members as they were until the switch at the end, one short
/// step. A rebuild that fails or is ended deletes what it made.
fn convert(
    txn: &mut Txn<'_>,
    job: JobHandle,
    desc: &FileDesc,
    old: &FormatDef,
    new: FormatDef,
    pause: crate::Pause<'_>,
) -> Result<()> {
    // What a rebuild cut short by a crash left.
    for e in &desc.members {
        rebuild::abandon(txn, job, e.object)?;
    }
    let plan = map::plan(
        old,
        &new,
        FileKind::Physical,
        FileKind::Physical,
        FmtOpt::MapDrop,
    )?;
    let mut shadows = Vec::with_capacity(desc.members.len());
    for e in &desc.members {
        shadows.push(rebuild::begin(txn, desc, e, &new)?);
    }
    // The new members, read by nobody: no journal, no dependents.
    let mut beside = desc.clone();
    beside.formats = vec![new.clone()];
    beside.attr.journal = ObjectId::NONE;
    beside.dependents.clear();
    let mut parted = false;
    let copied = desc.members.iter().zip(&shadows).try_for_each(|(e, s)| {
        let to = MemberEntry { object: *s, ..*e };
        copy_converted(txn, (desc, e), (&beside, &to), (old, &plan), &mut |t| {
            let p = pause(t)?;
            parted |= p;
            Ok(p)
        })
    });
    if let Err(err) = copied {
        for e in &desc.members {
            rebuild::abandon(txn, job, e.object)?;
        }
        // Parts were committed: so is their undoing.
        if parted {
            txn.commit_part()?;
        }
        return Err(err);
    }
    // The switch.
    for (e, s) in desc.members.iter().zip(&shadows) {
        rebuild::finish(txn, job, e.object)?;
        file::set_member_object(txn, desc.id, e.name, *s)?;
    }
    file::replace_formats(txn, desc.id, &[new])
}

/// One member's records converted into its new member, in batches.
fn copy_converted(
    txn: &mut Txn<'_>,
    (desc, from_e): (&FileDesc, &MemberEntry),
    (beside, to_e): (&FileDesc, &MemberEntry),
    (old, plan): (&FormatDef, &map::Plan),
    pause: crate::Pause<'_>,
) -> Result<()> {
    let new = &beside.formats[0];
    let lobs: Vec<usize> = new
        .format
        .fields
        .iter()
        .enumerate()
        .filter(|(_, f)| f.data_type.is_lob())
        .map(|(i, _)| i)
        .collect();
    let batch = usize::try_from(crate::PAUSE_EVERY).expect("fits");
    let mut from = Member::open(txn, desc, from_e)?;
    let mut to = Member::open(txn, beside, to_e)?;
    let mut copies = Vec::new();
    let mut next = 1;
    loop {
        let slots = from.scan(txn, next, batch)?;
        for s in &slots {
            let mut rec = vec![0u8; usize::try_from(new.format.record_len).expect("fits")];
            let mut nulls = vec![0u8; usize::from(new.null_map_len)];
            plan.apply(old, new, s, 0, &mut rec, &mut nulls)?;
            copy_lobs(txn, &lobs, new, &mut rec, (from_e.object, to_e.object), &mut copies)?;
            to.write_new(txn, &rec, &nulls).map_err(|e| match e {
                // A unique key the old records repeat.
                DbError::DuplicateKey { file, member, key } => DbError::DuplicateKeys { file, member, key },
                e => e,
            })?;
        }
        match slots.last() {
            Some(last) if slots.len() == batch => next = last.rrn + 1,
            _ => return Ok(()),
        }
        if pause(txn)? {
            from = Member::open(txn, desc, from_e)?;
            to = Member::open(txn, beside, to_e)?;
        }
    }
}

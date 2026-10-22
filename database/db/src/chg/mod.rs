// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGPF` and `CHGLF`: attributes after the fact, and a re-created
//! physical file format that keeps the data.

use dds::PfDefinition;
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId, ops};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{self, FileKind, Maint, Recover, Size, Wait};
use crate::format::FormatDef;
use crate::member::Member;
use crate::util::Hold;

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

/// `CHGPF SRCFILE(...)`: the file takes the new DDS when its record
/// format has the same level identifier and keys, so the data stays.
/// Anything else is refused: a conversion is a later phase's job.
pub fn chgpf_srcfile(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    def: &PfDefinition,
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
    if new.level_id != old.level_id {
        return Err(DbError::Definition {
            what: format!(
                "record format {} changed: level {} was {}; the data cannot be kept",
                new.format.name, new.level_id, old.level_id
            ),
        });
    }
    let key_of = |k: &dds::KeyField| (k.name, k.field, k.descend, k.transform, k.noaltseq);
    let same_keys = new.format.keys.len() == old.format.keys.len()
        && new
            .format
            .keys
            .iter()
            .zip(&old.format.keys)
            .all(|(a, b)| key_of(a) == key_of(b));
    if !same_keys || new.unique != old.unique || new.dup_order != old.dup_order {
        return Err(DbError::Definition {
            what: format!(
                "access path of {} changed; delete and re-create the file",
                new.format.name
            ),
        });
    }
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::Exclusive)],
        &desc.name().to_string(),
    )?;
    let r = file::replace_formats(txn, file, &[new]);
    hold.release(txn);
    r?;
    // Every member must still open and read.
    for e in &desc.members {
        Member::open(txn, &desc, e)?;
    }
    let _ = MemberSpec::First;
    Ok(())
}

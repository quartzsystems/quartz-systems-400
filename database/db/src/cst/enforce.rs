// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the constraints ask before a record changes, and the delete
//! rules' changes to dependent files. The open data path calls these
//! for every write, update and delete, from SQL, RPG and RLA alike;
//! unique keys are checked by the member's `link`.
//!
//! `NO ACTION` is checked as `RESTRICT` is, row by row, after this
//! row's cascades (`docs/DB_OPEN_QUESTIONS.md`): the difference shows
//! only within one statement that both deletes a parent and its
//! dependents' parents, and in the SQLSTATE it reports. As on the
//! primary, a parent whose rule is `NO ACTION` changes only when it and
//! its dependent are journaled (`CPF4328`, `SQL7008`).

#![allow(clippy::many_single_char_names)]

use qsname::MemberSpec;
use store::obj::ObjectId;
use store::txn::Txn;

use super::{Active, Kind, Rule};
use crate::cmt::CommitRef;
use crate::error::{DbError, Result};
use crate::member::Member;
use crate::path;

/// Most dependent records one parent's delete changes in one go.
const DEPENDENTS_MAX: usize = usize::MAX;

/// A constraint of the member's file in check pending refuses any use.
pub fn usable(m: &Member) -> Result<()> {
    let Some(a) = &m.csts else { return Ok(()) };
    if let Some(c) = a.csts.iter().find(|c| c.check_pending && c.enabled) {
        return Err(DbError::CheckPending {
            cst: c.name.clone(),
            file: m.file.name().to_string(),
        });
    }
    Ok(())
}

/// Before a record is written: its checks and foreign keys.
pub fn before_write(txn: &mut Txn<'_>, m: &Member, record: &[u8], nulls: &[u8]) -> Result<()> {
    let Some(a) = m.csts.clone() else {
        return Ok(());
    };
    usable(m)?;
    checks(m, &a, record, nulls)?;
    foreign_keys(txn, m, &a, record, nulls, None)
}

/// Before record `rrn` is replaced: the new record's checks and changed
/// foreign keys, and the dependents of a changed parent key.
pub fn before_update(
    txn: &mut Txn<'_>,
    m: &Member,
    rrn: u64,
    record: &[u8],
    nulls: &[u8],
) -> Result<()> {
    let Some(a) = m.csts.clone() else {
        return Ok(());
    };
    usable(m)?;
    rules_journaled(txn, m, &a, true)?;
    let old = m.read(txn, rrn)?.ok_or(DbError::RecordNotFound { rrn })?;
    checks(m, &a, record, nulls)?;
    foreign_keys(
        txn,
        m,
        &a,
        record,
        nulls,
        Some((&old.record, &old.null_map)),
    )?;
    // A parent key that changes while dependents match it: the update
    // rules are RESTRICT and NO ACTION only.
    for d in a.csts.iter().filter(|c| c.kind == Kind::Dependent) {
        let Some((child, ri, pk)) = dependent(txn, &a, d) else {
            continue;
        };
        let pk = &a.keys[pk];
        let before = pk.key(&old.record, &old.null_map)?;
        let after = pk.key(record, nulls)?;
        if before == after || path::key_has_null(&pk.format, &old.null_map) {
            continue;
        }
        if !children(txn, &child, ri, &before, 1)?.is_empty() {
            let c = &child.csts.as_ref().expect("has constraints").csts[ri];
            return Err(DbError::RefRestricted {
                cst: c.name.clone(),
                dependent: child.file.name().to_string(),
                file: m.file.name().to_string(),
                member: m.name().to_string(),
                update: true,
                no_action: c.update_rule == Rule::NoAction,
            });
        }
    }
    Ok(())
}

/// Before record `rrn` is deleted: each dependent file's delete rule,
/// its changes made under the same commit definition as the parent's.
pub fn before_delete(
    txn: &mut Txn<'_>,
    m: &mut Member,
    rrn: u64,
    cmt: Option<&CommitRef>,
) -> Result<()> {
    let Some(a) = m.csts.clone() else {
        return Ok(());
    };
    usable(m)?;
    rules_journaled(txn, m, &a, false)?;
    let mut changed = false;
    let old = m.read(txn, rrn)?.ok_or(DbError::RecordNotFound { rrn })?;
    for d in a.csts.iter().filter(|c| c.kind == Kind::Dependent) {
        let Some((mut child, ri, pk)) = dependent(txn, &a, d) else {
            continue;
        };
        let pk = &a.keys[pk];
        if path::key_has_null(&pk.format, &old.null_map) {
            continue;
        }
        let key = pk.key(&old.record, &old.null_map)?;
        let rrns = children(txn, &child, ri, &key, DEPENDENTS_MAX)?;
        if rrns.is_empty() {
            continue;
        }
        let c = child.csts.as_ref().expect("has constraints").csts[ri].clone();
        if !c.delete_rule.changes_dependents() {
            return Err(DbError::RefRestricted {
                cst: c.name.clone(),
                dependent: child.file.name().to_string(),
                file: m.file.name().to_string(),
                member: m.name().to_string(),
                update: false,
                no_action: c.delete_rule == Rule::NoAction,
            });
        }
        // The rule changes the dependents: both files journaled.
        for f in [&m.file, &child.file] {
            if f.attr.journal.is_none() {
                return Err(DbError::RuleNeedsJournal {
                    file: m.file.name().to_string(),
                });
            }
        }
        child.jrn = m.jrn;
        if let Some(cmt) = cmt {
            child.jrn.commit_cycle = cmt.borrow_mut().begin_change(txn, &child)?;
        }
        for crrn in rrns {
            if c.delete_rule == Rule::Cascade {
                before_delete(txn, &mut child, crrn, cmt)?;
                child.delete(txn, crrn)?;
            } else {
                {
                    let slot = child
                        .read(txn, crrn)?
                        .ok_or(DbError::RecordNotFound { rrn: crrn })?;
                    let (mut rec, mut nulls) = (slot.record.clone(), slot.null_map.clone());
                    let fmt = child.format().clone();
                    let dft = crate::odp::lfwrite::default_record(&fmt)?;
                    for col in &c.columns {
                        let (i, f) = fmt.format.field(col).ok_or_else(|| DbError::Corrupt {
                            what: format!("constraint {} names field {col}", c.name),
                        })?;
                        let ni = fmt.null_index(i);
                        let (off, len) = (f.offset as usize, f.byte_len as usize);
                        if c.delete_rule == Rule::SetNull {
                            if let Some(ni) = ni {
                                crate::record::set_null(&mut nulls, ni, true);
                            }
                        } else {
                            rec[off..off + len].copy_from_slice(&dft.0[off..off + len]);
                            if let Some(ni) = ni {
                                crate::record::set_null(
                                    &mut nulls,
                                    ni,
                                    crate::record::is_null(&dft.1, Some(ni)),
                                );
                            }
                        }
                    }
                    before_update_excluding(txn, &child, crrn, &rec, &nulls, (m.id(), rrn))?;
                    child.update(txn, crrn, &rec, &nulls)?;
                }
            }
        }
        changed = true;
    }
    // A self-referencing file: this handle as the dependents' changes left it.
    if changed {
        m.refresh(txn)?;
    }
    Ok(())
}

/// `before_update` for a `SET DEFAULT`/`SET NULL` change, where the
/// parent row being deleted (`gone`) no longer counts as a parent.
fn before_update_excluding(
    txn: &mut Txn<'_>,
    m: &Member,
    rrn: u64,
    record: &[u8],
    nulls: &[u8],
    gone: (ObjectId, u64),
) -> Result<()> {
    let Some(a) = m.csts.clone() else {
        return Ok(());
    };
    let old = m.read(txn, rrn)?.ok_or(DbError::RecordNotFound { rrn })?;
    checks(m, &a, record, nulls)?;
    foreign_keys_excluding(
        txn,
        m,
        &a,
        record,
        nulls,
        Some((&old.record, &old.null_map)),
        Some(gone),
    )
}

/// A parent file whose dependents' delete rules change them is updated
/// and deleted from only while it and those dependents are journaled, as
/// IBM refuses at once whatever the rows (`SQL7008` reason 3, `CPF4328`).
fn rules_journaled(txn: &mut Txn<'_>, m: &Member, a: &Active, update: bool) -> Result<()> {
    for d in a.csts.iter().filter(|c| c.kind == Kind::Dependent) {
        let child = if d.other == m.file.id {
            a.csts.clone()
        } else {
            super::list(txn, d.other).unwrap_or_default()
        };
        let Some(ri) = child
            .iter()
            .find(|c| c.kind == Kind::Referential && c.name == d.other_name)
        else {
            continue;
        };
        // As on the primary, both files are journaled for a delete of the
        // parent under any delete rule but RESTRICT, and for an update
        // under an update rule of NO ACTION or a delete rule that changes
        // dependents.
        let needs = if update {
            ri.update_rule != Rule::Restrict || ri.delete_rule.changes_dependents()
        } else {
            ri.delete_rule != Rule::Restrict
        };
        if !needs || !ri.enabled {
            continue;
        }
        let child_jrn = if d.other == m.file.id {
            m.file.attr.journal
        } else {
            crate::file::describe(txn, d.other)?.attr.journal
        };
        if m.file.attr.journal.is_none() || child_jrn.is_none() {
            return Err(DbError::RuleNeedsJournal {
                file: m.file.name().to_string(),
            });
        }
    }
    Ok(())
}

fn checks(m: &Member, a: &Active, record: &[u8], nulls: &[u8]) -> Result<()> {
    let fmt = m.format();
    for c in a
        .csts
        .iter()
        .filter(|c| c.kind == Kind::Check && c.active())
    {
        let Some(e) = &c.check else { continue };
        let row = super::check::Row {
            format: fmt,
            data: record,
            nulls,
        };
        if !e.met(&row)? {
            return Err(DbError::CheckViolation {
                cst: c.name.clone(),
                file: m.file.name().to_string(),
                member: m.name().to_string(),
            });
        }
    }
    Ok(())
}

fn foreign_keys(
    txn: &mut Txn<'_>,
    m: &Member,
    a: &Active,
    record: &[u8],
    nulls: &[u8],
    old: Option<(&[u8], &[u8])>,
) -> Result<()> {
    foreign_keys_excluding(txn, m, a, record, nulls, old, None)
}

/// Every foreign key of the record, unless null or unchanged, must
/// match a parent key.
fn foreign_keys_excluding(
    txn: &mut Txn<'_>,
    m: &Member,
    a: &Active,
    record: &[u8],
    nulls: &[u8],
    old: Option<(&[u8], &[u8])>,
    gone: Option<(ObjectId, u64)>,
) -> Result<()> {
    for k in &a.keys {
        let c = &a.csts[k.index];
        if c.kind != Kind::Referential || !c.active() || path::key_has_null(&k.format, nulls) {
            continue;
        }
        let key = k.key(record, nulls)?;
        if let Some((orec, onulls)) = old
            && !path::key_has_null(&k.format, onulls)
            && k.key(orec, onulls)? == key
        {
            continue;
        }
        let parent = Member::open_by(txn, c.other, MemberSpec::First)?;
        let pa = parent.csts.clone().ok_or_else(|| no_parent_key(c))?;
        let pk = pa.key_of(&c.other_name).ok_or_else(|| no_parent_key(c))?;
        let tree = parent.cst_tree().expect("keyed");
        let found = pa.find(txn, &tree, pk, &key, 2)?;
        let found = found
            .iter()
            .filter(|r| gone != Some((parent.id(), **r)))
            .count();
        // A row that is its own parent (a self-referencing file) counts.
        let own = c.other == m.file.id && pk.key(record, nulls)? == key;
        if found == 0 && !own {
            return Err(DbError::RefNoParent {
                cst: c.name.clone(),
                file: m.file.name().to_string(),
                member: m.name().to_string(),
            });
        }
    }
    Ok(())
}

fn no_parent_key(c: &super::Constraint) -> DbError {
    DbError::Corrupt {
        what: format!(
            "referential constraint {} has no parent key {}",
            c.name, c.other_name
        ),
    }
}

/// The dependent member, its referential constraint's index and the
/// parent key's index in `a.keys`, for a `Dependent` entry; `None` when
/// the dependent constraint is disabled or gone.
fn dependent(
    txn: &mut Txn<'_>,
    a: &Active,
    d: &super::Constraint,
) -> Option<(Member, usize, usize)> {
    let child = Member::open_by(txn, d.other, MemberSpec::First).ok()?;
    let ca = child.csts.clone()?;
    let ri = ca
        .csts
        .iter()
        .position(|c| c.kind == Kind::Referential && c.name == d.other_name)?;
    if !ca.csts[ri].active() {
        return None;
    }
    let pk = a
        .keys
        .iter()
        .position(|k| a.csts[k.index].name == ca.csts[ri].other_name)?;
    Some((child, ri, pk))
}

/// The dependent records whose foreign key is `key`.
fn children(
    txn: &mut Txn<'_>,
    child: &Member,
    ri: usize,
    key: &[u8],
    max: usize,
) -> Result<Vec<u64>> {
    let ca = child.csts.as_ref().expect("has constraints");
    let Some(k) = ca.keys.iter().find(|k| k.index == ri) else {
        return Ok(Vec::new());
    };
    let tree = child.cst_tree().expect("keyed");
    ca.find(txn, &tree, k, key, max)
}

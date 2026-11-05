// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Adding, removing and changing constraints: the library functions
//! behind `ADDPFCST`, `RMVPFCST`, `CHGPFCST` and SQL's `ALTER TABLE`
//! and `CREATE TABLE`. A failure leaves the transaction to be aborted:
//! the definition is stored before the records are checked.

#![allow(clippy::many_single_char_names)]

use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use super::{Constraint, Kind, Rule, check, list, store};
use crate::error::{DbError, Result};
use crate::file::{self, FileDesc, FileKind};
use crate::member::Member;
use crate::path;
use crate::sqlinf::names::{self, Space, Target};

/// Why a constraint cannot be added, removed or changed. Each has the
/// SQLCODE an SQL statement reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The name is taken in the library (`SQL0601`).
    Exists,
    /// A column is not in the file (`SQL0205`).
    NoColumn,
    /// A column is named twice, or is a LOB (`SQL0537`).
    BadColumns,
    /// The file has a primary key already (`SQL0624`).
    SecondPrimary,
    /// A unique or primary key over the same columns exists (`SQL0541`).
    SameKey,
    /// A primary key over a null-capable column (`SQL0542`).
    Nullable,
    /// Records repeat the unique key (`SQL0603`).
    DuplicateData,
    /// Records break the check (`SQL0544`).
    CheckData,
    /// Records' foreign keys have no parent (`SQL0667`).
    RefData,
    /// The foreign key does not match a parent key (`SQL0538`).
    KeyMismatch,
    /// `SET NULL` on a foreign key that cannot be null (`SQL0629`).
    SetNullNotNull,
    /// The constraint is not there (`SQL0204`).
    NotFound,
    /// A parent key referential constraints depend on (`SQL0616`).
    HasDependents,
    /// Not a physical data file (`SQL7008`).
    NotPhysical,
    /// A unique or primary key cannot be disabled.
    NotDisableable,
}

fn refuse(msgid: &'static str, reason: Refusal, cst: &str, what: impl Into<String>) -> DbError {
    DbError::Constraint { msgid, reason, cst: cst.to_string(), what: what.into() }
}

/// A constraint to add.
#[derive(Clone, Debug, PartialEq)]
pub struct NewCst {
    /// Its name; `None` for IBM's generated one.
    pub name: Option<String>,
    /// What it is (never `Dependent`).
    pub kind: Kind,
    /// Key or foreign key fields.
    pub columns: Vec<Name>,
    /// Referential: the parent file.
    pub parent: ObjectId,
    /// Referential: the parent key's fields; empty for its primary key.
    pub parent_columns: Vec<Name>,
    /// `DLTRULE`.
    pub delete_rule: Rule,
    /// `UPDRULE`.
    pub update_rule: Rule,
    /// Check: the condition's text and program.
    pub check: Option<(String, check::Expr)>,
    /// Records that break it leave it in check pending (`ADDPFCST`)
    /// rather than refusing it (SQL).
    pub pending_ok: bool,
}

impl NewCst {
    /// A constraint of `kind` over `columns`.
    #[must_use]
    pub fn new(kind: Kind, columns: Vec<Name>) -> Self {
        Self {
            name: None,
            kind,
            columns,
            parent: ObjectId::NONE,
            parent_columns: Vec::new(),
            delete_rule: Rule::NoAction,
            update_rule: Rule::NoAction,
            check: None,
            pending_ok: false,
        }
    }
}

/// What an add did, with the informational messages IBM sends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The constraints' names.
    pub names: Vec<String>,
    /// `(message ID, data)`: `CPI32B4` a parent not journaled, `CPI32B5`
    /// another referential constraint between the files, `CPD32C5` and
    /// `CPD32B7` a constraint left in check pending.
    pub messages: Vec<(&'static str, String)>,
}

fn physical(desc: &FileDesc, msgid: &'static str, cst: &str) -> Result<()> {
    if desc.attr.kind == FileKind::Physical {
        Ok(())
    } else {
        Err(refuse(msgid, Refusal::NotPhysical, cst, format!("{} is not a physical data file", desc.name())))
    }
}

/// `ADDPFCST`: add `new` to `file`.
#[allow(clippy::too_many_lines)]
pub fn add(txn: &mut Txn<'_>, file: ObjectId, new: &NewCst, now: u64) -> Result<Report> {
    const ADD: &str = "CPF32B0";
    crate::auth::check(txn, "file.change", file)?;
    let desc = file::describe(txn, file)?;
    let given = new.name.clone().unwrap_or_default();
    physical(&desc, ADD, &given)?;
    let lib = desc.header.library;
    let mut csts = list(txn, file)?;
    let fmt = desc.formats[0].clone();
    let mut c = Constraint::new("", new.kind, new.columns.clone());
    if let Some((text, e)) = &new.check {
        c.columns = e.columns();
        c.text.clone_from(text);
        c.check = Some(e.clone());
    }
    let name = if let Some(n) = &new.name {
        if names::find(txn, lib, Space::Constraint, n)?.is_some() {
            return Err(refuse(ADD, Refusal::Exists, n, format!("constraint {n} exists in library {}", desc.library_name)));
        }
        n.clone()
    } else {
        let lib_name = desc.library_name.to_string();
        let taken: Vec<String> = names::list(txn, lib, Space::Constraint)?.into_iter().map(|(n, _)| n).collect();
        super::generated_name(&lib_name, &desc.name().to_string(), &c, &|n| taken.iter().any(|t| t == n))
    };
    c.name.clone_from(&name);
    if c.kind == Kind::PrimaryKey && csts.iter().any(|x| x.kind == Kind::PrimaryKey) {
        return Err(refuse(ADD, Refusal::SecondPrimary, &name, format!("file {} has a primary key", desc.name())));
    }
    for (i, col) in c.columns.iter().enumerate() {
        let Some((_, f)) = fmt.format.field(col) else {
            return Err(refuse(ADD, Refusal::NoColumn, &name, format!("field {col} is not in file {}", desc.name())));
        };
        if c.kind.keyed() && (f.data_type.is_lob() || c.columns[..i].contains(col)) {
            return Err(refuse(ADD, Refusal::BadColumns, &name, format!("field {col} cannot be a key of constraint {name}")));
        }
    }
    if c.kind.unique() {
        if c.kind == Kind::PrimaryKey && csts.iter().any(|x| x.kind == Kind::PrimaryKey) {
            return Err(refuse(ADD, Refusal::SecondPrimary, &name, format!("file {} has a primary key", desc.name())));
        }
        if csts.iter().any(|x| x.kind.unique() && x.columns == c.columns) {
            return Err(refuse(ADD, Refusal::SameKey, &name, format!("a unique key over the same fields exists for {}", desc.name())));
        }
        if c.kind == Kind::PrimaryKey
            && let Some(col) = c.columns.iter().find(|col| fmt.format.field(col).is_some_and(|(_, f)| f.allow_null))
        {
            return Err(refuse(ADD, Refusal::Nullable, &name, format!("primary key field {col} allows nulls")));
        }
    }
    let mut report = Report::default();
    if c.kind == Kind::Referential {
        let pdesc = file::describe(txn, new.parent)?;
        physical(&pdesc, ADD, &name)?;
        let pcsts = if new.parent == file { csts.clone() } else { list(txn, new.parent)? };
        let pk = pcsts.iter().find(|p| {
            p.kind.unique()
                && if new.parent_columns.is_empty() { p.kind == Kind::PrimaryKey } else { p.columns == new.parent_columns }
        });
        let Some(pk) = pk else {
            return Err(refuse(ADD, Refusal::KeyMismatch, &name, format!("file {} has no matching parent key", pdesc.name())));
        };
        if pk.columns.len() != c.columns.len() {
            return Err(refuse(ADD, Refusal::KeyMismatch, &name, "the foreign key and the parent key differ in length"));
        }
        let pfmt = &pdesc.formats[0];
        for (fc, pc) in c.columns.iter().zip(&pk.columns) {
            let (_, f) = fmt.format.field(fc).expect("checked");
            let (_, p) = pfmt.format.field(pc).ok_or_else(|| no_field(pc))?;
            if f.data_type != p.data_type || f.length != p.length || f.decimals != p.decimals || f.varlen != p.varlen {
                return Err(refuse(ADD, Refusal::KeyMismatch, &name, format!("foreign key field {fc} is not like parent key field {pc}")));
            }
            if new.delete_rule == Rule::SetNull && !f.allow_null {
                return Err(refuse(ADD, Refusal::SetNullNotNull, &name, format!("field {fc} cannot be null")));
            }
        }
        if new.delete_rule == Rule::SetNull && !c.columns.iter().any(|col| fmt.format.field(col).is_some_and(|(_, f)| f.allow_null)) {
            return Err(refuse(ADD, Refusal::SetNullNotNull, &name, "no foreign key field can be null"));
        }
        c.other = new.parent;
        c.other_name.clone_from(&pk.name);
        c.delete_rule = new.delete_rule;
        c.update_rule = new.update_rule;
        if pdesc.attr.journal.is_none() {
            report.messages.push(("CPI32B4", pdesc.name().to_string()));
        }
        if csts.iter().any(|x| x.kind == Kind::Referential && x.other == new.parent) {
            report.messages.insert(0, ("CPI32B5", String::new()));
        }
    }
    c.number = csts.iter().map(|x| x.number).max().map_or(1, |n| n + 1);
    csts.push(c.clone());
    if c.kind == Kind::Referential {
        let mut back = Constraint::new(&name, Kind::Dependent, Vec::new());
        back.other = file;
        back.other_name.clone_from(&name);
        back.number = 0;
        if new.parent == file {
            csts.push(back);
        } else {
            let mut pcsts = list(txn, new.parent)?;
            pcsts.push(back);
            store(txn, new.parent, &pcsts, now)?;
        }
    }
    store(txn, file, &csts, now)?;
    names::put(txn, lib, Space::Constraint, &name, Target { id: file, system: desc.name() })?;
    // The records as they are. A refusal takes the constraint out again
    // itself: a caller that committed parts (a statement that took turns)
    // keeps nothing of it.
    let broken = match rebuild_and_validate(txn, file, &name) {
        Ok(b) => b,
        Err(e) => {
            drop_one(txn, file, &name, now)?;
            return Err(e);
        }
    };
    if broken {
        if !new.pending_ok {
            let reason = match c.kind {
                Kind::Check => Refusal::CheckData,
                _ => Refusal::RefData,
            };
            drop_one(txn, file, &name, now)?;
            return Err(refuse(ADD, reason, &name, format!("records of {} break constraint {name}", desc.name())));
        }
        set_pending(txn, file, &name, true, now)?;
        report.messages.push(("CPD32C5", name.clone()));
        report.messages.push(("CPD32B7", desc.name().to_string()));
    }
    report.names.push(name);
    Ok(report)
}

fn no_field(n: &Name) -> DbError {
    DbError::Definition { what: format!("field {n} is not in the parent file") }
}

/// Rebuild every member's constraint tree; for constraint `name`, a
/// check or foreign key the records break returns true. A repeated
/// unique key fails with `DuplicateData`.
fn rebuild_and_validate(txn: &mut Txn<'_>, file: ObjectId, name: &str) -> Result<bool> {
    let desc = file::describe(txn, file)?;
    let mut broken = false;
    for e in &desc.members {
        let mut m = Member::open(txn, &desc, e)?;
        m.rebuild_cst_tree(txn).map_err(|err| match err {
            DbError::DuplicateKeys { key, .. } => refuse("CPF32B0", Refusal::DuplicateData, &key, format!("records repeat the key of constraint {key}")),
            e => e,
        })?;
        broken |= breaks(txn, &m, name)?;
    }
    Ok(broken)
}

/// Whether any record breaks check or referential constraint `name`.
fn breaks(txn: &mut Txn<'_>, m: &Member, name: &str) -> Result<bool> {
    let Some(a) = m.csts.clone() else { return Ok(false) };
    let Some(c) = a.csts.iter().find(|c| c.name == name) else { return Ok(false) };
    if !matches!(c.kind, Kind::Check | Kind::Referential) {
        return Ok(false);
    }
    let parent = if c.kind == Kind::Referential { Some(Member::open_by(txn, c.other, MemberSpec::First)?) } else { None };
    let mut rrn = 1;
    loop {
        let slots = m.scan(txn, rrn, 256)?;
        let Some(last) = slots.last() else { return Ok(false) };
        rrn = last.rrn + 1;
        for s in &slots {
            if c.kind == Kind::Check {
                let row = check::Row { format: m.format(), data: &s.record, nulls: &s.null_map };
                if !c.check.as_ref().is_none_or(|e| e.met(&row).unwrap_or(false)) {
                    return Ok(true);
                }
                continue;
            }
            let k = a.key_of(name).expect("keyed");
            if path::key_has_null(&k.format, &s.null_map) {
                continue;
            }
            let key = k.key(&s.record, &s.null_map)?;
            let p = parent.as_ref().expect("referential");
            let pa = p.csts.clone().ok_or_else(|| DbError::Corrupt { what: "parent without keys".into() })?;
            let pk = pa.key_of(&c.other_name).ok_or_else(|| DbError::Corrupt { what: "parent key gone".into() })?;
            let tree = p.cst_tree().expect("keyed");
            if pa.find(txn, &tree, pk, &key, 1)?.is_empty() {
                return Ok(true);
            }
        }
    }
}

fn set_pending(txn: &mut Txn<'_>, file: ObjectId, name: &str, pending: bool, now: u64) -> Result<()> {
    let mut csts = list(txn, file)?;
    for c in &mut csts {
        if c.name == name {
            c.check_pending = pending;
        }
    }
    store(txn, file, &csts, now)
}

/// Which constraints `RMVPFCST` and `CHGPFCST` act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    /// One by name, or every one (`*ALL`).
    pub name: Option<String>,
    /// `TYPE`: only these kinds, every kind when empty.
    pub kinds: Vec<Kind>,
}

impl Pick {
    fn takes(&self, c: &Constraint) -> bool {
        c.kind != Kind::Dependent
            && self.name.as_ref().is_none_or(|n| *n == c.name)
            && (self.kinds.is_empty() || self.kinds.contains(&c.kind))
    }
}

/// What `RMVPFCST` did: the constraints removed and those it could not
/// remove (`CPD32B1`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    /// Removed.
    pub removed: Vec<String>,
    /// Not removed: a parent key others depend on, or not found.
    pub refused: Vec<String>,
}

/// `RMVPFCST`, and SQL's `DROP CONSTRAINT`: with `cascade` a parent
/// key's dependent referential constraints go too.
pub fn remove(txn: &mut Txn<'_>, file: ObjectId, pick: &Pick, cascade: bool, now: u64) -> Result<Removed> {
    crate::auth::check(txn, "file.change", file)?;
    let desc = file::describe(txn, file)?;
    let csts = list(txn, file)?;
    let mut out = Removed::default();
    let chosen: Vec<Constraint> = csts.iter().filter(|c| pick.takes(c)).cloned().collect();
    if chosen.is_empty() {
        out.refused.push(pick.name.clone().unwrap_or_else(|| "*ALL".into()));
        return Ok(out);
    }
    for c in chosen {
        if c.kind.unique() {
            let deps = dependents_of(txn, file, &c.name)?;
            if !deps.is_empty() && !cascade {
                out.refused.push(c.name.clone());
                continue;
            }
            for (child, ri) in deps {
                drop_one(txn, child, &ri, now)?;
            }
        }
        drop_one(txn, file, &c.name, now)?;
        out.removed.push(c.name.clone());
    }
    let _ = desc;
    Ok(out)
}

/// The referential constraints `(file, name)` whose parent key is
/// constraint `pk` of `file`.
pub fn dependents_of(txn: &mut Txn<'_>, file: ObjectId, pk: &str) -> Result<Vec<(ObjectId, String)>> {
    let mut out = Vec::new();
    for d in list(txn, file)?.into_iter().filter(|c| c.kind == Kind::Dependent) {
        let child = if d.other == file { list(txn, file)? } else { list(txn, d.other).unwrap_or_default() };
        if child.iter().any(|c| c.kind == Kind::Referential && c.name == d.other_name && c.other_name == pk && c.other == file) {
            out.push((d.other, d.other_name.clone()));
        }
    }
    Ok(out)
}

/// Remove constraint `name` of `file`: its entry, a referential one's
/// entry on its parent, its name, and the trees rebuilt.
fn drop_one(txn: &mut Txn<'_>, file: ObjectId, name: &str, now: u64) -> Result<()> {
    let desc = file::describe(txn, file)?;
    let mut csts = list(txn, file)?;
    let Some(at) = csts.iter().position(|c| c.name == name && c.kind != Kind::Dependent) else {
        return Ok(());
    };
    let c = csts.remove(at);
    if c.kind == Kind::Referential {
        if c.other == file {
            csts.retain(|d| !(d.kind == Kind::Dependent && d.other == file && d.other_name == name));
        } else if let Ok(mut p) = list(txn, c.other) {
            p.retain(|d| !(d.kind == Kind::Dependent && d.other == file && d.other_name == name));
            store(txn, c.other, &p, now)?;
        }
    }
    store(txn, file, &csts, now)?;
    names::remove(txn, desc.header.library, Space::Constraint, name)?;
    let desc = file::describe(txn, file)?;
    for e in &desc.members {
        Member::open(txn, &desc, e)?.rebuild_cst_tree(txn)?;
    }
    Ok(())
}

/// What `CHGPFCST` did: changed constraints, and the ones left in check
/// pending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    /// Changed.
    pub changed: Vec<String>,
    /// Enabled but broken by the records: in check pending.
    pub pending: Vec<String>,
}

/// `CHGPFCST STATE(*ENABLED|*DISABLED)`: an enabled one is checked
/// against the records and left in check pending when they break it.
pub fn set_state(txn: &mut Txn<'_>, file: ObjectId, pick: &Pick, enabled: bool, now: u64) -> Result<Changed> {
    crate::auth::check(txn, "file.change", file)?;
    let mut csts = list(txn, file)?;
    let mut out = Changed::default();
    let names: Vec<String> = csts
        .iter()
        .filter(|c| pick.takes(c) && (pick.name.is_some() || matches!(c.kind, Kind::Check | Kind::Referential)))
        .map(|c| c.name.clone())
        .collect();
    if names.is_empty() {
        let n = pick.name.clone().unwrap_or_else(|| "*ALL".into());
        return Err(refuse("CPF32B2", Refusal::NotFound, &n, format!("constraint {n} not found")));
    }
    for c in &mut csts {
        if !names.contains(&c.name) {
            continue;
        }
        if c.kind.unique() {
            return Err(refuse("CPF32B2", Refusal::NotDisableable, &c.name, "a unique or primary key cannot change state"));
        }
        c.enabled = enabled;
        c.check_pending = false;
    }
    store(txn, file, &csts, now)?;
    if enabled {
        let desc = file::describe(txn, file)?;
        for n in &names {
            let mut broken = false;
            for e in &desc.members {
                let m = Member::open(txn, &desc, e)?;
                broken |= breaks(txn, &m, n)?;
            }
            if broken {
                set_pending(txn, file, n, true, now)?;
                out.pending.push(n.clone());
            }
        }
    }
    out.changed = names;
    Ok(out)
}

/// `DLTF` of a physical file: its referential constraints leave their
/// parents, the dependent files' referential constraints over it are
/// removed, its names leave the library's index.
pub fn on_delete_file(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId, now: u64) -> Result<()> {
    let _ = job;
    let csts = list(txn, file)?;
    if csts.is_empty() {
        return Ok(());
    }
    let desc = file::describe(txn, file)?;
    for c in &csts {
        match c.kind {
            Kind::Referential if c.other != file => {
                if let Ok(mut p) = list(txn, c.other) {
                    p.retain(|d| !(d.kind == Kind::Dependent && d.other == file && d.other_name == c.name));
                    store(txn, c.other, &p, now)?;
                }
            }
            Kind::Dependent if c.other != file => {
                drop_one(txn, c.other, &c.other_name, now)?;
            }
            _ => {}
        }
        if c.kind != Kind::Dependent {
            names::remove(txn, desc.header.library, Space::Constraint, &c.name)?;
        }
    }
    Ok(())
}

/// The constraints a column takes part in, by name.
pub fn using_column(txn: &mut Txn<'_>, file: ObjectId, column: Name) -> Result<Vec<String>> {
    Ok(list(txn, file)?
        .into_iter()
        .filter(|c| c.kind != Kind::Dependent && c.columns.contains(&column))
        .map(|c| c.name)
        .collect())
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPDBR`: what a file is based on and what depends on it.

use std::fmt::Write as _;

use qsname::{MemberSpec, Name};
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use super::{kv, library_name};
use crate::error::Result;
use crate::file::{self, FileKind};
use crate::member::Core;

/// A related file member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    /// Library.
    pub library: String,
    /// File.
    pub file: Name,
    /// Member.
    pub member: Name,
    /// Whether the dependent keeps an access path (else data only).
    pub access_path: bool,
}

/// The relations of a file, or of one of its members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbRelations {
    /// File.
    pub file: Name,
    /// Library.
    pub library: String,
    /// Kind.
    pub kind: FileKind,
    /// The member asked about, if one.
    pub member: Option<Name>,
    /// Logical file: the files it is based on.
    pub based_on: Vec<Relation>,
    /// Physical file: the logical members depending on it.
    pub dependents: Vec<Relation>,
}

/// Describe the relations; `member` narrows a physical file to one member.
pub fn dspdbr(
    txn: &mut Txn<'_>,
    file: ObjectId,
    member: Option<MemberSpec>,
) -> Result<DbRelations> {
    crate::auth::check(txn, "file.describe", file)?;
    let desc = file::describe(txn, file)?;
    let member = member
        .map(|m| crate::util::member_of(&desc, m))
        .transpose()?;
    let mut based_on = Vec::new();
    let mut dependents = Vec::new();
    if desc.attr.kind == FileKind::Logical {
        for e in &desc.members {
            if member.is_some_and(|m| m.object != e.object) {
                continue;
            }
            for base in Core::load(txn, e.object)?.base_members(txn)? {
                let core = Core::load(txn, base)?;
                let (_, h) = ops::read_header(txn, core.attr.file)?;
                let r = Relation {
                    library: library_name(txn, h.library),
                    file: h.name,
                    member: core.name,
                    access_path: false,
                };
                if !based_on.contains(&r) {
                    based_on.push(r);
                }
            }
        }
    } else {
        for d in &desc.dependents {
            if let Some(m) = &member
                && d.base_member != m.object
            {
                continue;
            }
            let lf = file::describe(txn, d.lf_file)?;
            let Some(e) = lf.members.iter().find(|e| e.object == d.lf_member) else {
                continue;
            };
            let r = Relation {
                library: library_name(txn, lf.header.library),
                file: lf.name(),
                member: e.name,
                access_path: lf.formats.iter().any(|f| f.key_len > 0),
            };
            if !dependents.contains(&r) {
                dependents.push(r);
            }
        }
    }
    Ok(DbRelations {
        file: desc.name(),
        library: library_name(txn, desc.header.library),
        kind: desc.attr.kind,
        member: member.map(|m| m.name),
        based_on,
        dependents,
    })
}

impl DbRelations {
    /// Render.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("Database relations\n");
        kv(&mut out, "File", self.file);
        kv(&mut out, "Library", &self.library);
        if let Some(m) = self.member {
            kv(&mut out, "Member", m);
        }
        if self.kind == FileKind::Logical {
            kv(&mut out, "Based-on files", self.based_on.len());
            for r in &self.based_on {
                let _ = writeln!(out, "  {}/{}  member {}", r.library, r.file, r.member);
            }
        } else {
            kv(&mut out, "Dependent files", self.dependents.len());
            for r in &self.dependents {
                let _ = writeln!(
                    out,
                    "  {}/{}  member {}  {}",
                    r.library,
                    r.file,
                    r.member,
                    if r.access_path { "access path" } else { "data" }
                );
            }
        }
        out
    }
}

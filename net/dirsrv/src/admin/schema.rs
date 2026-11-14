// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The schema's questions over a [`Schema`]: a page of definitions, a
//! definition added or removed (what the storage service and
//! [`super::MemAdmin`] share).

use ldap::msg::{Attribute, ModOp, Modification};

use super::{AdminError, ErrorKind, Result, SchemaDef, SchemaKind, SchemaPage, SchemaQuery};
use crate::schema::{Schema, SchemaError};

/// `GLD0167`: the schema was not changed, and why.
pub const NOT_CHANGED: &str = "GLD0167";

/// One definition as the pages show it.
pub fn def_of(s: &Schema, kind: SchemaKind, oid: &str) -> Option<SchemaDef> {
    let name_of = |id: &String| {
        s.attribute(id)
            .map_or_else(|| id.clone(), |a| a.name().to_string())
    };
    match kind {
        SchemaKind::Attribute => {
            let a = s.attribute(oid)?;
            Some(SchemaDef {
                kind,
                oid: a.oid().into(),
                names: a.def.names.clone(),
                desc: a.def.desc.clone().unwrap_or_default(),
                sup: a.def.sup.iter().cloned().collect(),
                syntax: match (&a.syntax, a.syntax_len) {
                    (Some(o), Some(n)) => format!("{o}{{{n}}}"),
                    (Some(o), None) => o.clone(),
                    _ => String::new(),
                },
                single_value: a.single_value(),
                access: a.access.keyword().into(),
                custom: s.is_modified(a.oid()),
                protected: s.is_protected(a.oid()),
                text: a.def.to_string(),
                ..SchemaDef::default()
            })
        }
        SchemaKind::Class => {
            let c = s.class(oid)?;
            Some(SchemaDef {
                kind,
                oid: c.oid().into(),
                names: c.def.names.clone(),
                desc: c.def.desc.clone().unwrap_or_default(),
                sup: c.def.sup.clone(),
                class_kind: match c.kind() {
                    ldap::schema::ClassKind::Abstract => "ABSTRACT",
                    ldap::schema::ClassKind::Structural => "STRUCTURAL",
                    ldap::schema::ClassKind::Auxiliary => "AUXILIARY",
                }
                .into(),
                must: c.must.iter().map(name_of).collect(),
                may: c.may.iter().map(name_of).collect(),
                custom: s.is_modified(c.oid()),
                protected: s.is_protected(c.oid()),
                text: c.def.to_string(),
                ..SchemaDef::default()
            })
        }
    }
}

fn schema_attr(kind: SchemaKind) -> &'static str {
    match kind {
        SchemaKind::Attribute => "attributetypes",
        SchemaKind::Class => "objectclasses",
    }
}

/// `GLD0167` for `e`.
#[must_use]
pub fn error(e: &SchemaError) -> AdminError {
    let kind = match e {
        SchemaError::Duplicate(_) => ErrorKind::Exists,
        SchemaError::NotFound(_) => ErrorKind::NotFound,
        _ => ErrorKind::Invalid,
    };
    AdminError {
        kind,
        msgid: NOT_CHANGED.into(),
        data: vec![e.to_string()],
        text: format!("The schema was not changed: {e}."),
    }
}

/// The definitions `q` asks for.
#[must_use]
pub fn page(s: &Schema, q: &SchemaQuery) -> SchemaPage {
    let needle = q.contains.trim().to_ascii_lowercase();
    let oids: Vec<String> = match q.kind {
        SchemaKind::Attribute => s.attributes().map(|a| a.oid().into()).collect(),
        SchemaKind::Class => s.classes().map(|c| c.oid().into()).collect(),
    };
    let mut all: Vec<SchemaDef> = oids
        .iter()
        .filter_map(|o| def_of(s, q.kind, o))
        .filter(|d| !q.custom_only || d.custom)
        .filter(|d| {
            needle.is_empty()
                || d.oid.contains(&needle)
                || d.names
                    .iter()
                    .any(|n| n.to_ascii_lowercase().contains(&needle))
        })
        .collect();
    let key = |d: &SchemaDef| {
        d.names
            .first()
            .map_or_else(|| d.oid.clone(), |n| n.to_ascii_lowercase())
    };
    all.sort_by_key(key);
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    let start = usize::try_from(q.start).unwrap_or(usize::MAX);
    let take = if q.max == 0 {
        usize::MAX
    } else {
        usize::try_from(q.max).unwrap_or(usize::MAX)
    };
    SchemaPage {
        total,
        defs: all.into_iter().skip(start).take(take).collect(),
    }
}

/// Add the definition `text` (its RFC 4512 description).
///
/// # Errors
/// `GLD0167` with the schema's reason.
pub fn add(s: &mut Schema, kind: SchemaKind, text: &str) -> Result<()> {
    let m = Modification {
        op: ModOp::Add,
        attr: Attribute {
            ty: schema_attr(kind).into(),
            vals: vec![text.trim().as_bytes().to_vec()],
        },
    };
    s.modify(&[m]).map_err(|e| error(&e))
}

/// Remove definition `id` (a name or OID).
///
/// # Errors
/// `GLD0167` when there is none, it is protected or in use.
pub fn remove(s: &mut Schema, kind: SchemaKind, id: &str) -> Result<()> {
    let oid = match kind {
        SchemaKind::Attribute => s.attribute(id.trim()).map(|a| a.oid().to_string()),
        SchemaKind::Class => s.class(id.trim()).map(|c| c.oid().to_string()),
    }
    .ok_or_else(|| error(&SchemaError::NotFound(id.trim().into())))?;
    if s.is_protected(&oid) {
        return Err(error(&SchemaError::Protected(id.trim().into())));
    }
    let m = Modification {
        op: ModOp::Delete,
        attr: Attribute {
            ty: schema_attr(kind).into(),
            vals: vec![format!("( {oid} )").into_bytes()],
        },
    };
    s.modify(&[m]).map_err(|e| error(&e))
}

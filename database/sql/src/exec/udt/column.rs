// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Columns of a distinct type: a table's or view's `COLTYPE` entries
//! (`docs/DB_FORMAT.md` §8.1, `SqlColType`), written as it is made and
//! read back onto its fields (`FieldDef::udt`) as a statement names it.

use std::borrow::Cow;

use db::sqlinf::var::ColType;
use db::sqlinf::{self, Kind, SqlInfo, tag};
use dds::FieldDef;
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::registry;
use crate::ast::ColumnDef;
use crate::error::Result;

/// The columns with their distinct types resolved (`typing::resolve`).
pub fn resolve(cols: &[ColumnDef]) -> Result<Cow<'_, [ColumnDef]>> {
    if cols.iter().all(|c| c.type_spec.udt.is_none()) {
        return Ok(Cow::Borrowed(cols));
    }
    let mut out = cols.to_vec();
    for c in &mut out {
        c.type_spec = super::typing::resolve(&c.type_spec)?.into_owned();
    }
    Ok(Cow::Owned(out))
}

/// Record that `file`'s fields named in `typed` are of the distinct types
/// (`SCHEMA.NAME`) given.
pub fn record(
    txn: &mut Txn<'_>,
    file: ObjectId,
    typed: &[(Name, String)],
    kind: Kind,
    now: u64,
) -> Result<()> {
    if typed.is_empty() {
        return Ok(());
    }
    let mut info = sqlinf::load(txn, file)?.unwrap_or_else(|| SqlInfo::new(kind));
    info.remove(tag::COLTYPE);
    for (field, q) in typed {
        let Some(d) = registry::by_qualified(q) else {
            continue;
        };
        info.push(
            tag::COLTYPE,
            ColType {
                field: *field,
                id: d.id,
                library: d.schema,
                name: d.name.clone(),
            }
            .encode(),
        );
    }
    sqlinf::save(txn, file, &info, now)?;
    Ok(())
}

/// The fields of a format that are of a distinct type, for [`record`].
#[must_use]
pub fn typed(fields: &[FieldDef]) -> Vec<(Name, String)> {
    fields
        .iter()
        .filter_map(|f| f.udt.clone().map(|u| (f.name, u)))
        .collect()
}

/// The `COLTYPE` entries of `file`.
pub fn of(txn: &mut Txn<'_>, file: ObjectId) -> Result<Vec<ColType>> {
    let Some(info) = sqlinf::load(txn, file)? else {
        return Ok(Vec::new());
    };
    info.all(tag::COLTYPE)
        .map(|b| ColType::decode(b).map_err(Into::into))
        .collect()
}

/// Mark `fields` (of `file`) with the distinct types of its columns.
pub fn mark(txn: &mut Txn<'_>, file: ObjectId, fields: &mut [FieldDef]) -> Result<()> {
    if !registry::any() {
        return Ok(());
    }
    for c in of(txn, file)? {
        if let Some(f) = fields.iter_mut().find(|f| f.name == c.field) {
            f.udt = Some(qualified(&c));
        }
    }
    Ok(())
}

/// `SCHEMA.NAME` of an entry.
#[must_use]
pub fn qualified(c: &ColType) -> String {
    format!("{}.{}", c.library.to_string().trim_end(), c.name)
}

/// A table or view with a column of type `id`, for `DROP TYPE`'s
/// `SQL0478`: its name.
pub fn user_of(txn: &mut Txn<'_>, qsys: ObjectId, id: ObjectId) -> Result<Option<String>> {
    for (_, lib) in ops::list(txn, qsys, Some(types::LIB))? {
        let Ok(files) = ops::list(txn, lib, Some(types::FILE)) else {
            continue;
        };
        for (k, file) in files {
            let Ok((_, h)) = ops::read_header(txn, file) else {
                continue;
            };
            if h.sql.is_none() {
                continue;
            }
            if of(txn, file)?.iter().any(|c| c.id == id) {
                return Ok(Some(k.name.to_string().trim_end().to_string()));
            }
        }
    }
    Ok(None)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSTABLES`: IBM's 7.1 columns in their order, a row per table,
//! view, physical and logical file (no index) and alias, as the primary
//! shows them (`tools/conformance/oracle/sql-catalog/`).

use db::file::{self, FileDesc, FileKind};
use dds::DataType;
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row};
use super::sys::{Meta, col, n, nclob, nv, s, stamp, type_of, vc};
use crate::error::Result;
use crate::value::Value;

/// The columns.
pub(super) fn columns(alias: &str) -> Vec<ColInfo> {
    let c = |nm: &str, len: u32, nullable: bool| col(alias, nm, DataType::Char, len, 0, nullable);
    let int = |nm: &str| col(alias, nm, DataType::Packed, 9, 0, false);
    let ts = |nm: &str, nullable: bool| col(alias, nm, DataType::Timestamp, 26, 0, nullable);
    vec![
        vc(alias, "TABLE_NAME", 128, false),
        vc(alias, "TABLE_OWNER", 128, false),
        c("TABLE_TYPE", 1, false),
        int("COLUMN_COUNT"),
        int("ROW_LENGTH"),
        nv(alias, "TABLE_TEXT", 50, false),
        nv(alias, "LONG_COMMENT", 2000, true),
        vc(alias, "TABLE_SCHEMA", 128, false),
        ts("LAST_ALTERED_TIMESTAMP", false),
        c("SYSTEM_TABLE_NAME", 10, false),
        c("SYSTEM_TABLE_SCHEMA", 10, false),
        c("FILE_TYPE", 1, false),
        vc(alias, "BASE_TABLE_CATALOG", 18, true),
        vc(alias, "BASE_TABLE_SCHEMA", 128, true),
        vc(alias, "BASE_TABLE_NAME", 128, true),
        vc(alias, "BASE_TABLE_MEMBER", 10, true),
        c("SYSTEM_TABLE", 1, false),
        c("SELECT_OMIT", 1, false),
        vc(alias, "IS_INSERTABLE_INTO", 3, false),
        col(alias, "IASP_NUMBER", DataType::Binary, 4, 0, false),
        vc(alias, "ENABLED", 3, true),
        vc(alias, "MAINTENANCE", 6, true),
        vc(alias, "REFRESH", 9, true),
        ts("REFRESH_TIME", true),
        nclob(alias, "MQT_DEFINITION", 13488),
        c("ISOLATION", 2, true),
        vc(alias, "PARTITION_TABLE", 11, false),
        vc(alias, "TABLE_DEFINER", 128, true),
        c("MQT_RESTORE_DEFERRED", 1, true),
        c("ROUNDING_MODE", 1, true),
    ]
}

/// Whether rows can be inserted through a view or logical file.
fn insertable(txn: &mut Txn<'_>, d: &FileDesc) -> bool {
    match crate::exec::view::definition(txn, d.id) {
        Some((v, _)) => !v.read_only,
        None => !d.formats.first().and_then(|f| f.logical.as_ref()).is_some_and(|l| l.join),
    }
}

/// The rows: the files in `all` but indexes, then the aliases.
pub(super) fn rows(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    all: &[(Name, FileDesc)],
    meta: &std::collections::HashMap<ObjectId, Meta>,
) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for (lib, d) in all {
        let m = meta.get(&d.id).cloned().unwrap_or_default();
        let kind = type_of(Some(&m), d);
        // IBM lists no index here.
        if kind == "I" {
            continue;
        }
        let logical = d.formats.first().and_then(|f| f.logical.as_ref());
        let select_omit = m.kind.is_none() && logical.is_some_and(|l| !l.select_omit.is_empty());
        let owner = ccsid::decode_trimmed(&d.header.owner);
        let insert = d.attr.kind != FileKind::Logical || insertable(txn, d);
        rows.push(row(&RowParts {
            sql: m.sql.clone(),
            owner: owner.clone(),
            kind,
            count: d.formats.first().map_or(0, |f| f.format.fields.len() as u64),
            len: u64::from(d.attr.record_len),
            text: ccsid::decode_trimmed(&d.header.text),
            comment: m.comment(None),
            schema: m.schema.clone(),
            altered: d.header.changed.max(d.header.created),
            system: d.name().to_string(),
            library: lib.to_string(),
            file_type: if d.attr.kind == FileKind::Source { "S" } else { "D" },
            base: None,
            select_omit,
            insertable: insert,
            definer: owner,
        }));
    }
    rows.extend(alias_rows(txn, ctx)?);
    Ok(rows)
}

/// What one row says.
struct RowParts {
    sql: String,
    owner: String,
    kind: &'static str,
    count: u64,
    len: u64,
    text: String,
    comment: Value,
    schema: String,
    altered: u64,
    system: String,
    library: String,
    file_type: &'static str,
    /// An alias's table: schema, name, member.
    base: Option<(String, String, String)>,
    select_omit: bool,
    insertable: bool,
    definer: String,
}

fn row(p: &RowParts) -> Row {
    let yn = |b: bool| s(if b { "Y" } else { "N" });
    vec![
        s(p.sql.clone()),
        s(p.owner.clone()),
        s(p.kind),
        n(p.count),
        n(p.len),
        s(p.text.clone()),
        p.comment.clone(),
        s(p.schema.clone()),
        stamp(p.altered),
        s(p.system.clone()),
        s(p.library.clone()),
        s(p.file_type),
        Value::Null,
        p.base.as_ref().map_or(Value::Null, |b| s(b.0.clone())),
        p.base.as_ref().map_or(Value::Null, |b| s(b.1.clone())),
        // The member as a name in ten characters, blanks kept.
        p.base.as_ref().map_or(Value::Null, |b| s(format!("{:<10}", b.2))),
        yn(false),
        yn(p.select_omit),
        s(if p.insertable { "YES" } else { "NO" }),
        n(0),
        Value::Null,
        Value::Null,
        Value::Null,
        Value::Null,
        Value::Null,
        Value::Null,
        s("NO"),
        s(p.definer.clone()),
        Value::Null,
        Value::Null,
    ]
}

/// The rows of the aliases (`A`): no columns and no row length, as IBM
/// lists them, and the table each names.
fn alias_rows(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let mut libraries: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    if !libraries.contains(&ctx.qsys) {
        libraries.insert(0, ctx.qsys);
    }
    for lib in libraries {
        let Ok((_, lh)) = ops::read_header(txn, lib) else {
            continue;
        };
        let Ok(list) = ops::list(txn, lib, Some(types::FILE)) else {
            continue;
        };
        for (_, id) in list {
            let Ok(Some(def)) = file::alias_of(txn, id) else {
                continue;
            };
            let (_, h) = ops::read_header(txn, id)?;
            let schema = crate::exec::names::schema_name(txn, lib);
            let info = db::sqlinf::load(txn, id).ok().flatten();
            let comment = crate::exec::label::comments_of(info.as_ref())
                .into_iter()
                .find(|c| c.column.is_none())
                .map_or(Value::Null, |c| s(c.text));
            // The table by its SQL name when it is there, else as the alias says.
            let target = crate::exec::alias::target(&def);
            let (bschema, bname) = match ctx.resolve_table(txn, &target) {
                Ok(t) => {
                    let tl = ops::read_header(txn, t.id).map_or(t.id, |(_, h)| h.library);
                    (crate::exec::names::schema_name(txn, tl), crate::exec::names::sql_name(txn, t.id))
                }
                Err(_) => (def.library.map_or_else(|| "*LIBL".into(), |l| l.to_string()), def.file.to_string()),
            };
            rows.push(row(&RowParts {
                sql: crate::exec::names::sql_name(txn, id),
                owner: ccsid::decode_trimmed(&h.owner),
                kind: "A",
                count: 0,
                len: 0,
                text: ccsid::decode_trimmed(&h.text),
                comment,
                schema,
                altered: h.changed.max(h.created),
                system: h.name.to_string(),
                library: lh.name.to_string(),
                file_type: "D",
                base: Some((bschema, bname, def.member.map_or_else(|| "*FIRST".into(), |m| m.to_string()))),
                select_omit: false,
                insertable: false,
                definer: String::new(),
            }));
        }
    }
    Ok(rows)
}

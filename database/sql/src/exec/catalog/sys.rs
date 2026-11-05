// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The first QSYS2 catalog views and `SYSIBM.SYSDUMMY1`, computed from
//! the store when a query names them. The column names are IBM's; the
//! rows come from the files and libraries as they are now.

use db::file::{self, FileDesc, FileKind};
use db::member::Core;
use dds::{DataType, FieldDef};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema, sql_type};
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::{Timestamp, Value};

/// The QSYS2 views here.
pub const TABLES: &[&str] = &[
    "SYSTABLES",
    "SYSCOLUMNS",
    "SYSINDEXES",
    "SYSKEYS",
    "SYSVIEWS",
    "SYSVIEWDEP",
    "SYSSCHEMAS",
    "SYSPARTITIONSTAT",
    "SYSTABLESTAT",
    "SYSCOLUMNSTAT",
];

/// A column of a catalog view.
fn col(alias: &str, name: &str, ty: DataType, len: u32, dec: u8, nullable: bool) -> ColInfo {
    // IBM's types: names are VARCHAR(128), counts INTEGER or BIGINT.
    let (ty, len) = match (ty, len, dec) {
        (DataType::Packed, 0..=9, 0) => (DataType::Binary, 9),
        (DataType::Packed, _, 0) => (DataType::Binary, 18),
        other => (other.0, other.1),
    };
    let mut f = FieldDef::new(
        Name::parse(&name[..name.len().min(10)])
            .unwrap_or_else(|_| Name::parse("COL").expect("valid")),
        ty,
        len,
        dec,
    );
    f.int_range = ty == DataType::Binary;
    if ty == DataType::Char && (len == 128 || len >= 1000) {
        f.varlen = Some(dds::VarLen { allocated: None });
    }
    f.allow_null = nullable;
    if name.len() > 10 {
        f.alias = Some(name.to_string());
    }
    ColInfo {
        name: name.to_string(),
        table: Some(alias.to_string()),
        field: Some(f),
        nullable,
        hidden: false,
    }
}

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

fn n(v: u64) -> Value {
    Value::int(i64::try_from(v).unwrap_or(i64::MAX))
}

fn stamp(secs: u64) -> Value {
    Value::Timestamp(Timestamp::new(
        db::record::datetime::Date::from_unix(secs),
        db::record::datetime::Time::from_unix(secs),
        0,
    ))
}

/// Every database file in every library, with its library name.
fn files(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<(Name, FileDesc)>> {
    // QSYS lists itself; every other library is in its directory.
    let mut libraries: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    if !libraries.contains(&ctx.qsys) {
        libraries.insert(0, ctx.qsys);
    }
    let mut out = Vec::new();
    for lib in libraries {
        let Ok((_, h)) = ops::read_header(txn, lib) else {
            continue;
        };
        // A library the user may not read is left out, as on the machine.
        let list = match ops::list(txn, lib, Some(types::FILE)) {
            Ok(l) => l,
            Err(store::StoreError::NotAuthorized { .. }) => continue,
            Err(e) => return Err(e.into()),
        };
        for (_, id) in list {
            if let Ok(desc) = file::describe(txn, id) {
                out.push((h.name, desc));
            }
        }
    }
    Ok(out)
}

/// The `SYSTABLES` rows of the aliases (`A`): their columns and row
/// length the table's they name, 0 when it is not there.
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
            let (count, len) = ctx
                .resolve_table(txn, &crate::exec::alias::target(&def))
                .map_or((0, 0), |t| (t.format.format.fields.len() as u64, u64::from(t.desc.attr.record_len)));
            let sql = crate::exec::names::sql_name(txn, id);
            let schema = crate::exec::names::schema_name(txn, lib);
            rows.push(vec![
                s(sql),
                s(ccsid::decode_trimmed(&h.owner)),
                s("A"),
                n(count),
                n(len),
                s(ccsid::decode_trimmed(&h.text)),
                s(schema),
                stamp(h.changed.max(h.created)),
                s(h.name.to_string()),
                s(lh.name.to_string()),
                s("D"),
            ]);
        }
    }
    Ok(rows)
}

/// What SQL knows of a file: its SQL name, its schema's, its kind.
#[derive(Clone, Debug, Default)]
pub(crate) struct Meta {
    /// The SQL name.
    pub sql: String,
    /// The schema's SQL name.
    pub schema: String,
    /// The kind SQL made it as.
    pub kind: Option<db::sqlinf::Kind>,
}

/// The SQL names and kinds of every file listed, by file.
pub(crate) fn metas(txn: &mut Txn<'_>, all: &[(Name, FileDesc)]) -> std::collections::HashMap<ObjectId, Meta> {
    let mut schemas: std::collections::HashMap<ObjectId, String> = std::collections::HashMap::new();
    let mut out = std::collections::HashMap::new();
    for (_, d) in all {
        let lib = d.header.library;
        let schema = if let Some(s) = schemas.get(&lib) {
            s.clone()
        } else {
            let s = crate::exec::names::schema_name(txn, lib);
            schemas.insert(lib, s.clone());
            s
        };
        let (sql, kind) = if d.header.sql.is_none() {
            (d.name().to_string(), None)
        } else {
            (crate::exec::names::sql_name(txn, d.id), crate::exec::names::kind_of(txn, d.id))
        };
        out.insert(d.id, Meta { sql, schema, kind });
    }
    out
}

/// How SQL sees a file, by its kind when SQL made it.
fn type_of(m: Option<&Meta>, desc: &FileDesc) -> &'static str {
    use db::sqlinf::Kind as K;
    match m.and_then(|m| m.kind) {
        Some(K::Table) => "T",
        Some(K::View) => "V",
        Some(K::Index) => "I",
        Some(K::Alias) => "A",
        Some(K::Mqt) => "M",
        _ => table_type(desc),
    }
}

/// How SQL sees a file made before SQL kept kinds: table (`T`), view
/// (`V`), index (`I`) by its text, else physical (`P`) or logical (`L`).
fn table_type(desc: &FileDesc) -> &'static str {
    let text = ccsid::decode_trimmed(&desc.header.text);
    match desc.attr.kind {
        FileKind::Physical | FileKind::Source if text.starts_with("SQL table") => "T",
        FileKind::Physical | FileKind::Source => "P",
        FileKind::Logical if text.starts_with("SQL view") => "V",
        FileKind::Logical if text.starts_with("SQL index") => "I",
        FileKind::Logical => "L",
    }
}

fn is_index(m: Option<&Meta>, desc: &FileDesc) -> bool {
    match type_of(m, desc) {
        "I" => true,
        "L" => desc.formats.iter().any(|f| f.key_len > 0),
        _ => false,
    }
}

/// Whether `name` is one of the views here (`QSYS2` or no schema).
#[must_use]
pub fn has(name: &ObjectName) -> bool {
    if name.name.value == "SYSDUMMY1" && name.schema.as_ref().is_none_or(|s| s.value == "SYSIBM") {
        return true;
    }
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2") && TABLES.contains(&name.name.value.as_str())
}

/// The rows and columns of a catalog table, or `None` when `name` is
/// not one. `QSYS2` or no schema names them.
#[allow(clippy::too_many_lines)]
pub fn virtual_table(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    alias: &str,
) -> Result<Option<(Schema, Vec<Row>)>> {
    // SYSIBM.SYSDUMMY1: one row, one column.
    if name.name.value == "SYSDUMMY1" && name.schema.as_ref().is_none_or(|s| s.value == "SYSIBM") {
        let cols = vec![col(alias, "IBMREQD", DataType::Char, 1, 0, false)];
        return Ok(Some((Schema { cols }, vec![vec![s("Y")]])));
    }
    if name.schema.as_ref().is_some_and(|s| s.value != "QSYS2") {
        return Ok(None);
    }
    let which = name.name.value.as_str();
    if !TABLES.contains(&which) {
        return Ok(None);
    }
    let all = files(txn, ctx)?;
    let meta = metas(txn, &all);
    let m = |d: &FileDesc| meta.get(&d.id).cloned().unwrap_or_default();
    let c = |nm: &str, ty: DataType, len: u32, dec: u8, nullable: bool| {
        col(alias, nm, ty, len, dec, nullable)
    };
    let (cols, rows): (Vec<ColInfo>, Vec<Row>) = match which {
        "SYSTABLES" => {
            let cols = vec![
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("TABLE_OWNER", DataType::Char, 10, 0, false),
                c("TABLE_TYPE", DataType::Char, 1, 0, false),
                c("COLUMN_COUNT", DataType::Packed, 9, 0, false),
                c("ROW_LENGTH", DataType::Packed, 9, 0, false),
                c("TABLE_TEXT", DataType::Char, 50, 0, true),
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("LAST_ALTERED_TIMESTAMP", DataType::Timestamp, 26, 0, false),
                c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, 0, false),
                c("FILE_TYPE", DataType::Char, 1, 0, false),
            ];
            let mut rows: Vec<Row> = all
                .iter()
                // IBM lists no index here.
                .filter(|(_, d)| type_of(meta.get(&d.id), d) != "I")
                .map(|(lib, d)| {
                    let md = m(d);
                    vec![
                        s(md.sql.clone()),
                        s(ccsid::decode_trimmed(&d.header.owner)),
                        s(type_of(Some(&md), d)),
                        n(d.formats
                            .first()
                            .map_or(0, |f| f.format.fields.len() as u64)),
                        n(u64::from(d.attr.record_len)),
                        s(ccsid::decode_trimmed(&d.header.text)),
                        s(md.schema.clone()),
                        stamp(d.header.changed.max(d.header.created)),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        s(if d.attr.kind == FileKind::Source {
                            "S"
                        } else {
                            "D"
                        }),
                    ]
                })
                .collect();
            rows.extend(alias_rows(txn, ctx)?);
            (cols, rows)
        }
        "SYSCOLUMNS" => {
            let cols = vec![
                c("COLUMN_NAME", DataType::Char, 128, 0, false),
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("TABLE_OWNER", DataType::Char, 10, 0, false),
                c("ORDINAL_POSITION", DataType::Packed, 9, 0, false),
                c("DATA_TYPE", DataType::Char, 8, 0, false),
                c("LENGTH", DataType::Packed, 9, 0, false),
                c("NUMERIC_SCALE", DataType::Packed, 9, 0, true),
                c("NUMERIC_PRECISION", DataType::Packed, 9, 0, true),
                c("IS_NULLABLE", DataType::Char, 1, 0, false),
                c("COLUMN_HEADING", DataType::Char, 60, 0, true),
                c("COLUMN_TEXT", DataType::Char, 50, 0, true),
                c("SYSTEM_COLUMN_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, false),
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("COLUMN_DEFAULT", DataType::Char, 100, 0, true),
                c("CCSID", DataType::Packed, 5, 0, true),
                c("HAS_DEFAULT", DataType::Char, 1, 0, false),
                c("USER_DEFINED_TYPE_SCHEMA", DataType::Char, 128, 0, true),
                c("USER_DEFINED_TYPE_NAME", DataType::Char, 128, 0, true),
            ];
            let mut rows = Vec::new();
            for (_, d) in &all {
                let Some(fmt) = d.formats.first() else {
                    continue;
                };
                // Columns of a distinct type.
                let typed = if crate::exec::udt::registry::any() { crate::exec::udt::column::of(txn, d.id)? } else { Vec::new() };
                for (i, f) in fmt.format.fields.iter().enumerate() {
                    let udt = typed.iter().find(|c| c.field == f.name);
                    let numeric = f.data_type.is_numeric();
                    let (length, scale, precision) = match f.data_type {
                        // A DECFLOAT's length is its bytes, its precision its digits.
                        DataType::DecFloat => (
                            n(u64::from(f.data_len())),
                            Value::Null,
                            n(if f.length <= 16 { 16 } else { 34 }),
                        ),
                        // A timestamp's scale is its precision.
                        DataType::Timestamp => (
                            n(u64::from(f.length)),
                            n(u64::from(DataType::timestamp_precision(f.length))),
                            Value::Null,
                        ),
                        // An SQL integer's length is its bytes, its precision
                        // its range's digits.
                        DataType::Binary if f.int_range => (
                            n(u64::from(f.data_len())),
                            n(u64::from(f.decimals)),
                            n(u64::from(f.digits())),
                        ),
                        _ if numeric => (n(u64::from(f.length)), n(u64::from(f.decimals)), n(u64::from(f.length))),
                        _ => (n(u64::from(f.length)), Value::Null, Value::Null),
                    };
                    rows.push(vec![
                        s(f.alias.clone().unwrap_or_else(|| f.name.to_string())),
                        s(m(d).sql),
                        s(ccsid::decode_trimmed(&d.header.owner)),
                        n(i as u64 + 1),
                        s(if udt.is_some() { "DISTINCT".into() } else { data_type_name(f) }),
                        length,
                        scale,
                        precision,
                        s(if f.allow_null { "Y" } else { "N" }),
                        if f.colhdg.is_empty() {
                            Value::Null
                        } else {
                            s(f.colhdg.join(" "))
                        },
                        f.text.clone().map_or(Value::Null, s),
                        s(f.name.to_string()),
                        s(d.name().to_string()),
                        s(m(d).schema),
                        f.default
                            .as_ref()
                            .map_or(Value::Null, |dft| s(default_text(dft))),
                        f.ccsid.or((f.data_type == DataType::Hex).then_some(65535)).map_or(Value::Null, |x| n(u64::from(x))),
                        s(if f.default.is_some() { "Y" } else { "N" }),
                        udt.map_or(Value::Null, |c| s(c.library.to_string().trim_end().to_string())),
                        udt.map_or(Value::Null, |c| s(c.name.clone())),
                    ]);
                }
            }
            (cols, rows)
        }
        "SYSINDEXES" => {
            let cols = vec![
                c("INDEX_NAME", DataType::Char, 128, 0, false),
                c("INDEX_OWNER", DataType::Char, 10, 0, false),
                c("INDEX_SCHEMA", DataType::Char, 128, 0, false),
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("IS_UNIQUE", DataType::Char, 1, 0, false),
                c("COLUMN_COUNT", DataType::Packed, 9, 0, false),
                c("SYSTEM_INDEX_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_INDEX_SCHEMA", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, 0, false),
                c("INDEX_TEXT", DataType::Char, 50, 0, true),
                c("INDEX_HAS_SEARCH_CONDITION", DataType::Char, 1, 0, false),
                c("SEARCH_CONDITION", DataType::Char, 2000, 0, true),
                c("INDEX_HAS_INCLUDE_EXPRESSION", DataType::Char, 1, 0, false),
                c("INCLUDE_EXPRESSION", DataType::Char, 2000, 0, true),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                let md = m(d);
                if !is_index(Some(&md), d) {
                    continue;
                }
                let fmt = &d.formats[0];
                let (tlib, tname, tid) = base_of(txn, fmt);
                let (tsql, tschema) = match tid {
                    Some(id) => {
                        let tl = ops::read_header(txn, id).map_or(id, |(_, h)| h.library);
                        (crate::exec::names::sql_name(txn, id), crate::exec::names::schema_name(txn, tl))
                    }
                    None => (tname.clone(), tlib.clone()),
                };
                let def = crate::exec::index::definition(txn, d.id);
                let search = def.as_ref().map(|x| x.where_text.clone()).filter(|w| !w.is_empty());
                let include = def.as_ref().filter(|x| !x.includes.is_empty()).map(|x| x.includes.join(", "));
                rows.push(vec![
                    s(md.sql.clone()),
                    s(ccsid::decode_trimmed(&d.header.owner)),
                    s(md.schema.clone()),
                    s(tsql),
                    s(tschema),
                    s(match (fmt.unique, def.as_ref().is_some_and(|x| x.encoded_vector)) {
                        (_, true) => "E",
                        (None, _) => "D",
                        (Some(dds::NullKeys::Exclude), _) => "V",
                        (Some(_), _) => "U",
                    }),
                    n(fmt.format.keys.len() as u64),
                    s(d.name().to_string()),
                    s(lib.to_string()),
                    s(tname),
                    s(tlib),
                    s(ccsid::decode_trimmed(&d.header.text)),
                    s(if search.is_some() { "Y" } else { "N" }),
                    search.map_or(Value::Null, s),
                    s(if include.is_some() { "Y" } else { "N" }),
                    include.map_or(Value::Null, s),
                ]);
            }
            (cols, rows)
        }
        "SYSKEYS" => {
            let cols = vec![
                c("INDEX_NAME", DataType::Char, 128, 0, false),
                c("INDEX_SCHEMA", DataType::Char, 128, 0, false),
                c("COLUMN_NAME", DataType::Char, 128, 0, false),
                c("COLUMN_POSITION", DataType::Packed, 9, 0, false),
                c("ORDINAL_POSITION", DataType::Packed, 9, 0, false),
                c("ORDERING", DataType::Char, 1, 0, false),
                c("SYSTEM_INDEX_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_INDEX_SCHEMA", DataType::Char, 10, 0, false),
                c("SYSTEM_COLUMN_NAME", DataType::Char, 10, 0, false),
                c("COLUMN_IS_EXPRESSION", DataType::Char, 1, 0, false),
                c("KEY_EXPRESSION", DataType::Char, 2000, 0, true),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                let md = m(d);
                if !is_index(Some(&md), d) {
                    continue;
                }
                let fmt = &d.formats[0].format;
                for (i, k) in fmt.keys.iter().enumerate() {
                    let sql = fmt.fields.get(k.field).and_then(|f| f.alias.clone()).unwrap_or_else(|| k.name.to_string());
                    rows.push(vec![
                        s(md.sql.clone()),
                        s(md.schema.clone()),
                        s(sql.clone()),
                        n(k.field as u64 + 1),
                        n(i as u64 + 1),
                        s(if k.descend { "D" } else { "A" }),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        s(k.name.to_string()),
                        s("N"),
                        s(sql),
                    ]);
                }
            }
            (cols, rows)
        }
        "SYSVIEWS" => {
            let cols = vec![
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("VIEW_OWNER", DataType::Char, 128, 0, false),
                c("SEQNO", DataType::Binary, 9, 0, false),
                c("CHECK_OPTION", DataType::Char, 1, 0, false),
                c("VIEW_DEFINITION", DataType::Char, 5000, 0, true),
                c("IS_UPDATABLE", DataType::Char, 1, 0, false),
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("SYSTEM_VIEW_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_VIEW_SCHEMA", DataType::Char, 10, 0, false),
                c("IS_INSERTABLE_INTO", DataType::Char, 3, 0, false),
                c("IASP_NUMBER", DataType::Binary, 4, 0, false),
                c("IS_DELETABLE", DataType::Char, 1, 0, true),
                c("VIEW_DEFINER", DataType::Char, 128, 0, true),
                c("ROUNDING_MODE", DataType::Char, 1, 0, true),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                let md = m(d);
                if type_of(Some(&md), d) != "V" {
                    continue;
                }
                let def = crate::exec::view::definition(txn, d.id).map(|(v, _)| v);
                let updatable = match &def {
                    Some(v) => !v.read_only,
                    None => !d.formats[0].logical.as_ref().is_some_and(|l| l.join),
                };
                let owner = ccsid::decode_trimmed(&d.header.owner);
                rows.push(vec![
                    s(md.sql.clone()),
                    s(owner.clone()),
                    n(1),
                    s(match def.as_ref().map(|v| v.check) {
                        Some(db::sqlinf::body::CheckOption::Cascaded) => "C",
                        Some(db::sqlinf::body::CheckOption::Local) => "Y",
                        _ => "N",
                    }),
                    def.as_ref().map_or(Value::Null, |v| s(v.query.clone())),
                    s(if updatable { "Y" } else { "N" }),
                    s(md.schema.clone()),
                    s(d.name().to_string()),
                    s(lib.to_string()),
                    s(if updatable { "YES" } else { "NO" }),
                    n(0),
                    s(if updatable { "Y" } else { "N" }),
                    s(owner),
                    Value::Null,
                ]);
            }
            (cols, rows)
        }
        "SYSVIEWDEP" => {
            let cols = vec![
                c("VIEW_NAME", DataType::Char, 128, 0, false),
                c("VIEW_OWNER", DataType::Char, 128, 0, false),
                c("OBJECT_NAME", DataType::Char, 128, 0, false),
                c("OBJECT_SCHEMA", DataType::Char, 128, 0, false),
                c("OBJECT_TYPE", DataType::Char, 24, 0, false),
                c("VIEW_SCHEMA", DataType::Char, 128, 0, false),
                c("SYSTEM_VIEW_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_VIEW_SCHEMA", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, true),
                c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, 0, true),
                c("TABLE_NAME", DataType::Char, 128, 0, true),
                c("TABLE_OWNER", DataType::Char, 128, 0, true),
                c("TABLE_SCHEMA", DataType::Char, 128, 0, true),
                c("TABLE_TYPE", DataType::Char, 1, 0, true),
                c("IASP_NUMBER", DataType::Binary, 4, 0, false),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                let md = m(d);
                if type_of(Some(&md), d) != "V" {
                    continue;
                }
                // What it depends on: its DEPENDENCY entries, else the
                // files a logical view is based on.
                let mut on: Vec<ObjectId> = Vec::new();
                if let Some((_, deps)) = crate::exec::view::definition(txn, d.id) {
                    for dep in deps {
                        if let Some(id) = crate::exec::view::dependency_object(txn, ctx, &dep) {
                            on.push(id);
                        }
                    }
                }
                if on.is_empty() {
                    for fmt in &d.formats {
                        if let Some(l) = &fmt.logical {
                            on.extend(l.based_on.iter().map(|(id, _)| *id));
                        }
                    }
                }
                on.dedup();
                let owner = ccsid::decode_trimmed(&d.header.owner);
                for id in on {
                    let (blib, bname) = name_of(txn, id);
                    let (obj_sql, obj_schema, obj_type, obj_owner) = match all.iter().find(|(_, f)| f.id == id) {
                        Some((_, f)) => {
                            let om = m(f);
                            let t = type_of(Some(&om), f);
                            (om.sql, om.schema, t, ccsid::decode_trimmed(&f.header.owner))
                        }
                        None => (bname.clone(), blib.clone(), "T", String::new()),
                    };
                    rows.push(vec![
                        s(md.sql.clone()),
                        s(owner.clone()),
                        s(obj_sql.clone()),
                        s(obj_schema.clone()),
                        s(if obj_type == "V" { "VIEW" } else { "TABLE" }),
                        s(md.schema.clone()),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        s(bname),
                        s(blib),
                        s(obj_sql),
                        s(obj_owner),
                        s(obj_schema),
                        s(obj_type),
                        n(0),
                    ]);
                }
            }
            (cols, rows)
        }
        "SYSSCHEMAS" => {
            let cols = vec![
                c("SCHEMA_NAME", DataType::Char, 128, 0, false),
                c("SCHEMA_OWNER", DataType::Char, 10, 0, false),
                c("SYSTEM_SCHEMA_NAME", DataType::Char, 10, 0, false),
                c("SCHEMA_TEXT", DataType::Char, 50, 0, true),
                c("SCHEMA_CREATOR", DataType::Char, 10, 0, false),
            ];
            let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?
                .into_iter()
                .map(|(_, id)| id)
                .collect();
            if !libs.contains(&ctx.qsys) {
                libs.insert(0, ctx.qsys);
            }
            let mut rows = Vec::new();
            for id in libs {
                let Ok((_, h)) = ops::read_header(txn, id) else {
                    continue;
                };
                rows.push(vec![
                    s(crate::exec::names::schema_name(txn, id)),
                    s(ccsid::decode_trimmed(&h.owner)),
                    s(h.name.to_string()),
                    s(ccsid::decode_trimmed(&h.text)),
                    s(ccsid::decode_trimmed(&h.creator)),
                ]);
            }
            (cols, rows)
        }
        "SYSPARTITIONSTAT" | "SYSTABLESTAT" => {
            let per_member = which == "SYSPARTITIONSTAT";
            let mut cols = vec![];
            if per_member {
                cols.push(c("TABLE_PARTITION", DataType::Char, 128, 0, false));
            }
            cols.extend([
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("NUMBER_ROWS", DataType::Packed, 20, 0, false),
                c("NUMBER_DELETED_ROWS", DataType::Packed, 20, 0, false),
                c("DATA_SIZE", DataType::Packed, 20, 0, false),
            ]);
            if !per_member {
                cols.push(c("NUMBER_PARTITIONS", DataType::Packed, 9, 0, false));
            }
            let mut rows = Vec::new();
            for (lib, d) in &all {
                if d.attr.kind == FileKind::Logical {
                    continue;
                }
                let (mut rcount, mut dcount, mut size) = (0u64, 0u64, 0u64);
                for m in &d.members {
                    let Ok(core) = Core::load(txn, m.object) else {
                        continue;
                    };
                    let bytes = core.attr.slots * u64::from(core.attr.slot_len);
                    if per_member {
                        rows.push(vec![
                            s(m.name.to_string()),
                            s(lib.to_string()),
                            s(d.name().to_string()),
                            n(core.attr.record_count),
                            n(core.attr.deleted_count),
                            n(bytes),
                        ]);
                    }
                    rcount += core.attr.record_count;
                    dcount += core.attr.deleted_count;
                    size += bytes;
                }
                if !per_member {
                    rows.push(vec![
                        s(lib.to_string()),
                        s(d.name().to_string()),
                        n(rcount),
                        n(dcount),
                        n(size),
                        n(d.members.len() as u64),
                    ]);
                }
            }
            (cols, rows)
        }
        "SYSCOLUMNSTAT" => {
            // IBM's columns, a row per column of each member: a column
            // with statistics collected (`plan::stats`, a single-column
            // statistic each) shows them; the others are null where IBM
            // lists no row.
            let int = |nm: &str, digits: u32, nullable: bool| c(nm, DataType::Binary, digits, 0, nullable);
            let mut dbl = c("AVGDISTINCTPERPAGE", DataType::Float, 8, 0, true);
            if let Some(f) = dbl.field.as_mut() {
                f.precision = dds::FloatPrecision::Double;
            }
            let mut ratio = dbl.clone();
            ratio.name = "PAGEVARIANCERATIO".into();
            let mut id = c("INTERNAL_STATISTICS_ID", DataType::Bytes, 16, 0, true);
            if let Some(f) = id.field.as_mut() {
                f.varlen = Some(dds::VarLen { allocated: None });
            }
            let cols = vec![
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("TABLE_PARTITION", DataType::Char, 128, 0, false),
                c("PARTITION_TYPE", DataType::Char, 1, 0, false),
                int("PARTITION_NUMBER", 9, true),
                int("NUMBER_DISTRIBUTED_PARTITIONS", 9, true),
                int("NUMBER_COLUMN_NAMES", 9, false),
                c("COLUMN_NAME", DataType::Char, 1280, 0, false),
                int("NUMBER_DISTINCT_VALUES", 18, true),
                c("HIGH2KEY", DataType::Char, 1000, 0, true),
                c("LOW2KEY", DataType::Char, 1000, 0, true),
                int("AVERAGE_COLUMN_LENGTH", 9, true),
                int("MAXIMUM_COLUMN_LENGTH", 9, true),
                int("LENGTH_AT_90TH_PERCENTILE", 9, true),
                int("OVERFLOW_ROWS", 18, true),
                int("NUMBER_NULLS", 18, true),
                int("SUB_COUNT", 4, true),
                int("SUB_DELIM_LENGTH", 4, true),
                int("NUMBER_HISTOGRAM_RANGES", 9, true),
                int("NUMBER_MOST_FREQUENT_VALUES", 9, true),
                dbl,
                ratio,
                c("STATISTICS_NAME", DataType::Char, 128, 0, true),
                id,
                c("STATISTIC_CREATED", DataType::Timestamp, 26, 0, true),
                c("STATISTIC_CREATOR", DataType::Char, 128, 0, true),
                c("STATISTIC_LAST_UPDATED", DataType::Timestamp, 26, 0, true),
                c("STATISTIC_UPDATER", DataType::Char, 128, 0, true),
                int("STATISTICS_SIZE", 18, true),
                c("AGING_MODE", DataType::Char, 128, 0, true),
                c("AGING_STATUS", DataType::Char, 1, 0, true),
                c("BLOCK_OPTION", DataType::Char, 1, 0, true),
                c("CURRENT_LAST_CHANGE", DataType::Timestamp, 26, 0, true),
                int("CURRENT_ROWS", 18, true),
                int("CURRENT_DELETED_ROWS", 18, true),
                int("CURRENT_DATA_CHANGES", 18, true),
                int("STATISTICS_ROWS", 18, true),
                int("STATISTICS_DELETED_ROWS", 18, true),
                int("STATISTICS_DATA_CHANGES", 18, true),
                c("TRANSLATION_ATTRIBUTES", DataType::Char, 128, 0, true),
                c("TRANSLATION_TABLES", DataType::Char, 1000, 0, true),
                c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_TABLE_MEMBER", DataType::Char, 10, 0, false),
                c("SYSTEM_COLUMN_NAME", DataType::Char, 1000, 0, false),
            ];
            let mut rows = Vec::new();
            let i = |v: u64| Value::int(i64::try_from(v).unwrap_or(i64::MAX));
            for (lib, d) in &all {
                if d.attr.kind == FileKind::Logical {
                    continue;
                }
                let Some(fmt) = d.formats.first() else {
                    continue;
                };
                let tl = ops::read_header(txn, d.id).map_or(d.id, |(_, h)| h.library);
                let schema = crate::exec::names::schema_name(txn, tl);
                let table = crate::exec::names::sql_name(txn, d.id);
                crate::exec::plan::stats::prime(txn, d.id);
                for m in &d.members {
                    let now = db::member::Member::open(txn, d, m).ok().map(|x| x.attr().clone());
                    let (rc, dc, sl) = now.as_ref().map_or((0, 0, 0), |a| (a.record_count, a.deleted_count, a.slots));
                    for f in &fmt.format.fields {
                        let column = f.alias.clone().unwrap_or_else(|| f.name.to_string());
                        let st = crate::exec::plan::stats::get(m.object, &f.name.to_string());
                        let g = |pick: &dyn Fn(&crate::exec::plan::stats::ColumnStats) -> Value| st.as_ref().map_or(Value::Null, pick);
                        let changes = st.as_ref().map(|st| rc.abs_diff(st.rows) + dc.abs_diff(st.deleted) + sl.abs_diff(st.slots));
                        rows.push(vec![
                            s(schema.clone()),
                            s(table.clone()),
                            s(m.name.to_string()),
                            s(" "),
                            Value::int(1),
                            Value::Null,
                            Value::int(1),
                            s(column),
                            g(&|st| i(st.distinct)),
                            g(&|_| s("")),
                            g(&|_| s("")),
                            g(&|st| i(u64::from(st.avg_len))),
                            g(&|st| i(u64::from(st.max_len))),
                            Value::Null,
                            Value::Null,
                            g(&|st| i(st.nulls)),
                            g(&|_| Value::int(-1)),
                            g(&|_| Value::int(-1)),
                            g(&|st| i(st.bounds.len() as u64)),
                            g(&|st| i(st.frequent.len() as u64)),
                            Value::Null,
                            Value::Null,
                            g(&|st| s(st.name(m.object))),
                            g(&|st| Value::Bin(st.internal_id(m.object).to_vec())),
                            g(&|st| stamp(st.created)),
                            g(&|_| s("*SYS")),
                            g(&|st| stamp(st.updated)),
                            g(&|_| s("*SYS")),
                            g(&|_| i(4096)),
                            g(&|_| s("*SYS")),
                            g(&|_| s("1")),
                            g(&|_| s("0")),
                            Value::Null,
                            i(rc),
                            i(dc),
                            changes.map_or(Value::Null, i),
                            g(&|st| i(st.rows)),
                            g(&|st| i(st.deleted)),
                            g(&|_| Value::int(0)),
                            Value::Null,
                            g(&|_| s("")),
                            s(lib.to_string()),
                            s(d.name().to_string()),
                            s(m.name.to_string()),
                            s(f.name.to_string()),
                        ]);
                    }
                }
            }
            (cols, rows)
        }
        _ => return Ok(None),
    };
    Ok(Some((Schema { cols: cols.clone() }, pad_fixed(&cols, rows))))
}

/// The SQL type name of a field, as `SYSCOLUMNS.DATA_TYPE` spells it.
fn data_type_name(f: &FieldDef) -> String {
    let t = sql_type(f);
    let t = t.split('(').next().unwrap_or(&t).trim();
    // IBM's eight-character spellings.
    match t {
        "VARBINARY" => "VARBIN".into(),
        "TIMESTAMP" => "TIMESTMP".into(),
        t => t.to_string(),
    }
}

/// The library and name of a file by ID.
fn name_of(txn: &mut Txn<'_>, id: ObjectId) -> (String, String) {
    match ops::read_header(txn, id) {
        Ok((_, h)) => {
            let lib = ops::read_header(txn, h.library)
                .map_or_else(|_| String::new(), |(_, l)| l.name.to_string());
            (lib, h.name.to_string())
        }
        Err(_) => (String::new(), id.to_string()),
    }
}

/// The first based-on file of a logical format.
fn base_of(txn: &mut Txn<'_>, fmt: &db::format::FormatDef) -> (String, String, Option<ObjectId>) {
    match fmt.logical.as_ref().and_then(|l| l.based_on.first()) {
        Some((id, _)) => {
            let (l, n) = name_of(txn, *id);
            (l, n, Some(*id))
        }
        None => (String::new(), String::new(), None),
    }
}

/// A fixed-length character column's values padded with blanks to its
/// length, as IBM's catalog returns them.
fn pad_fixed(cols: &[ColInfo], mut rows: Vec<Row>) -> Vec<Row> {
    let widths: Vec<Option<usize>> = cols
        .iter()
        .map(|c| c.field.as_ref().filter(|f| f.data_type == DataType::Char && f.varlen.is_none()).map(|f| f.length as usize))
        .collect();
    for r in &mut rows {
        for (v, w) in r.iter_mut().zip(&widths) {
            if let (Value::Str(s), Some(w)) = (v, w) {
                let n = s.chars().count();
                if n < *w && *w <= 50 {
                    s.extend(std::iter::repeat_n(' ', w - n));
                }
            }
        }
    }
    rows
}

/// A column default as `SYSCOLUMNS.COLUMN_DEFAULT` writes it: the SQL
/// text of the value.
pub(crate) fn default_text(d: &dds::DefaultValue) -> String {
    use dds::DefaultValue as D;
    match d {
        D::Null => "NULL".into(),
        D::Quoted(t) => format!("'{}'", t.replace('\'', "''")),
        D::Hex(b) => format!("X'{}'", crate::value::hex(b)),
        D::Number(n) => n.clone(),
        D::Expr(e) => e.clone(),
    }
}

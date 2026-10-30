// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSYS2 catalog views, computed from the store when a query names
//! them. The column names are IBM's; the rows come from the files and
//! libraries as they are now.

use db::file::{self, FileDesc, FileKind};
use db::member::Core;
use dds::{DataType, FieldDef};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::SqlContext;
use super::eval::{ColInfo, Row, Schema, sql_type};
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::{Timestamp, Value};

/// The catalog tables.
pub const TABLES: &[&str] = &[
    "SYSTABLES",
    "SYSCOLUMNS",
    "SYSINDEXES",
    "SYSKEYS",
    "SYSVIEWS",
    "SYSVIEWDEP",
    "SYSCST",
    "SYSCSTCOL",
    "SYSKEYCST",
    "SYSREFCST",
    "SYSSCHEMAS",
    "SYSPARTITIONSTAT",
    "SYSTABLESTAT",
    "SYSCOLUMNSTAT",
];

/// A column of a catalog view.
fn col(alias: &str, name: &str, ty: DataType, len: u32, dec: u8, nullable: bool) -> ColInfo {
    let mut f = FieldDef::new(
        Name::parse(&name[..name.len().min(10)])
            .unwrap_or_else(|_| Name::parse("COL").expect("valid")),
        ty,
        len,
        dec,
    );
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
    Value::Timestamp(Timestamp {
        date: db::record::datetime::Date::from_unix(secs),
        time: db::record::datetime::Time::from_unix(secs),
        micros: 0,
    })
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
        for (_, id) in ops::list(txn, lib, Some(types::FILE))? {
            if let Ok(desc) = file::describe(txn, id) {
                out.push((h.name, desc));
            }
        }
    }
    Ok(out)
}

/// How SQL sees a file: table (`T`), view (`V`), index (`I`), logical (`L`).
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

fn is_index(desc: &FileDesc) -> bool {
    desc.attr.kind == FileKind::Logical
        && desc.formats.iter().any(|f| f.key_len > 0)
        && table_type(desc) != "V"
}

/// Whether `name` names a catalog view (`QSYS2` or no schema).
#[must_use]
pub fn is_view(name: &ObjectName) -> bool {
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
    let c = |nm: &str, ty: DataType, len: u32, dec: u8, nullable: bool| {
        col(alias, nm, ty, len, dec, nullable)
    };
    let (cols, rows): (Vec<ColInfo>, Vec<Row>) = match which {
        "SYSTABLES" => (
            vec![
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
            ],
            all.iter()
                .map(|(lib, d)| {
                    vec![
                        s(d.name().to_string()),
                        s(ccsid::decode_trimmed(&d.header.owner)),
                        s(table_type(d)),
                        n(d.formats
                            .first()
                            .map_or(0, |f| f.format.fields.len() as u64)),
                        n(u64::from(d.attr.record_len)),
                        s(ccsid::decode_trimmed(&d.header.text)),
                        s(lib.to_string()),
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
                .collect(),
        ),
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
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                let Some(fmt) = d.formats.first() else {
                    continue;
                };
                for (i, f) in fmt.format.fields.iter().enumerate() {
                    let numeric = f.data_type.is_numeric();
                    rows.push(vec![
                        s(f.alias.clone().unwrap_or_else(|| f.name.to_string())),
                        s(d.name().to_string()),
                        s(ccsid::decode_trimmed(&d.header.owner)),
                        n(i as u64 + 1),
                        s(data_type_name(f)),
                        n(u64::from(f.length)),
                        if numeric {
                            n(u64::from(f.decimals))
                        } else {
                            Value::Null
                        },
                        if numeric {
                            n(u64::from(f.length))
                        } else {
                            Value::Null
                        },
                        s(if f.allow_null { "Y" } else { "N" }),
                        if f.colhdg.is_empty() {
                            Value::Null
                        } else {
                            s(f.colhdg.join(" "))
                        },
                        f.text.clone().map_or(Value::Null, s),
                        s(f.name.to_string()),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        f.default
                            .as_ref()
                            .map_or(Value::Null, |dft| s(format!("{dft:?}"))),
                        f.ccsid.map_or(Value::Null, |x| n(u64::from(x))),
                        s(if f.default.is_some() { "Y" } else { "N" }),
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
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                if !is_index(d) {
                    continue;
                }
                let fmt = &d.formats[0];
                let (tlib, tname) = base_of(txn, fmt);
                rows.push(vec![
                    s(d.name().to_string()),
                    s(ccsid::decode_trimmed(&d.header.owner)),
                    s(lib.to_string()),
                    s(tname.clone()),
                    s(tlib.clone()),
                    s(match fmt.unique {
                        None => "D",
                        Some(dds::NullKeys::Exclude) => "V",
                        Some(_) => "U",
                    }),
                    n(fmt.format.keys.len() as u64),
                    s(d.name().to_string()),
                    s(lib.to_string()),
                    s(tname),
                    s(tlib),
                    s(ccsid::decode_trimmed(&d.header.text)),
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
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                if !is_index(d) {
                    continue;
                }
                for (i, k) in d.formats[0].format.keys.iter().enumerate() {
                    rows.push(vec![
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        s(k.name.to_string()),
                        n(k.field as u64 + 1),
                        n(i as u64 + 1),
                        s(if k.descend { "D" } else { "A" }),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                    ]);
                }
            }
            (cols, rows)
        }
        "SYSVIEWS" => {
            let cols = vec![
                c("VIEW_NAME", DataType::Char, 128, 0, false),
                c("VIEW_SCHEMA", DataType::Char, 128, 0, false),
                c("VIEW_DEFINITION", DataType::Char, 2000, 0, true),
                c("CHECK_OPTION", DataType::Char, 8, 0, false),
                c("IS_UPDATABLE", DataType::Char, 1, 0, false),
                c("SYSTEM_VIEW_NAME", DataType::Char, 10, 0, false),
                c("SYSTEM_VIEW_SCHEMA", DataType::Char, 10, 0, false),
            ];
            let rows = all
                .iter()
                .filter(|(_, d)| table_type(d) == "V")
                .map(|(lib, d)| {
                    vec![
                        s(d.name().to_string()),
                        s(lib.to_string()),
                        Value::Null,
                        s("NONE"),
                        s(if d.formats[0].logical.as_ref().is_some_and(|l| l.join) {
                            "N"
                        } else {
                            "Y"
                        }),
                        s(d.name().to_string()),
                        s(lib.to_string()),
                    ]
                })
                .collect();
            (cols, rows)
        }
        "SYSVIEWDEP" => {
            let cols = vec![
                c("VIEW_NAME", DataType::Char, 128, 0, false),
                c("VIEW_SCHEMA", DataType::Char, 128, 0, false),
                c("OBJECT_NAME", DataType::Char, 128, 0, false),
                c("OBJECT_SCHEMA", DataType::Char, 128, 0, false),
                c("OBJECT_TYPE", DataType::Char, 24, 0, false),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                if table_type(d) != "V" {
                    continue;
                }
                for fmt in &d.formats {
                    let Some(l) = &fmt.logical else { continue };
                    for (id, _) in &l.based_on {
                        let (blib, bname) = name_of(txn, *id);
                        rows.push(vec![
                            s(d.name().to_string()),
                            s(lib.to_string()),
                            s(bname),
                            s(blib),
                            s("TABLE"),
                        ]);
                    }
                }
            }
            rows.dedup();
            (cols, rows)
        }
        "SYSCST" | "SYSCSTCOL" | "SYSKEYCST" => constraints(which, alias, &all),
        "SYSREFCST" => (
            vec![
                c("CONSTRAINT_NAME", DataType::Char, 128, 0, false),
                c("CONSTRAINT_SCHEMA", DataType::Char, 128, 0, false),
                c("UNIQUE_CONSTRAINT_NAME", DataType::Char, 128, 0, false),
                c("UNIQUE_CONSTRAINT_SCHEMA", DataType::Char, 128, 0, false),
                c("UPDATE_RULE", DataType::Char, 11, 0, false),
                c("DELETE_RULE", DataType::Char, 11, 0, false),
            ],
            Vec::new(),
        ),
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
                    s(h.name.to_string()),
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
            let cols = vec![
                c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
                c("TABLE_NAME", DataType::Char, 128, 0, false),
                c("COLUMN_NAME", DataType::Char, 128, 0, false),
                c("TABLE_PARTITION", DataType::Char, 128, 0, false),
                c("NUMBER_DISTINCT_VALUES", DataType::Packed, 20, 0, true),
            ];
            let mut rows = Vec::new();
            for (lib, d) in &all {
                if d.attr.kind == FileKind::Logical {
                    continue;
                }
                let Some(fmt) = d.formats.first() else {
                    continue;
                };
                for m in &d.members {
                    for f in &fmt.format.fields {
                        rows.push(vec![
                            s(lib.to_string()),
                            s(d.name().to_string()),
                            s(f.name.to_string()),
                            s(m.name.to_string()),
                            Value::Null,
                        ]);
                    }
                }
            }
            (cols, rows)
        }
        _ => return Ok(None),
    };
    Ok(Some((Schema { cols }, rows)))
}

/// The SQL type name of a field, as `SYSCOLUMNS.DATA_TYPE` spells it.
fn data_type_name(f: &FieldDef) -> String {
    let t = sql_type(f);
    t.split('(').next().unwrap_or(&t).trim().to_string()
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
fn base_of(txn: &mut Txn<'_>, fmt: &db::format::FormatDef) -> (String, String) {
    fmt.logical
        .as_ref()
        .and_then(|l| l.based_on.first())
        .map_or((String::new(), String::new()), |(id, _)| name_of(txn, *id))
}

/// `SYSCST`, `SYSCSTCOL` and `SYSKEYCST`: the primary keys, which are
/// the `UNIQUE` keys of physical files.
fn constraints(which: &str, alias: &str, all: &[(Name, FileDesc)]) -> (Vec<ColInfo>, Vec<Row>) {
    let c = |nm: &str, ty: DataType, len: u32, dec: u8, nullable: bool| {
        col(alias, nm, ty, len, dec, nullable)
    };
    let mut rows = Vec::new();
    for (lib, d) in all {
        let Some(fmt) = d.formats.first() else {
            continue;
        };
        if fmt.unique.is_none() || fmt.format.keys.is_empty() {
            continue;
        }
        // A unique index made for a UNIQUE constraint carries the
        // constraint's name in its text and belongs to its based-on table.
        let text = ccsid::decode_trimmed(&d.header.text);
        let (cname, ctype, table) = match (d.attr.kind, text.strip_prefix(super::ddl::UNIQUE_TEXT))
        {
            (FileKind::Logical, Some(c)) => {
                let base = fmt
                    .logical
                    .as_ref()
                    .and_then(|l| l.based_on.first())
                    .and_then(|(id, _)| all.iter().find(|(_, f)| f.id == *id))
                    .map_or_else(|| d.name().to_string(), |(_, f)| f.name().to_string());
                (c.to_string(), "UNIQUE", base)
            }
            (FileKind::Logical, None) => continue,
            _ => (
                format!("Q_{lib}_{}_{}_00001", d.name(), fmt.format.keys[0].name),
                "PRIMARY KEY",
                d.name().to_string(),
            ),
        };
        if which == "SYSCST" {
            rows.push(vec![
                s(cname),
                s(lib.to_string()),
                s(ctype),
                s(lib.to_string()),
                s(table.clone()),
                s("NO"),
                s("NO"),
                n(fmt.format.keys.len() as u64),
                s(table),
                s(lib.to_string()),
            ]);
        } else {
            for (i, k) in fmt.format.keys.iter().enumerate() {
                let mut row = vec![
                    s(cname.clone()),
                    s(lib.to_string()),
                    s(k.name.to_string()),
                    s(lib.to_string()),
                    s(table.clone()),
                ];
                if which == "SYSKEYCST" {
                    row.push(n(i as u64 + 1));
                }
                rows.push(row);
            }
        }
    }
    let cols = if which == "SYSCST" {
        vec![
            c("CONSTRAINT_NAME", DataType::Char, 128, 0, false),
            c("CONSTRAINT_SCHEMA", DataType::Char, 128, 0, false),
            c("CONSTRAINT_TYPE", DataType::Char, 11, 0, false),
            c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
            c("TABLE_NAME", DataType::Char, 128, 0, false),
            c("IS_DEFERRABLE", DataType::Char, 3, 0, false),
            c("INITIALLY_DEFERRED", DataType::Char, 3, 0, false),
            c("CONSTRAINT_KEYS", DataType::Packed, 9, 0, false),
            c("SYSTEM_TABLE_NAME", DataType::Char, 10, 0, false),
            c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, 0, false),
        ]
    } else {
        let mut v = vec![
            c("CONSTRAINT_NAME", DataType::Char, 128, 0, false),
            c("CONSTRAINT_SCHEMA", DataType::Char, 128, 0, false),
            c("COLUMN_NAME", DataType::Char, 128, 0, false),
            c("TABLE_SCHEMA", DataType::Char, 128, 0, false),
            c("TABLE_NAME", DataType::Char, 128, 0, false),
        ];
        if which == "SYSKEYCST" {
            v.push(c("ORDINAL_POSITION", DataType::Packed, 9, 0, false));
        }
        v
    };
    (cols, rows)
}

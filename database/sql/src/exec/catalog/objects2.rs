// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSVARIABLES`, `SYSVARIABLEDEP`, `SYSTYPES`, `SYSTRIGGERS`,
//! `SYSTRIGCOL`, `SYSTRIGDEP`, `SYSTRIGUPD`, `SYSJARCONTENTS` and
//! `SYSJAROBJECTS`, from the `VARIABLE`, `TYPE` and `TRIGGER` entries.
//! Columns, types and order are the primary's (V7R1M0,
//! `tools/conformance/sql/oracle/trigger/cols.out`). An SQL trigger's
//! definition (`db::trg::SqlDef`) gives its condition and statement, its
//! correlation names, the columns it uses (`SYSTRIGCOL`) and the tables
//! it names (`SYSTRIGDEP`); `SYSTRIGUPD` is any trigger's `UPDATE OF`
//! columns. `SYSVARIABLEDEP` is empty (BLOCKED(sql): variables record no
//! dependencies); the Java views are empty, as QS/400 has no Java
//! routines. IBM's LOB columns (`DEFAULT`, `ACTION_STATEMENT`, ...) are
//! `VARGRAPHIC(2000)` here: computed views have no LOB columns
//! (BLOCKED(sql)).

use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use super::super::restype::{Kind, SqlType};
use super::Table;
use crate::ast::{DataType, ObjectName};
use crate::error::Result;
use crate::value::{Timestamp, Value};

const VIEWS: &[&str] = &[
    "SYSVARIABLES",
    "SYSVARIABLEDEP",
    "SYSTYPES",
    "SYSTRIGGERS",
    "SYSTRIGCOL",
    "SYSTRIGDEP",
    "SYSTRIGUPD",
    "SYSJARCONTENTS",
    "SYSJAROBJECTS",
];

/// Whether `name` is one of these views.
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2") && VIEWS.contains(&name.name.value.as_str())
}

/// A column: name, kind, length, scale, nullable.
type Col = (&'static str, Kind, u32, u8, bool);

const V128: Kind = Kind::Varchar;
const NAT: u16 = 1200;

fn schema(alias: &str, cols: &[Col], national: &[&str]) -> Schema {
    Schema {
        cols: cols
            .iter()
            .map(|&(name, k, len, scale, null)| {
                let mut f = SqlType::new(k, len, scale, null).to_field();
                if national.contains(&name) {
                    f.ccsid = Some(NAT);
                } else if k == Kind::Dbclob {
                    // IBM's DBCLOB catalog columns not in CCSID 1200.
                    f.ccsid = Some(13488);
                }
                // Named as the column, so a query's description finds it.
                if let Ok(n) = qsname::Name::parse(&name[..name.len().min(10)]) {
                    f.name = n;
                }
                if name.len() > 10 {
                    f.alias = Some(name.to_string());
                }
                ColInfo { name: name.into(), table: Some(alias.into()), nullable: null, field: Some(f), hidden: false }
            })
            .collect(),
    }
}

const VARIABLES: &[Col] = &[
    ("VARIABLE_SCHEMA", V128, 128, 0, false),
    ("VARIABLE_NAME", V128, 128, 0, false),
    ("SYSTEM_VAR_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_VAR_NAME", Kind::Char, 10, 0, false),
    ("VARIABLE_OWNER", V128, 128, 0, false),
    ("VARIABLE_DEFINER", V128, 128, 0, false),
    ("VARIABLE_CREATED", Kind::Timestamp, 26, 6, false),
    ("SCOPE", Kind::Char, 1, 0, false),
    ("OWNER_TYPE", Kind::Char, 1, 0, false),
    ("USER_DEFINED_TYPE_SCHEMA", V128, 128, 0, true),
    ("USER_DEFINED_TYPE_NAME", V128, 128, 0, true),
    ("DATA_TYPE", V128, 128, 0, false),
    ("LENGTH", Kind::Integer, 0, 0, true),
    ("NUMERIC_SCALE", Kind::Integer, 0, 0, true),
    ("STORAGE", Kind::Integer, 0, 0, true),
    ("NUMERIC_PRECISION", Kind::Integer, 0, 0, true),
    ("CCSID", Kind::Integer, 0, 0, true),
    ("CHARACTER_MAXIMUM_LENGTH", Kind::Integer, 0, 0, true),
    ("CHARACTER_OCTET_LENGTH", Kind::Integer, 0, 0, true),
    ("NUMERIC_PRECISION_RADIX", Kind::Integer, 0, 0, true),
    ("DATETIME_PRECISION", Kind::Integer, 0, 0, true),
    ("DEFAULT", Kind::Dbclob, 2_097_152, 0, true),
    ("IASP_NUMBER", Kind::SmallInt, 0, 0, false),
    ("VARIABLE_TEXT", Kind::Vargraphic, 50, 0, true),
    ("LONG_COMMENT", Kind::Vargraphic, 2000, 0, true),
    ("DEFAULT_SCHEMA", V128, 128, 0, true),
    ("SQL_PATH", V128, 3483, 0, true),
    ("ROUNDING_MODE", Kind::Char, 1, 0, true),
];

const VARIABLEDEP: &[Col] = &[
    ("VARIABLE_SCHEMA", V128, 128, 0, false),
    ("VARIABLE_NAME", V128, 128, 0, false),
    ("OBJECT_SCHEMA", V128, 128, 0, false),
    ("OBJECT_NAME", V128, 128, 0, false),
    ("OBJECT_TYPE", Kind::Char, 24, 0, false),
    ("IASP_NUMBER", Kind::SmallInt, 0, 0, false),
    ("PARM_SIGNATURE", Kind::Varbinary, 10000, 0, true),
];

const TYPES: &[Col] = &[
    ("USER_DEFINED_TYPE_SCHEMA", V128, 128, 0, false),
    ("USER_DEFINED_TYPE_NAME", V128, 128, 0, false),
    ("USER_DEFINED_TYPE_DEFINER", V128, 128, 0, false),
    ("SOURCE_SCHEMA", V128, 128, 0, true),
    ("SOURCE_TYPE", V128, 128, 0, true),
    ("SYSTEM_TYPE_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_TYPE_NAME", Kind::Char, 10, 0, false),
    ("METATYPE", Kind::Char, 1, 0, false),
    ("LENGTH", Kind::Integer, 0, 0, false),
    ("NUMERIC_SCALE", Kind::SmallInt, 0, 0, true),
    ("CCSID", Kind::Integer, 0, 0, true),
    ("STORAGE", Kind::Integer, 0, 0, false),
    ("NUMERIC_PRECISION", Kind::Integer, 0, 0, true),
    ("CHARACTER_MAXIMUM_LENGTH", Kind::Integer, 0, 0, true),
    ("CHARACTER_OCTET_LENGTH", Kind::Integer, 0, 0, true),
    ("ALLOCATE", Kind::Integer, 0, 0, true),
    ("NUMERIC_PRECISION_RADIX", Kind::Integer, 0, 0, true),
    ("DATETIME_PRECISION", Kind::Integer, 0, 0, true),
    ("CREATE_TIME", Kind::Timestamp, 26, 6, true),
    ("LONG_COMMENT", Kind::Vargraphic, 2000, 0, true),
    ("IASP_NUMBER", Kind::SmallInt, 0, 0, false),
    ("LAST_ALTERED", Kind::Timestamp, 26, 6, true),
    ("NORMALIZE_DATA", Kind::Varchar, 3, 0, true),
    ("TYPE_TEXT", Kind::Vargraphic, 50, 0, true),
    ("MAXIMUM_CARDINALITY", Kind::BigInt, 0, 0, true),
];

const TRIGGERS: &[Col] = &[
    ("TRIGGER_SCHEMA", V128, 128, 0, true),
    ("TRIGGER_NAME", V128, 128, 0, false),
    ("EVENT_MANIPULATION", Kind::Varchar, 6, 0, false),
    ("EVENT_OBJECT_SCHEMA", V128, 128, 0, false),
    ("EVENT_OBJECT_TABLE", V128, 128, 0, false),
    ("ACTION_ORDER", Kind::Integer, 0, 0, false),
    ("ACTION_CONDITION", Kind::Dbclob, 2_097_152, 0, true),
    ("ACTION_STATEMENT", Kind::Dbclob, 2_097_152, 0, true),
    ("ACTION_ORIENTATION", Kind::Varchar, 9, 0, false),
    ("ACTION_TIMING", Kind::Varchar, 7, 0, false),
    ("TRIGGER_MODE", Kind::Varchar, 6, 0, false),
    ("ACTION_REFERENCE_OLD_ROW", V128, 128, 0, true),
    ("ACTION_REFERENCE_NEW_ROW", V128, 128, 0, true),
    ("ACTION_REFERENCE_OLD_TABLE", V128, 128, 0, true),
    ("ACTION_REFERENCE_NEW_TABLE", V128, 128, 0, true),
    ("SQL_PATH", V128, 3483, 0, true),
    ("CREATED", Kind::Timestamp, 26, 6, false),
    ("TRIGGER_PROGRAM_NAME", V128, 128, 0, false),
    ("TRIGGER_PROGRAM_LIBRARY", V128, 128, 0, false),
    ("OPERATIVE", Kind::Varchar, 1, 0, false),
    ("ENABLED", Kind::Varchar, 1, 0, false),
    ("THREADSAFE", Kind::Varchar, 8, 0, false),
    ("MULTITHREADED_JOB_ACTION", Kind::Varchar, 8, 0, false),
    ("ALLOW_REPEATED_CHANGE", Kind::Varchar, 8, 0, false),
    ("TRIGGER_UPDATE_CONDITION", Kind::Char, 8, 0, true),
    ("TRIGGER_DEFINER", V128, 128, 0, false),
    ("TRIGGER_TEXT", Kind::Vargraphic, 50, 0, true),
    ("LONG_COMMENT", Kind::Dbclob, 2000, 0, true),
    ("ROUNDING_MODE", Kind::Char, 1, 0, true),
    ("SYSTEM_TRIGGER_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_EVENT_OBJECT_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_EVENT_OBJECT_TABLE", Kind::Char, 10, 0, false),
];

const TRIGCOL: &[Col] = &[
    ("TRIGGER_SCHEMA", V128, 128, 0, false),
    ("TRIGGER_NAME", V128, 128, 0, false),
    ("TABLE_SCHEMA", V128, 128, 0, false),
    ("TABLE_NAME", V128, 128, 0, false),
    ("COLUMN_NAME", V128, 128, 0, false),
    ("OBJECT_TYPE", Kind::Varchar, 24, 0, false),
    ("SYSTEM_TRIGGER_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_TABLE_SCHEMA", Kind::Char, 10, 0, true),
    ("SYSTEM_TABLE_NAME", Kind::Char, 10, 0, true),
];

const TRIGDEP: &[Col] = &[
    ("TRIGGER_SCHEMA", V128, 128, 0, false),
    ("TRIGGER_NAME", V128, 128, 0, false),
    ("OBJECT_SCHEMA", V128, 128, 0, false),
    ("OBJECT_NAME", V128, 128, 0, false),
    ("OBJECT_TYPE", Kind::Char, 24, 0, false),
    ("PARM_SIGNATURE", Kind::Varbinary, 10000, 0, true),
    ("SYSTEM_TRIGGER_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_OBJECT_SCHEMA", Kind::Char, 10, 0, true),
    ("SYSTEM_OBJECT_NAME", Kind::Char, 10, 0, true),
];

const TRIGUPD: &[Col] = &[
    ("TRIGGER_SCHEMA", V128, 128, 0, true),
    ("TRIGGER_NAME", V128, 128, 0, false),
    ("EVENT_OBJECT_SCHEMA", V128, 128, 0, false),
    ("EVENT_OBJECT_TABLE", V128, 128, 0, false),
    ("TRIGGERED_UPDATE_COLUMNS", V128, 128, 0, false),
    ("SYSTEM_TRIGGER_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_EVENT_OBJECT_SCHEMA", Kind::Char, 10, 0, false),
    ("SYSTEM_EVENT_OBJECT_TABLE", Kind::Char, 10, 0, false),
];

const JARCONTENTS: &[Col] = &[
    ("JARSCHEMA", V128, 128, 0, false),
    ("JAR_ID", V128, 128, 0, false),
    ("CLASS", V128, 128, 0, false),
    ("CLASS_SOURCE", Kind::Vargraphic, 2000, 0, true),
    ("IASP_NUMBER", Kind::SmallInt, 0, 0, false),
];

const JAROBJECTS: &[Col] = &[
    ("JARSCHEMA", V128, 128, 0, false),
    ("JAR_ID", V128, 128, 0, false),
    ("DEFINER", V128, 128, 0, false),
    ("JAR_DATA", Kind::Varbinary, 2000, 0, true),
    ("IASP_NUMBER", Kind::SmallInt, 0, 0, false),
    ("JAR_CREATED", Kind::Timestamp, 26, 6, false),
    ("LAST_ALTERED", Kind::Timestamp, 26, 6, true),
    ("DEBUG_MODE", Kind::Char, 1, 0, false),
    ("DEBUG_DATA", Kind::Varchar, 2000, 0, true),
];

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

fn i(v: i64) -> Value {
    Value::int(v)
}

fn opt(v: Option<i64>) -> Value {
    v.map_or(Value::Null, i)
}

fn stamp(secs: u64) -> Value {
    Value::Timestamp(Timestamp::new(db::record::datetime::Date::from_unix(secs), db::record::datetime::Time::from_unix(secs), 0))
}

/// Every library, with its system name.
fn libraries(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<(String, ObjectId)>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    if !libs.contains(&ctx.qsys) {
        libs.insert(0, ctx.qsys);
    }
    let mut out = Vec::new();
    for l in libs {
        if let Ok((_, h)) = ops::read_header(txn, l) {
            out.push((h.name.to_string(), l));
        }
    }
    Ok(out)
}

/// A type's catalog shape, as the primary shows it: name, length,
/// scale, precision, CCSID, storage, character length, radix.
struct Shape {
    name: &'static str,
    length: i64,
    scale: Option<i64>,
    precision: Option<i64>,
    ccsid: Option<i64>,
    storage: i64,
    chars: Option<i64>,
    radix: Option<i64>,
}

fn shape(t: &crate::ast::TypeSpec) -> Shape {
    let num = |name, length: i64, scale: i64, precision: i64, storage| Shape {
        name,
        length,
        scale: Some(scale),
        precision: Some(precision),
        ccsid: None,
        storage,
        chars: None,
        radix: Some(10),
    };
    let chars = |name, n: u32, storage: i64| Shape {
        name,
        length: i64::from(n),
        scale: None,
        precision: None,
        ccsid: Some(t.ccsid.map_or(0, i64::from)),
        storage,
        chars: Some(i64::from(n)),
        radix: None,
    };
    let other = |name, length: i64| Shape { name, length, scale: None, precision: None, ccsid: None, storage: length, chars: None, radix: None };
    match &t.data_type {
        DataType::SmallInt => num("SMALLINT", 2, 0, 4, 2),
        DataType::Integer => num("INTEGER", 4, 0, 9, 4),
        DataType::BigInt => num("BIGINT", 8, 0, 18, 8),
        DataType::Decimal(p, sc) => num("DECIMAL", i64::from(*p), i64::from(*sc), i64::from(*p), i64::from(*p) / 2 + 1),
        DataType::Numeric(p, sc) => num("NUMERIC", i64::from(*p), i64::from(*sc), i64::from(*p), i64::from(*p)),
        DataType::Real => other("REAL", 4),
        DataType::Double => other("DOUBLE", 8),
        DataType::DecFloat(p) => other("DECFLOAT", if *p == 16 { 8 } else { 16 }),
        DataType::Char(n) => chars("CHARACTER", *n, i64::from(*n)),
        DataType::Varchar(n) => chars("VARCHAR", *n, i64::from(*n) + 2),
        DataType::Clob(n) => chars("CLOB", *n, i64::from(*n)),
        DataType::Graphic(n) => chars("GRAPHIC", *n, i64::from(*n) * 2),
        DataType::Vargraphic(n) => chars("VARGRAPHIC", *n, i64::from(*n) * 2 + 2),
        DataType::Dbclob(n) => chars("DBCLOB", *n, i64::from(*n) * 2),
        DataType::Binary(n) => other("BINARY", i64::from(*n)),
        DataType::Varbinary(n) => other("VARBINARY", i64::from(*n)),
        DataType::Blob(n) => other("BLOB", i64::from(*n)),
        DataType::Date => other("DATE", 4),
        DataType::Time => other("TIME", 3),
        DataType::Timestamp(_) => other("TIMESTAMP", 10),
        DataType::Boolean => other("BOOLEAN", 1),
    }
}

/// The rows and columns, or `None` when `name` is not one of these.
pub fn virtual_table(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName, alias: &str) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    let nat = ["VARIABLE_TEXT", "LONG_COMMENT", "TRIGGER_TEXT", "TYPE_TEXT", "DEFAULT", "ACTION_CONDITION", "ACTION_STATEMENT"];
    Ok(Some(match name.name.value.as_str() {
        "SYSVARIABLES" => (schema(alias, VARIABLES, &nat), variables(txn, ctx)?),
        "SYSVARIABLEDEP" => (schema(alias, VARIABLEDEP, &nat), Vec::new()),
        "SYSTYPES" => (schema(alias, TYPES, &nat), udts(txn, ctx)?),
        "SYSTRIGGERS" => (schema(alias, TRIGGERS, &["TRIGGER_TEXT"]), triggers(txn, ctx)?),
        "SYSTRIGCOL" => (schema(alias, TRIGCOL, &nat), trigger_uses(txn, ctx, false)?),
        "SYSTRIGDEP" => (schema(alias, TRIGDEP, &nat), trigger_uses(txn, ctx, true)?),
        "SYSTRIGUPD" => (schema(alias, TRIGUPD, &nat), trigger_updates(txn, ctx)?),
        "SYSJARCONTENTS" => (schema(alias, JARCONTENTS, &nat), Vec::new()),
        _ => (schema(alias, JAROBJECTS, &nat), Vec::new()),
    }))
}

fn variables(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for (lib, l) in libraries(txn, ctx)? {
        let sch = crate::exec::names::schema_name(txn, l);
        for (sys, _, name, def, created, owner) in super::super::var::in_library(txn, l)? {
            let Ok(t) = crate::pl::types::parse_type(&def.data_type) else { continue };
            let sh = shape(&t);
            let datetime = matches!(t.data_type, DataType::Date | DataType::Time | DataType::Timestamp(_));
            rows.push(vec![
                s(sch.clone()),
                s(name),
                ten(&lib),
                ten(&sys.to_string()),
                s(owner.clone()),
                s(owner),
                stamp(created),
                s("S"),
                s("U"),
                Value::Null,
                Value::Null,
                s(sh.name),
                i(sh.length),
                opt(sh.scale),
                i(sh.storage),
                opt(sh.precision),
                opt(sh.ccsid),
                opt(sh.chars),
                opt(sh.chars),
                opt(sh.radix),
                if datetime { i(0) } else { Value::Null },
                def.default.map_or(Value::Null, s),
                Value::int(0),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ]);
        }
    }
    Ok(rows)
}

fn udts(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for (lib, l) in libraries(txn, ctx)? {
        let sch = crate::exec::names::schema_name(txn, l);
        for (sys, name, u, created, owner) in super::super::udt::in_library(txn, l)? {
            let Ok(t) = crate::pl::types::parse_type(&u.source) else { continue };
            let sh = shape(&t);
            let array = u.metatype == b'A';
            rows.push(vec![
                s(sch.clone()),
                s(name),
                s(owner),
                if array { s("QSYS2") } else { Value::Null },
                s(sh.name),
                ten(&lib),
                ten(&sys.to_string()),
                s(char::from(u.metatype).to_string()),
                i(sh.length),
                opt(sh.scale),
                opt(sh.ccsid),
                i(sh.storage),
                opt(sh.precision),
                opt(sh.chars),
                opt(sh.chars),
                Value::Null,
                opt(sh.radix),
                Value::Null,
                stamp(created),
                Value::Null,
                Value::int(0),
                if array { Value::Null } else { stamp(created) },
                Value::Null,
                Value::Null,
                if array { Value::int(i64::try_from(u.cardinality).unwrap_or(i64::MAX)) } else { Value::Null },
            ]);
        }
    }
    Ok(rows)
}

/// A word padded to eight, as the primary's `VARCHAR(8)` columns hold it.
fn ten(w: &str) -> Value {
    s(format!("{w:<10}"))
}

/// A word padded to eight, as the primary's `VARCHAR(8)` columns hold it.
fn eight(w: &str) -> Value {
    s(format!("{w:<8}"))
}

/// One trigger of a table: the table's schema and library, its SQL and
/// system names, the trigger.
struct OnTable {
    schema: String,
    lib: String,
    table: String,
    sys: String,
    t: db::trg::Trigger,
}

/// Every trigger of every SQL table.
fn all_triggers(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<OnTable>> {
    let mut out = Vec::new();
    for (lib, l) in libraries(txn, ctx)? {
        let sch = crate::exec::names::schema_name(txn, l);
        let Ok(list) = ops::list(txn, l, Some(types::FILE)) else { continue };
        for (sys, id) in list {
            let Ok(desc) = db::file::describe(txn, id) else { continue };
            if desc.header.sql.is_none() || desc.attr.kind != db::file::FileKind::Physical {
                continue;
            }
            let trgs = db::trg::list(txn, id)?;
            if trgs.is_empty() {
                continue;
            }
            let table = crate::exec::names::sql_name(txn, id);
            for t in trgs {
                out.push(OnTable { schema: sch.clone(), lib: lib.clone(), table: table.clone(), sys: sys.name.to_string(), t });
            }
        }
    }
    Ok(out)
}

/// A file's schema, SQL name, kind (`TABLE`, `VIEW`) and its columns'
/// system and SQL names.
type FileNames = (String, String, &'static str, Vec<(String, String)>);

/// [`FileNames`] from the library's and the file's system names.
fn file_names(txn: &mut Txn<'_>, ctx: &SqlContext, lib: qsname::Name, file: qsname::Name) -> Option<FileNames> {
    let l = ops::find_library(txn, ctx.qsys, lib).ok()?;
    let id = store::obj::library::Library::load(txn, l.id).ok()?.lookup(txn, types::FILE, file).ok()??;
    let kind = if crate::exec::view::is_view(txn, id) { "VIEW" } else { "TABLE" };
    let cols = db::file::describe(txn, id)
        .ok()
        .and_then(|d| d.formats.first().cloned())
        .map(|f| f.format.fields.iter().map(|x| (x.name.to_string(), x.alias.clone().unwrap_or_else(|| x.name.to_string()))).collect())
        .unwrap_or_default();
    Some((crate::exec::names::schema_name(txn, l.id), crate::exec::names::sql_name(txn, id), kind, cols))
}

/// `SYSTRIGCOL` (`deps` false) or `SYSTRIGDEP` (true) rows.
fn trigger_uses(txn: &mut Txn<'_>, ctx: &SqlContext, deps: bool) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for o in all_triggers(txn, ctx)? {
        let Some(def) = o.t.def.as_deref() else { continue };
        let tlib = o.t.library.to_string();
        let tsch = ops::find_library(txn, ctx.qsys, o.t.library).map_or_else(|_| tlib.clone(), |l| crate::exec::names::schema_name(txn, l.id));
        if deps {
            for (l, f) in &def.objects {
                let Some((sch, name, kind, _)) = file_names(txn, ctx, *l, *f) else { continue };
                rows.push(vec![s(tsch.clone()), s(o.t.name.clone()), s(sch), s(name), s(format!("{kind:<24}")), Value::Null, ten(&tlib), ten(&l.to_string()), ten(&f.to_string())]);
            }
            continue;
        }
        for (l, f, c) in &def.columns {
            let Some((sch, name, kind, cols)) = file_names(txn, ctx, *l, *f) else { continue };
            let col = cols.iter().find(|(sys, _)| *sys == c.to_string()).map_or_else(|| c.to_string(), |(_, n)| n.clone());
            rows.push(vec![s(tsch.clone()), s(o.t.name.clone()), s(sch), s(name), s(col), s(kind), ten(&tlib), ten(&l.to_string()), ten(&f.to_string())]);
        }
    }
    Ok(rows)
}

/// `SYSTRIGUPD`: the `UPDATE OF` columns.
fn trigger_updates(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for o in all_triggers(txn, ctx)? {
        if o.t.columns.is_empty() {
            continue;
        }
        let tlib = o.t.library.to_string();
        let tsch = ops::find_library(txn, ctx.qsys, o.t.library).map_or_else(|_| tlib.clone(), |l| crate::exec::names::schema_name(txn, l.id));
        let lib = qsname::Name::parse(&o.lib).ok();
        let file = qsname::Name::parse(&o.sys).ok();
        let cols = match (lib, file) {
            (Some(l), Some(f)) => file_names(txn, ctx, l, f).map(|x| x.3).unwrap_or_default(),
            _ => Vec::new(),
        };
        for c in &o.t.columns {
            let col = cols.iter().find(|(sys, _)| *sys == c.to_string()).map_or_else(|| c.to_string(), |(_, n)| n.clone());
            rows.push(vec![s(tsch.clone()), s(o.t.name.clone()), s(o.schema.clone()), s(o.table.clone()), s(col), ten(&tlib), ten(&o.lib), ten(&o.sys)]);
        }
    }
    Ok(rows)
}

fn triggers(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Row>> {
    use db::trg::{MtAction, Threadsafe, UpdateCondition};
    let mut rows = Vec::new();
    for o in all_triggers(txn, ctx)? {
        let OnTable { schema: sch, lib, table, sys, t } = o;
        let tlib = t.library.to_string();
        let def = t.def.as_deref();
        let text = |f: fn(&db::trg::SqlDef) -> &String| def.map(f).filter(|v| !v.is_empty()).map_or(Value::Null, |v| s(v.clone()));
        rows.push(vec![
            s(tlib.clone()),
            s(t.name.clone()),
            s(t.event.sql()),
            s(sch.clone()),
            s(table.clone()),
            i(i64::from(t.number)),
            text(|d| &d.condition),
            text(|d| &d.statement),
            s(if t.statement { "STATEMENT" } else { "ROW" }),
            s(t.time.sql()),
            s(if t.db2sql { "DB2SQL" } else { "DB2ROW" }),
            text(|d| &d.old_row),
            text(|d| &d.new_row),
            text(|d| &d.old_table),
            text(|d| &d.new_table),
            Value::Null,
            stamp(t.created),
            s(t.program.1.to_string()),
            s(t.program.0.to_string()),
            s(if t.operative { "Y" } else { "N" }),
            s(if t.enabled { "Y" } else { "N" }),
            eight(match t.threadsafe {
                Threadsafe::Unknown => "UNKNOWN",
                Threadsafe::Yes => "YES",
                Threadsafe::No => "NO",
            }),
            eight(match t.mt_action {
                MtAction::Sysval => "SYSVAL",
                MtAction::Msg => "MSG",
                MtAction::NoRun => "NORUN",
                MtAction::Run => "RUN",
            }),
            eight(if t.alwrepchg { "YES" } else { "NO" }),
            match t.update_condition {
                UpdateCondition::None => Value::Null,
                UpdateCondition::Always => eight("ALWAYS"),
                UpdateCondition::Change => eight("CHANGE"),
            },
            s(t.definer.clone()),
            if t.text.is_empty() { Value::Null } else { s(t.text.clone()) },
            text(|d| &d.comment),
            if t.sql { s("E") } else { Value::Null },
            ten(&tlib),
            ten(&lib),
            ten(&sys),
        ]);
    }
    Ok(rows)
}

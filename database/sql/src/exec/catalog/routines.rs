// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSROUTINES`, `SYSPROCS`, `SYSFUNCS`, `SYSPARMS` and
//! `SYSROUTINEDEP`: the routines of every library, from the `ROUTINE`,
//! `PARM` and `DEPENDENCY` entries of their programs' `*SQLINF`. The
//! columns are the primary's (V7R1M0). IBM's internal `PARSE_TREE` and
//! `PARM_ARRAY` are empty here.

use dds::{DataType, FieldDef};
use db::sqlinf::routine::flag;
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use super::super::routine::find::{self, Found};
use super::super::routine::model;
use super::Table;
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::{Timestamp, Value};

const VIEWS: &[&str] = &["SYSROUTINES", "SYSPROCS", "SYSFUNCS", "SYSPARMS", "SYSROUTINEDEP"];

/// Whether `name` is one of these views.
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2") && VIEWS.contains(&name.name.value.as_str())
}

/// The types the columns have.
#[derive(Clone, Copy)]
enum T {
    Varchar(u32),
    Char(u32),
    Small,
    Int,
    Big,
    Stamp,
    Bits,
}

fn col(alias: &str, name: &str, t: T, nullable: bool) -> ColInfo {
    let (ty, len) = match t {
        T::Varchar(n) | T::Char(n) => (DataType::Char, n),
        T::Small => (DataType::Binary, 4),
        T::Int => (DataType::Binary, 9),
        T::Big => (DataType::Binary, 18),
        T::Stamp => (DataType::Timestamp, 26),
        T::Bits => (DataType::Hex, 32_700),
    };
    let fname = Name::parse(&name[..name.len().min(10)]).unwrap_or_else(|_| Name::parse("COL").expect("valid"));
    let mut f = FieldDef::new(fname, ty, len, 0);
    f.int_range = ty == DataType::Binary;
    if matches!(t, T::Varchar(_) | T::Bits) {
        f.varlen = Some(dds::VarLen { allocated: None });
    }
    f.allow_null = nullable;
    if name.len() > 10 {
        f.alias = Some(name.to_string());
    }
    ColInfo { name: name.to_string(), table: Some(alias.to_string()), field: Some(f), nullable, hidden: false }
}

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

fn yes(b: bool) -> Value {
    s(if b { "YES" } else { "NO" })
}

fn stamp(secs: u64) -> Value {
    Value::Timestamp(Timestamp::new(db::record::datetime::Date::from_unix(secs), db::record::datetime::Time::from_unix(secs), 0))
}

/// Every routine of every library.
fn routines(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Found>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    let mut out = Vec::new();
    for lib in libs {
        out.extend(find::in_library(txn, lib)?);
    }
    Ok(out)
}

/// The rows and columns, or `None` when `name` is not one of these.
pub fn virtual_table(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName, alias: &str) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    let all = routines(txn, ctx)?;
    Ok(Some(match name.name.value.as_str() {
        "SYSPARMS" => parms(txn, &all, alias),
        "SYSROUTINEDEP" => deps(txn, &all, alias)?,
        view => routine_rows(txn, &all, alias, view),
    }))
}

/// The columns of `SYSROUTINES`, and which of them each other view has.
const ROUTINE_COLS: &[(&str, T, bool, u8)] = &[
    // name, type, nullable, views: 1 SYSROUTINES, 2 SYSPROCS, 4 SYSFUNCS
    ("SPECIFIC_SCHEMA", T::Varchar(128), false, 7),
    ("SPECIFIC_NAME", T::Varchar(128), false, 7),
    ("ROUTINE_SCHEMA", T::Varchar(128), false, 7),
    ("ROUTINE_NAME", T::Varchar(128), false, 7),
    ("ROUTINE_TYPE", T::Varchar(9), false, 1),
    ("ROUTINE_CREATED", T::Stamp, false, 7),
    ("ROUTINE_DEFINER", T::Varchar(128), false, 7),
    ("ROUTINE_BODY", T::Varchar(8), false, 7),
    ("EXTERNAL_NAME", T::Varchar(279), true, 7),
    ("EXTERNAL_LANGUAGE", T::Varchar(8), true, 7),
    ("PARAMETER_STYLE", T::Varchar(7), true, 7),
    ("IS_DETERMINISTIC", T::Varchar(3), false, 7),
    ("SQL_DATA_ACCESS", T::Varchar(8), true, 7),
    ("SQL_PATH", T::Varchar(3483), true, 7),
    ("PARM_SIGNATURE", T::Bits, false, 7),
    ("NUMBER_OF_RESULTS", T::Small, true, 5),
    ("MAX_DYNAMIC_RESULT_SETS", T::Small, false, 1),
    ("RESULT_SETS", T::Small, false, 2),
    ("IN_PARMS", T::Small, false, 7),
    ("OUT_PARMS", T::Small, false, 3),
    ("INOUT_PARMS", T::Small, false, 3),
    ("PARSE_TREE", T::Bits, false, 1),
    ("PARM_ARRAY", T::Bits, false, 1),
    ("LONG_COMMENT", T::Varchar(2000), true, 7),
    ("ROUTINE_DEFINITION", T::Varchar(32_740), true, 7),
    ("FUNCTION_ORIGIN", T::Char(1), false, 5),
    ("FUNCTION_TYPE", T::Char(1), false, 5),
    ("EXTERNAL_ACTION", T::Char(1), true, 5),
    ("IS_NULL_CALL", T::Varchar(3), true, 5),
    ("SCRATCH_PAD", T::Int, true, 5),
    ("FINAL_CALL", T::Varchar(3), true, 5),
    ("PARALLELIZABLE", T::Varchar(3), true, 5),
    ("DBINFO", T::Varchar(3), true, 7),
    ("SOURCE_SPECIFIC_SCHEMA", T::Varchar(128), true, 5),
    ("SOURCE_SPECIFIC_NAME", T::Varchar(128), true, 5),
    ("IS_USER_DEFINED_CAST", T::Varchar(3), true, 5),
    ("CARDINALITY", T::Big, true, 5),
    ("FENCED", T::Varchar(3), true, 5),
    ("COMMIT_ON_RETURN", T::Varchar(3), true, 3),
    ("IASP_NUMBER", T::Small, false, 7),
    ("NEW_SAVEPOINT_LEVEL", T::Varchar(3), true, 3),
    ("LAST_ALTERED", T::Stamp, true, 1),
    ("DEBUG_MODE", T::Char(1), false, 1),
    ("DEBUG_DATA", T::Varchar(32_740), true, 1),
    ("ROUNDING_MODE", T::Char(1), true, 7),
    ("INLINE", T::Varchar(3), true, 4),
    ("ROUTINE_TEXT", T::Varchar(50), true, 7),
];

/// A routine's parameters (role 0) as stored.
fn parm_types(f: &Found) -> Vec<(u8, crate::ast::TypeSpec)> {
    f.routine
        .parms
        .iter()
        .filter(|p| p.role == 0)
        .filter_map(|p| crate::pl::types::parse_type(&p.ty).ok().map(|t| (p.mode, t)))
        .collect()
}

/// One routine's value of a column.
#[allow(clippy::too_many_lines)] // one arm per column
fn value(txn: &mut Txn<'_>, f: &Found, col: &str) -> Value {
    let d = &f.routine.def;
    let sql = d.body == 1;
    let func = d.routine_type == 2;
    let ps = parm_types(f);
    let count = |m: u8| Value::int(i64::try_from(ps.iter().filter(|(pm, _)| *pm == m).count()).unwrap_or(0));
    match col {
        "SPECIFIC_SCHEMA" | "ROUTINE_SCHEMA" => s(f.schema.clone()),
        "SPECIFIC_NAME" => s(d.specific.clone()),
        "ROUTINE_NAME" => s(d.name.clone()),
        "ROUTINE_TYPE" => s(if func { "FUNCTION" } else { "PROCEDURE" }),
        "ROUTINE_CREATED" => stamp(d.created),
        "ROUTINE_DEFINER" => s(ops::read_header(txn, f.owner).map(|(_, h)| store::obj::header::text_of(&h.owner)).unwrap_or_default()),
        "ROUTINE_BODY" => s(if sql { "SQL" } else { "EXTERNAL" }),
        "EXTERNAL_NAME" => {
            if d.entry.is_empty() {
                s(format!("{}/{}", d.program_library, d.program))
            } else {
                s(format!("{}/{}({})", d.program_library, d.program, d.entry))
            }
        }
        "EXTERNAL_LANGUAGE" if d.body == 2 => s(model::language_word(d.language)),
        "PARAMETER_STYLE" if d.body == 2 => s(model::style_word(d.style)),
        // A sourced function is as deterministic as its source (the built-ins are).
        "IS_DETERMINISTIC" => yes(d.has(flag::DETERMINISTIC) || d.body == 3),
        "SQL_DATA_ACCESS" => s(model::access_word(d.access)),
        "SQL_PATH" => {
            let mut p: Vec<String> = ["QSYS", "QSYS2", "SYSPROC", "SYSIBMADM"].iter().map(|x| format!("\"{x}\"")).collect();
            p.push(format!("\"{}\"", f.schema));
            s(p.join(","))
        }
        "PARM_SIGNATURE" => Value::Bin(ps.iter().flat_map(|(_, t)| model::sqltype(&t.data_type).to_be_bytes()).collect()),
        "NUMBER_OF_RESULTS" if d.function_type == 2 => Value::int(i64::try_from(f.routine.parms.iter().filter(|p| p.role == 1).count()).unwrap_or(0)),
        "NUMBER_OF_RESULTS" => Value::int(i64::from(func)),
        "MAX_DYNAMIC_RESULT_SETS" | "RESULT_SETS" => Value::int(d.result_sets.into()),
        "IN_PARMS" => count(1),
        "OUT_PARMS" => count(2),
        "INOUT_PARMS" => count(3),
        "PARSE_TREE" | "PARM_ARRAY" => Value::Bin(Vec::new()),
        "ROUTINE_DEFINITION" if sql => s(d.body_text.clone()),
        "FUNCTION_ORIGIN" | "FUNCTION_TYPE" if !func => s(" "),
        // 7.1 shows its SQL functions as external (C service programs).
        "FUNCTION_ORIGIN" => s(if d.body == 3 { "U" } else { "E" }),
        "FUNCTION_TYPE" => s(if d.function_type == 2 { "T" } else { "S" }),
        "EXTERNAL_ACTION" if func => s(if d.has(flag::NO_EXTERNAL_ACTION) { "N" } else { "E" }),
        "IS_NULL_CALL" if func => yes(!d.has(flag::NULL_INPUT)),
        "SCRATCH_PAD" if func => Value::int(d.scratchpad.into()),
        "FINAL_CALL" if func => yes(d.has(flag::FINAL_CALL)),
        "PARALLELIZABLE" if func => yes(d.has(flag::PARALLEL)),
        "SOURCE_SPECIFIC_SCHEMA" if d.body == 3 => s(d.sourced.rsplit_once('.').map_or(String::new(), |(a, _)| a.to_string())),
        "SOURCE_SPECIFIC_NAME" if d.body == 3 => s(d.sourced.rsplit_once('.').map_or(d.sourced.clone(), |(_, b)| b.to_string())),
        "IS_USER_DEFINED_CAST" if func => s("NO"),
        "FENCED" if func => yes(!d.has(flag::NOT_FENCED)),
        "INLINE" if func => yes(d.body_text.starts_with("RETURN")),
        "DBINFO" => yes(d.has(flag::DBINFO)),
        "COMMIT_ON_RETURN" => yes(d.has(flag::COMMIT_ON_RETURN)),
        "IASP_NUMBER" => Value::int(0),
        "NEW_SAVEPOINT_LEVEL" => yes(d.has(flag::NEW_SAVEPOINT_LEVEL)),
        "DEBUG_MODE" => s("0"),
        "ROUNDING_MODE" => s("E"),
        _ => Value::Null,
    }
}

fn routine_rows(txn: &mut Txn<'_>, all: &[Found], alias: &str, view: &str) -> Table {
    let bit = match view {
        "SYSPROCS" => 2,
        "SYSFUNCS" => 4,
        _ => 1,
    };
    let chosen: Vec<&(&str, T, bool, u8)> = ROUTINE_COLS.iter().filter(|c| c.3 & bit != 0).collect();
    let cols = chosen.iter().map(|(n, t, null, _)| col(alias, n, *t, *null)).collect();
    let mut rows = Vec::new();
    for f in all {
        let func = f.routine.def.routine_type == 2;
        if (bit == 2 && func) || (bit == 4 && !func) {
            continue;
        }
        rows.push(chosen.iter().map(|(n, ..)| value(txn, f, n)).collect());
    }
    (Schema { cols }, rows)
}

fn parms(_txn: &mut Txn<'_>, all: &[Found], alias: &str) -> Table {
    let cols = vec![
        col(alias, "SPECIFIC_SCHEMA", T::Varchar(128), false),
        col(alias, "SPECIFIC_NAME", T::Varchar(128), false),
        col(alias, "ORDINAL_POSITION", T::Int, false),
        col(alias, "PARAMETER_MODE", T::Varchar(5), false),
        col(alias, "PARAMETER_NAME", T::Varchar(128), true),
        col(alias, "DATA_TYPE", T::Varchar(128), false),
        col(alias, "NUMERIC_SCALE", T::Int, true),
        col(alias, "NUMERIC_PRECISION", T::Int, true),
        col(alias, "CCSID", T::Int, true),
        col(alias, "CHARACTER_MAXIMUM_LENGTH", T::Int, true),
        col(alias, "CHARACTER_OCTET_LENGTH", T::Int, true),
        col(alias, "NUMERIC_PRECISION_RADIX", T::Int, true),
        col(alias, "DATETIME_PRECISION", T::Int, true),
        col(alias, "IS_NULLABLE", T::Varchar(3), false),
        col(alias, "LONG_COMMENT", T::Varchar(2000), true),
        col(alias, "ROW_TYPE", T::Char(1), true),
        col(alias, "DATA_TYPE_SCHEMA", T::Varchar(128), true),
        col(alias, "DATA_TYPE_NAME", T::Varchar(128), true),
        col(alias, "AS_LOCATOR", T::Varchar(3), false),
        col(alias, "IASP_NUMBER", T::Small, false),
        col(alias, "NORMALIZE_DATA", T::Varchar(3), true),
        // 7.2 and later: a parameter's DEFAULT as written.
        col(alias, "DEFAULT", T::Varchar(2000), true),
    ];
    let mut rows: Vec<Row> = Vec::new();
    for f in all {
        for p in &f.routine.parms {
            let Ok(t) = crate::pl::types::parse_type(&p.ty) else { continue };
            let (_, len, scale, numeric) = crate::pl::types::catalog_type(&t.data_type);
            let int = |v: i64| Value::int(v);
            let char_like = !numeric && !matches!(t.data_type, crate::ast::DataType::Date | crate::ast::DataType::Time | crate::ast::DataType::Timestamp(_));
            let octets = match t.data_type {
                crate::ast::DataType::Graphic(n) | crate::ast::DataType::Vargraphic(n) | crate::ast::DataType::Dbclob(n) => i64::from(n) * 2,
                _ => len,
            };
            let datetime = match t.data_type {
                crate::ast::DataType::Date | crate::ast::DataType::Time => Some(0),
                crate::ast::DataType::Timestamp(p) => Some(i64::from(p)),
                _ => None,
            };
            let binary = matches!(t.data_type, crate::ast::DataType::Binary(_) | crate::ast::DataType::Varbinary(_) | crate::ast::DataType::Blob(_));
            rows.push(vec![
                s(f.schema.clone()),
                s(f.routine.def.specific.clone()),
                int(p.ordinal.into()),
                s(match (p.role, p.mode) {
                    (0, 2) => "OUT",
                    (0, 3) => "INOUT",
                    (0, _) => "IN",
                    _ => "OUT",
                }),
                if p.name.is_empty() { Value::Null } else { s(p.name.clone()) },
                s(model::parm_type_word(&t.data_type)),
                scale.map_or(Value::Null, int),
                if numeric { int(len) } else { Value::Null },
                if char_like && !binary { int(t.ccsid.map_or(37, i64::from)) } else { Value::Null },
                if char_like { int(len) } else { Value::Null },
                if char_like { int(octets) } else { Value::Null },
                if numeric { int(if matches!(t.data_type, crate::ast::DataType::Real | crate::ast::DataType::Double) { 2 } else { 10 }) } else { Value::Null },
                datetime.map_or(Value::Null, int),
                s("YES"),
                Value::Null,
                // P a parameter, C a scalar function's result, R a table function's column.
                s(match p.role {
                    0 => "P",
                    1 => "R",
                    _ => "C",
                }),
                Value::Null,
                Value::Null,
                s("NO"),
                int(0),
                s("NO"),
                p.default.clone().map_or(Value::Null, s),
            ]);
        }
    }
    (Schema { cols }, rows)
}

fn deps(txn: &mut Txn<'_>, all: &[Found], alias: &str) -> Result<Table> {
    let cols = vec![
        col(alias, "SPECIFIC_SCHEMA", T::Varchar(128), false),
        col(alias, "SPECIFIC_NAME", T::Varchar(128), false),
        col(alias, "OBJECT_CATALOG", T::Varchar(128), true),
        col(alias, "OBJECT_SCHEMA", T::Varchar(128), false),
        col(alias, "OBJECT_NAME", T::Varchar(128), false),
        col(alias, "OBJECT_TYPE", T::Char(24), false),
        col(alias, "PARM_SIGNATURE", T::Bits, true),
        col(alias, "IASP_NUMBER", T::Small, false),
        col(alias, "NUMBER_OF_PARMS", T::Small, true),
    ];
    let mut rows: Vec<Row> = Vec::new();
    for f in all.iter().filter(|f| f.routine.def.body == 1) {
        let Some(info) = db::sqlinf::load(txn, f.owner)? else { continue };
        for b in info.all(db::sqlinf::tag::DEPENDENCY) {
            let Ok(d) = db::sqlinf::body::Dependency::decode(b) else { continue };
            rows.push(vec![
                s(f.schema.clone()),
                s(f.routine.def.specific.clone()),
                Value::Null,
                s(crate::exec::names::unquoted(d.library)),
                s(crate::exec::names::unquoted(d.name)),
                s(format!("{:<24}", "TABLE")),
                Value::Null,
                Value::int(0),
                Value::Null,
            ]);
        }
    }
    Ok((Schema { cols }, rows))
}

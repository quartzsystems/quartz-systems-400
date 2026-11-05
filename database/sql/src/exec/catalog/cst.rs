// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The constraint and sequence catalogs: `SYSCST`, `SYSCSTCOL`,
//! `SYSKEYCST`, `SYSCHKCST`, `SYSREFCST`, `SYSCSTDEP` and `SYSSEQUENCES`,
//! from the files' `CONSTRAINT` entries and the sequences' `*SQLINF`.
//! Columns, types and order are the primary's (V7R1M0,
//! `tools/conformance/sql/oracle/integrity/catalogs.txt`).

use db::cst::{Constraint, Kind};
use db::file::{self, FileDesc, FileKind};
use dds::{DataType, FieldDef, VarLen};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use super::Table;
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::Value;

/// The views here.
const VIEWS: &[&str] = &["SYSCST", "SYSCSTCOL", "SYSKEYCST", "SYSCHKCST", "SYSREFCST", "SYSCSTDEP", "SYSSEQUENCES"];

/// Whether `name` is one of them.
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2")
        && VIEWS.contains(&name.name.value.as_str())
}

/// A column's shape.
#[derive(Clone, Copy)]
enum T {
    /// `VARCHAR(n)`.
    Var(u32),
    /// `CHAR(n)`.
    Fix(u32),
    /// `NVARCHAR(n)`.
    NVar(u32),
    /// `SMALLINT`.
    Small,
    /// `INTEGER`.
    Int,
    /// `BIGINT`.
    Big,
    /// `DECIMAL(p, 0)`.
    Dec(u32),
}

fn col(alias: &str, name: &str, t: T, nullable: bool) -> ColInfo {
    let short = Name::parse(&name[..name.len().min(10)]).unwrap_or_else(|_| Name::parse("COL").expect("valid"));
    let mut f = match t {
        T::Var(n) | T::Fix(n) => FieldDef::new(short, DataType::Char, n, 0),
        T::NVar(n) => {
            let mut f = FieldDef::new(short, DataType::Graphic, n, 0);
            f.ccsid = Some(1200);
            f
        }
        T::Small => FieldDef::new(short, DataType::Binary, 4, 0),
        T::Int => FieldDef::new(short, DataType::Binary, 9, 0),
        T::Big => FieldDef::new(short, DataType::Binary, 18, 0),
        T::Dec(p) => FieldDef::new(short, DataType::Packed, p, 0),
    };
    if matches!(t, T::Var(_) | T::NVar(_)) {
        f.varlen = Some(VarLen { allocated: None });
    }
    f.int_range = matches!(t, T::Small | T::Int | T::Big);
    f.allow_null = nullable;
    if name.len() > 10 {
        f.alias = Some(name.to_string());
    }
    ColInfo { name: name.to_string(), table: Some(alias.to_string()), field: Some(f), nullable, hidden: false }
}

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

fn i(v: usize) -> Value {
    Value::int(i64::try_from(v).unwrap_or(i64::MAX))
}

fn opt(v: Option<String>) -> Value {
    v.map_or(Value::Null, Value::Str)
}

/// One physical file with SQL information, its library and SQL names.
struct Owner {
    lib: Name,
    desc: FileDesc,
    /// The table's SQL name.
    sql: String,
    csts: Vec<Constraint>,
}

fn sql_name(txn: &mut Txn<'_>, id: ObjectId, sys: Name) -> String {
    db::sqlinf::load(txn, id).ok().flatten().and_then(|i| i.name()).unwrap_or_else(|| sys.to_string())
}

fn libraries(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<(Name, ObjectId)>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    if !libs.contains(&ctx.qsys) {
        libs.insert(0, ctx.qsys);
    }
    let mut out = Vec::new();
    for l in libs {
        if let Ok((_, h)) = ops::read_header(txn, l) {
            out.push((h.name, l));
        }
    }
    Ok(out)
}

/// Every table with constraints.
fn owners(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Owner>> {
    let mut out = Vec::new();
    for (lib, l) in libraries(txn, ctx)? {
        let Ok(list) = ops::list(txn, l, Some(types::FILE)) else { continue };
        for (_, id) in list {
            let Ok(desc) = file::describe(txn, id) else { continue };
            if desc.attr.kind != FileKind::Physical || desc.header.sql.is_none() {
                continue;
            }
            let csts: Vec<Constraint> = db::cst::list(txn, id)?.into_iter().filter(|c| c.kind != Kind::Dependent).collect();
            if csts.is_empty() {
                continue;
            }
            let sql = sql_name(txn, id, desc.name());
            out.push(Owner { lib, desc, sql, csts });
        }
    }
    Ok(out)
}

/// The SQL name of field `n` of a file.
fn column(desc: &FileDesc, n: Name) -> String {
    desc.formats[0].format.field(&n).and_then(|(_, f)| f.alias.clone()).unwrap_or_else(|| n.to_string())
}

/// The columns and rows of view `which`.
#[allow(clippy::too_many_lines)]
pub fn table(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName, alias: &str) -> Result<Option<Table>> {
    let which = name.name.value.as_str();
    let c = |n: &str, t: T, null: bool| col(alias, n, t, null);
    let v128 = T::Var(128);
    let mut rows: Vec<Row> = Vec::new();
    let cols = match which {
        "SYSSEQUENCES" => return sequences(txn, ctx, alias).map(Some),
        "SYSCST" => {
            for o in owners(txn, ctx)? {
                let lib = o.lib.to_string();
                for k in &o.csts {
                    let keys = if k.kind == Kind::Check { Value::Null } else { i(k.columns.len()) };
                    let pending = match k.kind {
                        Kind::Unique | Kind::PrimaryKey => Value::Null,
                        _ if !k.enabled => Value::Null,
                        _ => s(if k.check_pending { "YES" } else { "NO" }),
                    };
                    rows.push(vec![
                        s(lib.clone()),
                        s(k.name.clone()),
                        s(k.kind.sql_type()),
                        s(lib.clone()),
                        s(o.sql.clone()),
                        s("NO"),
                        s("NO"),
                        s(o.desc.name().to_string()),
                        s(lib.clone()),
                        keys,
                        i(0),
                        s("ESTABLISHED"),
                        s(if k.enabled { "YES" } else { "NO" }),
                        pending,
                        Value::Null,
                        Value::Null,
                        s(lib.clone()),
                    ]);
                }
            }
            vec![
                c("CONSTRAINT_SCHEMA", v128, false),
                c("CONSTRAINT_NAME", v128, false),
                c("CONSTRAINT_TYPE", T::Var(11), false),
                c("TABLE_SCHEMA", v128, false),
                c("TABLE_NAME", v128, false),
                c("IS_DEFERRABLE", T::Var(3), false),
                c("INITIALLY_DEFERRED", T::Var(3), false),
                c("SYSTEM_TABLE_NAME", T::Fix(10), false),
                c("SYSTEM_TABLE_SCHEMA", T::Fix(10), false),
                c("CONSTRAINT_KEYS", T::Small, true),
                c("IASP_NUMBER", T::Small, false),
                c("CONSTRAINT_STATE", T::Var(11), false),
                c("ENABLED", T::Var(3), true),
                c("CHECK_PENDING", T::Var(3), true),
                c("CONSTRAINT_TEXT", T::NVar(50), true),
                c("LONG_COMMENT", T::NVar(2000), true),
                c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
            ]
        }
        "SYSCSTCOL" | "SYSKEYCST" | "SYSCSTDEP" => {
            for o in owners(txn, ctx)? {
                let lib = o.lib.to_string();
                let sys = o.desc.name().to_string();
                let owner = ccsid::decode_trimmed(&o.desc.header.owner);
                for k in &o.csts {
                    if which == "SYSCSTDEP" {
                        rows.push(vec![s(lib.clone()), s(o.sql.clone()), s(lib.clone()), s(k.name.clone()), s(sys.clone()), s(lib.clone()), i(0), s(lib.clone())]);
                        continue;
                    }
                    if which == "SYSKEYCST" && k.kind == Kind::Check {
                        continue;
                    }
                    for (n, f) in k.columns.iter().enumerate() {
                        let cn = column(&o.desc, *f);
                        if which == "SYSCSTCOL" {
                            rows.push(vec![s(lib.clone()), s(o.sql.clone()), s(cn), s(lib.clone()), s(k.name.clone()), s(f.to_string()), s(sys.clone()), s(lib.clone()), s(lib.clone())]);
                        } else {
                            let pos = o.desc.formats[0].format.field(f).map_or(0, |(p, _)| p + 1);
                            rows.push(vec![
                                s(lib.clone()),
                                s(k.name.clone()),
                                s(lib.clone()),
                                s(o.sql.clone()),
                                s(cn),
                                i(n + 1),
                                i(pos),
                                s(owner.clone()),
                                s(f.to_string()),
                                s(sys.clone()),
                                s(lib.clone()),
                                s(lib.clone()),
                            ]);
                        }
                    }
                }
            }
            match which {
                "SYSCSTCOL" => vec![
                    c("TABLE_SCHEMA", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("COLUMN_NAME", v128, false),
                    c("CONSTRAINT_SCHEMA", v128, false),
                    c("CONSTRAINT_NAME", v128, false),
                    c("SYSTEM_COLUMN_NAME", T::Fix(10), false),
                    c("SYSTEM_TABLE_NAME", T::Fix(10), false),
                    c("SYSTEM_TABLE_SCHEMA", T::Fix(10), false),
                    c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
                ],
                "SYSKEYCST" => vec![
                    c("CONSTRAINT_SCHEMA", v128, false),
                    c("CONSTRAINT_NAME", v128, false),
                    c("TABLE_SCHEMA", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("COLUMN_NAME", v128, false),
                    c("ORDINAL_POSITION", T::Int, false),
                    c("COLUMN_POSITION", T::Int, false),
                    c("TABLE_OWNER", v128, false),
                    c("SYSTEM_COLUMN_NAME", T::Fix(10), false),
                    c("SYSTEM_TABLE_NAME", T::Fix(10), false),
                    c("SYSTEM_TABLE_SCHEMA", T::Fix(10), false),
                    c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
                ],
                _ => vec![
                    c("TABLE_SCHEMA", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("CONSTRAINT_SCHEMA", v128, false),
                    c("CONSTRAINT_NAME", v128, false),
                    c("SYSTEM_TABLE_NAME", T::Fix(10), false),
                    c("SYSTEM_TABLE_SCHEMA", T::Fix(10), false),
                    c("IASP_NUMBER", T::Small, false),
                    c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
                ],
            }
        }
        "SYSCHKCST" => {
            for o in owners(txn, ctx)? {
                let lib = o.lib.to_string();
                for k in o.csts.iter().filter(|k| k.kind == Kind::Check) {
                    rows.push(vec![s(lib.clone()), s(k.name.clone()), s(k.text.clone()), Value::Null, s(lib.clone())]);
                }
            }
            vec![
                c("CONSTRAINT_SCHEMA", v128, false),
                c("CONSTRAINT_NAME", v128, false),
                c("CHECK_CLAUSE", T::NVar(2000), true),
                c("ROUNDING_MODE", T::Fix(1), true),
                c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
            ]
        }
        "SYSREFCST" => {
            for o in owners(txn, ctx)? {
                let lib = o.lib.to_string();
                for k in o.csts.iter().filter(|k| k.kind == Kind::Referential) {
                    let plib = ops::read_header(txn, k.other)
                        .ok()
                        .and_then(|(_, h)| ops::read_header(txn, h.library).ok())
                        .map(|(_, l)| l.name.to_string());
                    rows.push(vec![
                        s(lib.clone()),
                        s(k.name.clone()),
                        opt(plib.clone()),
                        s(k.other_name.clone()),
                        s("NONE"),
                        s(k.update_rule.sql()),
                        s(k.delete_rule.sql()),
                        i(k.columns.len()),
                        s(lib.clone()),
                        opt(plib),
                    ]);
                }
            }
            vec![
                c("CONSTRAINT_SCHEMA", v128, false),
                c("CONSTRAINT_NAME", v128, false),
                c("UNIQUE_CONSTRAINT_SCHEMA", v128, true),
                c("UNIQUE_CONSTRAINT_NAME", v128, true),
                c("MATCH_OPTION", T::Var(7), false),
                c("UPDATE_RULE", T::Var(11), false),
                c("DELETE_RULE", T::Var(11), false),
                c("COLUMN_COUNT", T::Int, false),
                c("SYSTEM_CONSTRAINT_SCHEMA", T::Fix(10), false),
                c("SYSTEM_UNIQUE_CONSTRAINT_SCHEMA", T::Fix(10), true),
            ]
        }
        _ => return Ok(None),
    };
    Ok(Some((Schema { cols }, rows)))
}

/// `SYSSEQUENCES`: the sequences of every library.
#[allow(clippy::many_single_char_names)]
fn sequences(txn: &mut Txn<'_>, ctx: &SqlContext, alias: &str) -> Result<Table> {
    let c = |n: &str, t: T, null: bool| col(alias, n, t, null);
    let mut rows = Vec::new();
    for (lib, l) in libraries(txn, ctx)? {
        let Ok(list) = ops::list(txn, l, Some(types::DTAARA)) else { continue };
        for (sys, id) in list {
            let Ok(Some(info)) = db::sqlinf::load(txn, id) else { continue };
            let Some(body) = info.first(db::sqlinf::tag::SEQUENCE) else { continue };
            let Ok(q) = db::sqlinf::seq::Sequence::decode(body) else { continue };
            let (_, h) = ops::read_header(txn, id)?;
            let (ty, p) = match q.data_type {
                1 => ("SMALLINT", 5),
                3 => ("BIGINT", 19),
                4 => ("DECIMAL", u32::from(q.precision)),
                5 => ("NUMERIC", u32::from(q.precision)),
                _ => ("INTEGER", 10),
            };
            let d = |v: i128| Value::Num(qdecimal::Decimal::new(v, 0).expect("fits"));
            let lname = lib.to_string();
            rows.push(vec![
                s(lname.clone()),
                s(info.name().unwrap_or_else(|| sys.name.to_string())),
                s(ccsid::decode_trimmed(&h.owner)),
                s(ty),
                i(usize::try_from(p).unwrap_or(0)),
                d(q.attrs.start),
                d(q.attrs.min),
                d(q.attrs.max),
                d(q.attrs.increment),
                s(if q.attrs.cycle { "YES" } else { "NO" }),
                i(q.attrs.cache as usize),
                s(if q.attrs.order { "YES" } else { "NO" }),
                s(sys.name.to_string()),
                s(lname),
            ]);
        }
    }
    let cols = vec![
        c("SEQUENCE_SCHEMA", T::Var(128), false),
        c("SEQUENCE_NAME", T::Var(128), false),
        c("SEQUENCE_DEFINER", T::Var(128), false),
        c("DATA_TYPE", T::Var(128), false),
        c("NUMERIC_PRECISION", T::Int, false),
        c("START", T::Dec(31), false),
        c("MINIMUM_VALUE", T::Dec(31), false),
        c("MAXIMUM_VALUE", T::Dec(31), false),
        c("INCREMENT", T::Dec(31), false),
        c("CYCLE_OPTION", T::Var(3), false),
        c("CACHE", T::Int, false),
        c("ORDER", T::Var(3), false),
        c("SYSTEM_SEQUENCE_NAME", T::Fix(10), false),
        c("SYSTEM_SEQUENCE_SCHEMA", T::Fix(10), false),
    ];
    let _ = T::Big;
    Ok((Schema { cols }, rows))
}

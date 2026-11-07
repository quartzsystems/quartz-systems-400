// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSPACKAGE` and `QSYS2.SYSPACKAGESTAT`: the SQL packages
//! (`*SQLPKG`, `docs/DB_FORMAT.md` §9) of every library. The columns are
//! the primary's (V7R1M0); the precompile options and use counts a
//! package does not keep are null.

use db::sqlpkg::{self, Package, PkgKind};
use dds::{DataType, FieldDef};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use super::Table;
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::{Timestamp, Value};

const VIEWS: &[&str] = &["SYSPACKAGE", "SYSPACKAGESTAT"];

/// Whether `name` is one of these views.
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2")
        && VIEWS.contains(&name.name.value.as_str())
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
    /// `CHAR(n) FOR BIT DATA`.
    Bits(u32),
    /// `VARBINARY(n)`.
    Varbin(u32),
}

fn col(alias: &str, name: &str, t: T, nullable: bool) -> ColInfo {
    let (ty, len) = match t {
        T::Varchar(n) | T::Char(n) => (DataType::Char, n),
        T::Bits(n) => (DataType::Hex, n),
        T::Small => (DataType::Binary, 4),
        T::Int => (DataType::Binary, 9),
        T::Big => (DataType::Binary, 18),
        T::Stamp => (DataType::Timestamp, 26),
        T::Varbin(n) => (DataType::Bytes, n),
    };
    let fname = Name::parse(&name[..name.len().min(10)])
        .unwrap_or_else(|_| Name::parse("COL").expect("valid"));
    let mut f = FieldDef::new(fname, ty, len, 0);
    f.int_range = ty == DataType::Binary;
    if matches!(t, T::Varchar(_) | T::Varbin(_)) {
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

/// `SYSPACKAGE`'s columns.
const PACKAGE_COLS: &[(&str, T, bool)] = &[
    ("PACKAGE_CATALOG", T::Varchar(18), true),
    ("PACKAGE_SCHEMA", T::Varchar(128), false),
    ("PACKAGE_NAME", T::Varchar(128), false),
    ("PACKAGE_OWNER", T::Varchar(128), false),
    ("PACKAGE_CREATOR", T::Varchar(128), false),
    ("CREATION_TIMESTAMP", T::Char(26), false),
    ("DEFAULT_SCHEMA", T::Varchar(128), true),
    ("PROGRAM_NAME", T::Varchar(128), true),
    ("PROGRAM_SCHEMA", T::Varchar(128), true),
    ("PROGRAM_CATALOG", T::Varchar(18), true),
    ("ISOLATION", T::Char(2), false),
    ("QUOTE", T::Char(1), false),
    ("COMMA", T::Char(1), false),
    ("PACKAGE_TEXT", T::Varchar(50), true),
    ("LONG_COMMENT", T::Varchar(2000), true),
    ("CONSISTENCY_TOKEN", T::Bits(8), false),
    ("SYSTEM_PACKAGE_NAME", T::Char(10), false),
    ("SYSTEM_PACKAGE_SCHEMA", T::Char(10), false),
    ("SYSTEM_DEFAULT_SCHEMA", T::Char(10), true),
    ("SYSTEM_PROGRAM_NAME", T::Char(10), true),
    ("SYSTEM_PROGRAM_SCHEMA", T::Char(10), true),
    ("IASP_NUMBER", T::Small, false),
    ("ROUNDING_MODE", T::Char(1), true),
    ("CONCURRENTACCESSRESOLUTION", T::Char(1), true),
];

/// `SYSPACKAGESTAT`'s columns.
const STAT_COLS: &[(&str, T, bool)] = &[
    ("PACKAGE_SCHEMA", T::Varchar(128), false),
    ("PACKAGE_NAME", T::Varchar(128), false),
    ("PACKAGE_OWNER", T::Varchar(128), false),
    ("PACKAGE_CREATOR", T::Varchar(128), false),
    ("CREATION_TIMESTAMP", T::Char(26), false),
    ("DEFAULT_SCHEMA", T::Varchar(128), true),
    ("ISOLATION", T::Varchar(2), false),
    ("CONCURRENTACCESSRESOLUTION", T::Varchar(2), true),
    ("SECONDARY_SPACE_COUNT", T::Int, true),
    ("PENDING_FULL", T::Varchar(3), true),
    ("PACKAGE_TYPE", T::Varchar(16), false),
    ("NUMBER_STATEMENTS", T::Int, false),
    ("PACKAGE_USED_SIZE", T::Int, false),
    ("NUMBER_DUMMIES", T::Int, true),
    ("NUMBER_COMPRESSIONS", T::Int, true),
    ("STATEMENT_CONTENTION_COUNT", T::Big, true),
    ("EXTENDED_INDICATOR", T::Varchar(10), true),
    ("C_NUL_REQUIRED", T::Varchar(10), true),
    ("NAMING", T::Varchar(4), false),
    ("TARGET_RELEASE", T::Varchar(6), true),
    ("EARLIEST_POSSIBLE_RELEASE", T::Varchar(6), true),
    ("RDB", T::Varchar(18), true),
    ("CONSISTENCY_TOKEN", T::Varbin(8), false),
    ("ALLOW_COPY_DATA", T::Varchar(9), true),
    ("DATE_FORMAT", T::Varchar(4), false),
    ("DATE_SEPARATOR", T::Char(1), false),
    ("TIME_FORMAT", T::Varchar(4), false),
    ("TIME_SEPARATOR", T::Char(1), false),
    ("DYNAMIC_DEFAULT_SCHEMA", T::Varchar(4), true),
    ("CURRENT_RULES", T::Varchar(4), true),
    ("ALLOW_BLOCK", T::Varchar(8), true),
    ("DELAY_PREPARE", T::Varchar(4), true),
    ("USER_PROFILE", T::Varchar(7), true),
    ("DYNAMIC_USER_PROFILE", T::Varchar(6), true),
    ("SORT_SEQUENCE", T::Varchar(10), true),
    ("LANGUAGE_IDENTIFIER", T::Char(3), true),
    ("SORT_SEQUENCE_SCHEMA", T::Char(10), true),
    ("SORT_SEQUENCE_NAME", T::Char(10), true),
    ("RDB_CONNECTION_METHOD", T::Varchar(4), true),
    ("DECRESULT_MAXIMUM_PRECISION", T::Small, true),
    ("DECRESULT_MAXIMUM_SCALE", T::Small, true),
    ("DECRESULT_MINIMUM_DIVIDE_SCALE", T::Small, true),
    ("DECFLOAT_ROUNDING_MODE", T::Varchar(8), true),
    ("DECFLOAT_WARNING", T::Varchar(3), true),
    ("SQLPATH", T::Varchar(3483), true),
    ("LAST_USED_TIMESTAMP", T::Stamp, true),
    ("DAYS_USED_COUNT", T::Int, true),
    ("LAST_RESET_TIMESTAMP", T::Stamp, true),
    ("SYSTEM_PACKAGE_NAME", T::Char(10), false),
    ("SYSTEM_PACKAGE_SCHEMA", T::Char(10), false),
    ("IASP_NUMBER", T::Small, false),
];

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

/// A system name blank padded to ten, as a CHAR(10) column holds it.
fn ten(n: &str) -> Value {
    s(format!("{n:<10}"))
}

fn int(n: impl Into<i64>) -> Value {
    Value::int(n.into())
}

/// The creation time IBM's way, `YYYY-MM-DD-HH.MM.SS.ffffff`, from the
/// package's token (microseconds).
fn created(token: u64) -> String {
    let secs = token / 1_000_000;
    let d = db::record::datetime::Date::from_unix(secs);
    let t = db::record::datetime::Time::from_unix(secs);
    format!(
        "{:04}-{:02}-{:02}-{:02}.{:02}.{:02}.{:06}",
        d.year,
        d.month,
        d.day,
        t.hour,
        t.minute,
        t.second,
        token % 1_000_000
    )
}

fn stamp(secs: u64) -> Value {
    Value::Timestamp(Timestamp::new(
        db::record::datetime::Date::from_unix(secs),
        db::record::datetime::Time::from_unix(secs),
        0,
    ))
}

/// The isolation of a package's commit code, as each view spells it.
fn isolation(commit: u8, stat: bool) -> &'static str {
    match (commit, stat) {
        (0, false) => "NO",
        (0, true) => "NC",
        (1, _) => "UR",
        (2, _) => "CS",
        (3, _) => "RS",
        _ => "RR",
    }
}

fn date_format(code: u8) -> &'static str {
    [
        "*JUL", "*MDY", "*DMY", "*YMD", "*USA", "*ISO", "*EUR", "*JIS",
    ]
    .get(usize::from(code))
    .copied()
    .unwrap_or("*ISO")
}

fn time_format(code: u8) -> &'static str {
    ["*HMS", "*USA", "*ISO", "*EUR", "*JIS"]
        .get(usize::from(code))
        .copied()
        .unwrap_or("*HMS")
}

fn sep(b: u8) -> Value {
    s(char::from(if b == 0 { b' ' } else { b }).to_string())
}

/// One package and where it is.
struct Pkg {
    schema: String,
    lib: String,
    name: String,
    owner: String,
    creator: String,
    text: String,
    last_used: u64,
    days_used: u32,
    reset: u64,
    package: Package,
}

/// Every package of every library.
fn packages(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<Pkg>> {
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    libs.insert(0, ctx.qsys);
    let mut out = Vec::new();
    for lib in libs {
        let Ok(objs) = ops::list(txn, lib, Some(sqlpkg::SQLPKG)) else {
            continue;
        };
        if objs.is_empty() {
            continue;
        }
        let schema = crate::exec::names::schema_name(txn, lib);
        let lname = ops::read_header(txn, lib)
            .map(|(_, h)| h.name.to_string())
            .unwrap_or_default();
        for (_, id) in objs {
            let Ok((_, h)) = ops::read_header(txn, id) else {
                continue;
            };
            let Ok(pkg) = sqlpkg::load(txn, id) else {
                continue;
            };
            out.push(Pkg {
                schema: schema.clone(),
                lib: lname.clone(),
                name: h.name.to_string(),
                owner: ccsid::decode_trimmed(&h.owner),
                creator: ccsid::decode_trimmed(&h.creator),
                text: ccsid::decode_trimmed(&h.text),
                last_used: h.last_used,
                days_used: h.days_used,
                reset: h.reset_date,
                package: pkg,
            });
        }
    }
    Ok(out)
}

fn or_null(v: &str) -> Value {
    if v.is_empty() { Value::Null } else { s(v) }
}

fn package_row(p: &Pkg) -> Row {
    let a = &p.package.attr;
    let extended = a.kind == PkgKind::Extended;
    let creator = if p.creator.is_empty() {
        p.owner.clone()
    } else {
        p.creator.clone()
    };
    vec![
        // The program that made it: the host server's for extended dynamic.
        if extended { s("QSQPRCED") } else { Value::Null },
        s(p.schema.clone()),
        s(p.name.clone()),
        s(p.owner.clone()),
        s(creator),
        s(created(a.token)),
        or_null(&a.default_collection),
        Value::Null,
        Value::Null,
        Value::Null,
        s(isolation(a.commit, false)),
        s("N"),
        s(if a.decimal_point == b',' { "Y" } else { "N" }),
        or_null(&p.text),
        Value::Null,
        Value::Bin(a.token.to_be_bytes().to_vec()),
        ten(&p.name),
        ten(&p.lib),
        if a.default_collection.is_empty() {
            Value::Null
        } else {
            ten(&a.default_collection)
        },
        Value::Null,
        Value::Null,
        int(0),
        Value::Null,
        Value::Null,
    ]
}

fn stat_row(p: &Pkg) -> Row {
    let a = &p.package.attr;
    let size: usize = 64
        + p.package
            .stmts
            .iter()
            .map(|x| x.encode().len())
            .sum::<usize>();
    let creator = if p.creator.is_empty() {
        p.owner.clone()
    } else {
        p.creator.clone()
    };
    let mut r = vec![
        s(p.schema.clone()),
        s(p.name.clone()),
        s(p.owner.clone()),
        s(creator),
        s(created(a.token)),
        or_null(&a.default_collection),
        s(isolation(a.commit, true)),
        Value::Null,
        Value::Null,
        s("NO"),
        s(if a.kind == PkgKind::Extended {
            "EXTENDED DYNAMIC"
        } else {
            "DISTRIBUTED"
        }),
        int(i64::try_from(p.package.stmts.len()).unwrap_or(i64::MAX)),
        int(i64::try_from(size).unwrap_or(i64::MAX)),
        int(0),
        int(0),
        int(0),
        Value::Null,
        Value::Null,
        s(if a.sql_naming { "*SQL" } else { "*SYS" }),
        Value::Null,
        Value::Null,
        Value::Null,
        Value::Bin(a.token.to_be_bytes().to_vec()),
        Value::Null,
        s(date_format(a.date_format)),
        sep(a.date_sep),
        s(time_format(a.time_format)),
        sep(a.time_sep),
    ];
    // DYNAMIC_DEFAULT_SCHEMA to SQLPATH: options a package does not keep.
    r.extend(std::iter::repeat_n(Value::Null, 17));
    r.extend([
        if p.last_used == 0 {
            Value::Null
        } else {
            stamp(p.last_used)
        },
        if p.last_used == 0 {
            Value::Null
        } else {
            int(p.days_used)
        },
        if p.reset == 0 {
            Value::Null
        } else {
            stamp(p.reset)
        },
        ten(&p.name),
        ten(&p.lib),
        int(0),
    ]);
    r
}

/// The rows and columns, or `None` when `name` is not one of these.
pub fn virtual_table(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    alias: &str,
) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    let stat = name.name.value == "SYSPACKAGESTAT";
    let defs = if stat { STAT_COLS } else { PACKAGE_COLS };
    let cols = defs
        .iter()
        .map(|(n, t, null)| col(alias, n, *t, *null))
        .collect();
    let rows = packages(txn, ctx)?
        .iter()
        .map(|p| if stat { stat_row(p) } else { package_row(p) })
        .collect();
    Ok(Some((Schema { cols }, rows)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_have_every_column() {
        let p = Pkg {
            schema: "QSTEST".into(),
            lib: "QSTEST".into(),
            name: "QSPKG9899".into(),
            owner: "QSECOFR".into(),
            creator: String::new(),
            text: String::new(),
            last_used: 0,
            days_used: 0,
            reset: 0,
            package: Package::default(),
        };
        assert_eq!(package_row(&p).len(), PACKAGE_COLS.len());
        assert_eq!(stat_row(&p).len(), STAT_COLS.len());
        assert_eq!(created(1_762_309_112_456_405), "2025-11-05-02.18.32.456405");
    }
}

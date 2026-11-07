// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `SYSIBM` metadata views JDBC and ODBC drivers read (`SQLTABLES`,
//! `SQLCOLUMNS`, ...) and the ANSI information views (`TABLES`,
//! `COLUMNS`, `VIEWS`, `SCHEMATA`), computed from the `QSYS2` catalog
//! rows. Names, columns and types are the primary's
//! (`tools/conformance/sql/cases/sysibm/`). The views over routines,
//! types, character sets and the `_S` system-name forms are not here
//! yet: naming one is `SQL0204`.

use dds::{DataType, FieldDef, VarLen};
use qsname::Name;
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::Value;

/// The views here.
pub const VIEWS: &[&str] = &[
    "SQLTABLES",
    "SQLCOLUMNS",
    "SQLPRIMARYKEYS",
    "SQLFOREIGNKEYS",
    "SQLSCHEMAS",
    "SQLTABLETYPES",
    "SYSTABLETYPES",
    "SQLTYPEINFO",
    "SQLSTATISTICS",
    "SQLSPECIALCOLUMNS",
    "SQLTABLEPRIVILEGES",
    "TABLES",
    "COLUMNS",
    "VIEWS",
    "SCHEMATA",
];

/// The server's name: `CATALOG_NAME`, `TABLE_CAT`.
const SERVER: &str = "QS400";

/// Whether `name` is one of the views here: `SYSIBM` qualifies them.
#[must_use]
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_some_and(|s| s.value == "SYSIBM")
        && VIEWS.contains(&name.name.value.as_str())
}

/// A column's shape.
#[derive(Clone, Copy)]
enum Ty {
    Var(u32),
    Fixed(u32),
    Graphic(u32),
    Nclob,
    Small,
    Int,
    Big,
}

fn col(alias: &str, name: &str, ty: Ty, nullable: bool) -> ColInfo {
    let short = Name::parse(&name[..name.len().min(10)])
        .unwrap_or_else(|_| Name::parse("COL").expect("valid"));
    let mut f = match ty {
        Ty::Var(n) | Ty::Fixed(n) => FieldDef::new(short, DataType::Char, n, 0),
        Ty::Graphic(n) => FieldDef::new(short, DataType::Graphic, n, 0),
        Ty::Nclob => FieldDef::new(short, DataType::Dbclob, 2_097_152, 0),
        Ty::Small => FieldDef::new(short, DataType::Binary, 4, 0),
        Ty::Int => FieldDef::new(short, DataType::Binary, 9, 0),
        Ty::Big => FieldDef::new(short, DataType::Binary, 18, 0),
    };
    if matches!(ty, Ty::Var(_) | Ty::Graphic(_)) {
        f.varlen = Some(VarLen { allocated: None });
    }
    if matches!(ty, Ty::Graphic(_) | Ty::Nclob) {
        f.ccsid = Some(1200);
    }
    f.int_range = matches!(ty, Ty::Small | Ty::Int | Ty::Big);
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

fn int(v: i64) -> Value {
    Value::int(v)
}

const N: Value = Value::Null;

/// The rows of a `QSYS2` view, read by column name.
struct View {
    cols: Vec<String>,
    rows: Vec<Row>,
}

impl View {
    fn load(txn: &mut Txn<'_>, ctx: &SqlContext, name: &str) -> Result<Self> {
        let n = ObjectName::qualified("QSYS2", name);
        Ok(match super::virtual_table(txn, ctx, &n, name)? {
            Some((sch, rows)) => Self {
                cols: sch.cols.iter().map(|c| c.name.clone()).collect(),
                rows,
            },
            None => Self {
                cols: Vec::new(),
                rows: Vec::new(),
            },
        })
    }

    fn at<'a>(&self, r: &'a Row, c: &str) -> &'a Value {
        self.cols
            .iter()
            .position(|x| x == c)
            .and_then(|i| r.get(i))
            .unwrap_or(&N)
    }

    fn text(&self, r: &Row, c: &str) -> String {
        let v = self.at(r, c);
        if v.is_null() {
            String::new()
        } else {
            v.as_text().trim_end().to_string()
        }
    }

    fn num(&self, r: &Row, c: &str) -> Option<i64> {
        self.at(r, c).as_i64().ok()
    }
}

/// What a column's type is to JDBC and ODBC.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TypeInfo {
    odbc: i64,
    name: &'static str,
    jdbc: i64,
    internal: i64,
    size: Option<i64>,
    buffer: i64,
    digits: Option<i64>,
    radix: Option<i64>,
    octets: Option<i64>,
    datetime_sub: Option<i64>,
}

/// A `SYSCOLUMNS` type as the metadata views give it.
fn type_info(data_type: &str, len: i64, scale: Option<i64>, ccsid: Option<i64>) -> TypeInfo {
    let bits = ccsid == Some(65535);
    let t = |odbc, name, jdbc, internal| TypeInfo {
        odbc,
        name,
        jdbc,
        internal,
        size: Some(len),
        buffer: len,
        digits: None,
        radix: None,
        octets: Some(len),
        datetime_sub: None,
    };
    let exact = |odbc, name, internal, size: i64, buffer| TypeInfo {
        size: Some(size),
        buffer,
        digits: Some(scale.unwrap_or(0)),
        radix: Some(10),
        octets: None,
        ..t(odbc, name, odbc, internal)
    };
    let dt = |odbc, name, size, sub, digits| TypeInfo {
        size: Some(size),
        buffer: if sub == 3 { 16 } else { 6 },
        digits: Some(digits),
        octets: None,
        datetime_sub: Some(sub),
        ..t(odbc, name, odbc, odbc)
    };
    match data_type {
        "CHAR" | "CHARACTER" if bits => t(-2, "CHAR () FOR BIT DATA", -2, -2),
        "CHAR" | "CHARACTER" => t(1, "CHAR", 1, 1),
        "VARCHAR" if bits => t(-3, "VARCHAR () FOR BIT DATA", -3, -3),
        "VARCHAR" => t(12, "VARCHAR", 12, 12),
        "GRAPHIC" => TypeInfo {
            buffer: len * 2,
            octets: Some(len * 2),
            ..t(-95, "GRAPHIC", 1, 95)
        },
        "VARG" | "VARGRAPHIC" => TypeInfo {
            buffer: len * 2,
            octets: Some(len * 2),
            ..t(-96, "VARGRAPHIC", 12, 96)
        },
        "BINARY" => t(-2, "BINARY", -2, -2),
        "VARBIN" | "VARBINARY" => t(-3, "VARBINARY", -3, -3),
        "BLOB" => t(-98, "BLOB", 2004, 13),
        "CLOB" => t(-99, "CLOB", 2005, 14),
        "DBCLOB" => TypeInfo {
            buffer: len * 2,
            octets: Some(len * 2),
            ..t(-350, "DBCLOB", 2005, 15)
        },
        "SMALLINT" => exact(5, "SMALLINT", 5, 5, 2),
        "INTEGER" => exact(4, "INTEGER", 4, 10, 4),
        "BIGINT" => TypeInfo {
            jdbc: -5,
            ..exact(-5, "BIGINT", 19, 19, 8)
        },
        "DECIMAL" => exact(3, "DECIMAL", 3, len, len + 2),
        "NUMERIC" => exact(2, "NUMERIC", 2, len, len + 2),
        "REAL" => TypeInfo {
            digits: None,
            radix: Some(2),
            ..exact(7, "REAL", 7, 24, 4)
        },
        "FLOAT" if len == 4 => TypeInfo {
            digits: None,
            radix: Some(2),
            ..exact(7, "REAL", 7, 24, 4)
        },
        "DOUBLE" | "FLOAT" => TypeInfo {
            digits: None,
            radix: Some(2),
            ..exact(8, "DOUBLE", 8, 53, 8)
        },
        "DECFLOAT" => TypeInfo {
            jdbc: 1111,
            digits: None,
            size: Some(if len <= 8 { 16 } else { 34 }),
            buffer: if len <= 8 { 8 } else { 16 },
            ..exact(-360, "DECFLOAT", -360, 34, 16)
        },
        "DATE" => dt(91, "DATE", 10, 1, 0),
        "TIME" => dt(92, "TIME", 8, 2, 0),
        "TIMESTMP" | "TIMESTAMP" => {
            let p = scale.unwrap_or(6);
            dt(93, "TIMESTAMP", if p > 0 { 20 + p } else { 19 }, 3, p)
        }
        "ROWID" => t(-100, "ROWID", -8, 24),
        "XML" => t(-370, "XML", 2009, -370),
        "BOOLEAN" => TypeInfo {
            octets: None,
            ..t(16, "BOOLEAN", 16, 16)
        },
        _ => t(1111, "OTHER", 1111, 1111),
    }
}

/// `SYSTABLES.TABLE_TYPE` as `SQLTABLES` names it.
fn table_type(t: &str) -> Option<&'static str> {
    Some(match t {
        "T" | "P" => "TABLE",
        "V" | "L" => "VIEW",
        "A" => "ALIAS",
        "M" => "MATERIALIZED QUERY TABLE",
        _ => return None,
    })
}

/// The constant rows of `SQLTABLETYPES` and `SYSTABLETYPES`.
const TABLE_TYPES: &[(&str, &str)] = &[
    ("ALIAS", "A"),
    ("MATERIALIZED QUERY TABLE", "M"),
    ("SYSTEM TABLE", "?"),
    ("TABLE", "T"),
    ("VIEW", "V"),
];

/// The rows and columns of a view here, or `None` when `name` is not one.
#[allow(clippy::too_many_lines)] // one arm per view
pub fn virtual_table(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    alias: &str,
) -> Result<Option<(Schema, Vec<Row>)>> {
    if !has(name) {
        return Ok(None);
    }
    let c = |n: &str, ty: Ty, null: bool| col(alias, n, ty, null);
    let v128 = Ty::Var(128);
    let (cols, rows): (Vec<ColInfo>, Vec<Row>) = match name.name.value.as_str() {
        "SQLTABLES" | "TABLES" => {
            let t = View::load(txn, ctx, "SYSTABLES")?;
            let views = View::load(txn, ctx, "SYSVIEWS")?;
            let ansi = name.name.value == "TABLES";
            let cols = if ansi {
                vec![
                    c("TABLE_CATALOG", v128, false),
                    c("TABLE_SCHEMA", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("TABLE_TYPE", Ty::Var(24), false),
                    c("SELF_REFERENCING_COLUMN_NAME", v128, true),
                    c("REFERENCE_GENERATION", v128, true),
                    c("USER_DEFINED_TYPE_CATALOG", v128, true),
                    c("USER_DEFINED_TYPE_SCHEMA", v128, true),
                    c("USER_DEFINED_TYPE_NAME", v128, true),
                    c("IS_INSERTABLE_INTO", Ty::Var(3), false),
                ]
            } else {
                vec![
                    c("TABLE_CAT", v128, false),
                    c("TABLE_SCHEM", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("TABLE_TYPE", Ty::Var(24), false),
                    c("REMARKS", Ty::Graphic(2000), true),
                    c("TYPE_CAT", v128, true),
                    c("TYPE_SCHEM", v128, true),
                    c("TYPE_NAME", v128, true),
                    c("SELF_REFERENCING_COL_NAME", v128, true),
                    c("REF_GENERATION", v128, true),
                    c("DBNAME", Ty::Var(8), true),
                    c("TABLE_TEXT", Ty::Graphic(50), false),
                ]
            };
            let rows = t
                .rows
                .iter()
                .filter_map(|r| {
                    let kind = table_type(&t.text(r, "TABLE_TYPE"))?;
                    let (sch, tab) = (t.text(r, "TABLE_SCHEMA"), t.text(r, "TABLE_NAME"));
                    if ansi {
                        let insertable = match kind {
                            "VIEW" => views
                                .rows
                                .iter()
                                .find(|v| {
                                    views.text(v, "TABLE_SCHEMA") == sch
                                        && views.text(v, "TABLE_NAME") == tab
                                })
                                .map_or_else(
                                    || "NO".to_string(),
                                    |v| views.text(v, "IS_INSERTABLE_INTO"),
                                ),
                            _ => "YES".into(),
                        };
                        let kind = if kind == "TABLE" { "BASE TABLE" } else { kind };
                        return Some(vec![
                            s(SERVER),
                            s(sch),
                            s(tab),
                            s(kind),
                            N,
                            N,
                            N,
                            N,
                            N,
                            s(insertable),
                        ]);
                    }
                    Some(vec![
                        s(SERVER),
                        s(sch),
                        s(tab),
                        s(kind),
                        N,
                        N,
                        N,
                        N,
                        N,
                        N,
                        N,
                        s(t.text(r, "TABLE_TEXT")),
                    ])
                })
                .collect();
            (cols, rows)
        }
        "SQLCOLUMNS" | "COLUMNS" => columns(txn, ctx, alias, name.name.value == "COLUMNS")?,
        "SQLPRIMARYKEYS" | "SQLSPECIALCOLUMNS" => {
            let keys = primary_keys(txn, ctx)?;
            if name.name.value == "SQLPRIMARYKEYS" {
                let cols = vec![
                    c("TABLE_CAT", v128, false),
                    c("TABLE_SCHEM", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("COLUMN_NAME", v128, false),
                    c("KEY_SEQ", Ty::Small, false),
                    c("PK_NAME", v128, false),
                ];
                let rows = keys
                    .into_iter()
                    .map(|k| {
                        vec![
                            s(SERVER),
                            s(k.schema),
                            s(k.table),
                            s(k.column),
                            int(k.seq),
                            s(k.name),
                        ]
                    })
                    .collect();
                (cols, rows)
            } else {
                let cols = vec![
                    c("SCOPE", Ty::Small, false),
                    c("COLUMN_NAME", v128, false),
                    c("DATA_TYPE", Ty::Small, false),
                    c("TYPE_NAME", Ty::Var(261), false),
                    c("COLUMN_SIZE", Ty::Int, false),
                    c("BUFFER_LENGTH", Ty::Int, false),
                    c("DECIMAL_DIGITS", Ty::Small, true),
                    c("PSEUDO_COLUMN", Ty::Small, false),
                    c("TABLE_CAT", v128, false),
                    c("TABLE_SCHEM", v128, false),
                    c("TABLE_NAME", v128, false),
                    c("NULLABLE", Ty::Small, false),
                    c("JDBC_DATA_TYPE", Ty::Small, false),
                    c("I_DATA_TYPE", Ty::Small, false),
                ];
                let cview = View::load(txn, ctx, "SYSCOLUMNS")?;
                let rows = keys
                    .into_iter()
                    .filter_map(|k| {
                        let r = cview.rows.iter().find(|r| {
                            cview.text(r, "TABLE_SCHEMA") == k.schema
                                && cview.text(r, "TABLE_NAME") == k.table
                                && cview.text(r, "COLUMN_NAME") == k.column
                        })?;
                        let ti = column_type(&cview, r);
                        Some(vec![
                            int(0),
                            s(k.column),
                            int(ti.odbc),
                            s(ti.name),
                            ti.size.map_or(N, int),
                            int(ti.buffer),
                            ti.digits.map_or(N, int),
                            int(1),
                            s(SERVER),
                            s(k.schema),
                            s(k.table),
                            int(0),
                            int(ti.jdbc),
                            int(ti.internal),
                        ])
                    })
                    .collect();
                (cols, rows)
            }
        }
        "SQLFOREIGNKEYS" => foreign_keys(txn, ctx, alias)?,
        "SQLSCHEMAS" | "SCHEMATA" => {
            let sch = View::load(txn, ctx, "SYSSCHEMAS")?;
            if name.name.value == "SCHEMATA" {
                let cols = vec![
                    c("CATALOG_NAME", v128, false),
                    c("SCHEMA_NAME", v128, false),
                    c("SCHEMA_OWNER", v128, false),
                    c("DEFAULT_CHARACTER_SET_CATALOG", v128, false),
                    c("DEFAULT_CHARACTER_SET_SCHEMA", v128, false),
                    c("DEFAULT_CHARACTER_SET_NAME", v128, false),
                    c("SQL_PATH", Ty::Var(4096), true),
                ];
                let rows = sch
                    .rows
                    .iter()
                    .map(|r| {
                        vec![
                            s(SERVER),
                            s(sch.text(r, "SCHEMA_NAME")),
                            s(sch.text(r, "SCHEMA_OWNER")),
                            s(SERVER),
                            s("SYSIBM"),
                            s("IBM37   "),
                            N,
                        ]
                    })
                    .collect();
                (cols, rows)
            } else {
                let cols = vec![
                    c("TABLE_CAT", v128, false),
                    c("TABLE_SCHEM", v128, false),
                    c("TABLE_NAME", v128, true),
                    c("TABLE_TYPE", v128, true),
                    c("REMARKS", Ty::Graphic(2000), true),
                    c("TYPE_CAT", v128, true),
                    c("TYPE_SCHEM", v128, true),
                    c("TYPE_NAME", v128, true),
                    c("SELF_REFERENCING_COL_NAME", v128, true),
                    c("REF_GENERATION", v128, true),
                    c("DBNAME", Ty::Var(8), true),
                    c("SCHEMA_TEXT", Ty::Graphic(50), true),
                    c("SYSTEM_TABLE_SCHEMA", Ty::Fixed(10), false),
                ];
                let rows = sch
                    .rows
                    .iter()
                    .map(|r| {
                        let text = sch.text(r, "SCHEMA_TEXT");
                        vec![
                            s(SERVER),
                            s(sch.text(r, "SCHEMA_NAME")),
                            N,
                            N,
                            N,
                            N,
                            N,
                            N,
                            N,
                            N,
                            N,
                            if text.is_empty() { N } else { s(text) },
                            s(format!("{:<10}", sch.text(r, "SYSTEM_SCHEMA_NAME"))),
                        ]
                    })
                    .collect();
                (cols, rows)
            }
        }
        "SQLTABLETYPES" => {
            let cols = vec![
                c("TABLE_CAT", v128, false),
                c("TABLE_SCHEM", v128, true),
                c("TABLE_NAME", v128, true),
                c("TABLE_TYPE", v128, false),
                c("REMARKS", Ty::Graphic(2000), true),
                c("TYPE_CAT", v128, true),
                c("TYPE_SCHEM", v128, true),
                c("TYPE_NAME", v128, true),
                c("SELF_REFERENCING_COL_NAME", v128, true),
                c("REF_GENERATION", v128, true),
                c("DBNAME", Ty::Var(8), true),
                c("TYPE", Ty::Fixed(1), false),
            ];
            let rows = TABLE_TYPES
                .iter()
                .map(|(t, k)| vec![s(SERVER), N, N, s(*t), N, N, N, N, N, N, N, s(*k)])
                .collect();
            (cols, rows)
        }
        "SYSTABLETYPES" => {
            let cols = vec![
                c("TABLE_SCHEM", v128, true),
                c("TABLE_NAME", v128, true),
                c("TABLE_TYPE", v128, false),
                c("REMARKS", Ty::Graphic(2000), true),
                c("DBNAME", Ty::Var(8), true),
                c("TYPE", Ty::Fixed(1), false),
            ];
            let rows = TABLE_TYPES
                .iter()
                .map(|(t, k)| vec![N, N, s(*t), N, N, s(*k)])
                .collect();
            (cols, rows)
        }
        "SQLTYPEINFO" => type_rows(alias),
        "SQLSTATISTICS" => statistics(txn, ctx, alias)?,
        "SQLTABLEPRIVILEGES" => {
            let cols = vec![
                c("TABLE_CAT", v128, false),
                c("TABLE_SCHEM", v128, false),
                c("TABLE_NAME", v128, false),
                c("GRANTOR", v128, true),
                c("GRANTEE", v128, false),
                c("PRIVILEGE", Ty::Var(10), false),
                c("IS_GRANTABLE", Ty::Var(3), false),
                c("DBNAME", Ty::Var(8), true),
            ];
            let a = View::load(txn, ctx, "SYSTABAUTH")?;
            // INDEX is not a table privilege here (oracle/sql-catalog/PRIVS).
            let rows = a
                .rows
                .iter()
                .filter(|r| a.text(r, "PRIVILEGE_TYPE") != "INDEX")
                .map(|r| {
                    let grantor = a.text(r, "GRANTOR");
                    vec![
                        s(SERVER),
                        s(a.text(r, "TABLE_SCHEMA")),
                        s(a.text(r, "TABLE_NAME")),
                        if grantor.is_empty() { N } else { s(grantor) },
                        s(a.text(r, "GRANTEE")),
                        s(a.text(r, "PRIVILEGE_TYPE")),
                        s(a.text(r, "IS_GRANTABLE")),
                        N,
                    ]
                })
                .collect();
            (cols, rows)
        }
        "VIEWS" => {
            let cols = vec![
                c("TABLE_CATALOG", v128, false),
                c("TABLE_SCHEMA", v128, false),
                c("TABLE_NAME", v128, false),
                c("VIEW_DEFINITION", Ty::Nclob, true),
                c("CHECK_OPTION", Ty::Var(8), false),
                c("IS_UPDATABLE", Ty::Var(3), false),
            ];
            let v = View::load(txn, ctx, "SYSVIEWS")?;
            let rows = v
                .rows
                .iter()
                .map(|r| {
                    let check = match v.text(r, "CHECK_OPTION").as_str() {
                        "C" => "CASCADED",
                        "L" => "LOCAL",
                        _ => "NONE",
                    };
                    let def = v.at(r, "VIEW_DEFINITION").clone();
                    vec![
                        s(SERVER),
                        s(v.text(r, "TABLE_SCHEMA")),
                        s(v.text(r, "TABLE_NAME")),
                        def,
                        s(check),
                        s(if v.text(r, "IS_UPDATABLE") == "Y" {
                            "YES"
                        } else {
                            "NO"
                        }),
                    ]
                })
                .collect();
            (cols, rows)
        }
        _ => return Ok(None),
    };
    Ok(Some((Schema { cols }, rows)))
}

/// A `SYSCOLUMNS` row's type.
fn column_type(v: &View, r: &Row) -> TypeInfo {
    // A timestamp's precision is DATETIME_PRECISION (its NUMERIC_SCALE is null).
    let ty = v.text(r, "DATA_TYPE");
    let scale = if ty == "TIMESTMP" {
        v.num(r, "DATETIME_PRECISION")
    } else {
        v.num(r, "NUMERIC_SCALE")
    };
    type_info(
        &ty,
        v.num(r, "LENGTH").unwrap_or(0),
        scale,
        v.num(r, "CCSID"),
    )
}

/// The digits of an exact numeric as the information schema gives them:
/// an integer's binary digits (4, 9, 18), a decimal's precision.
fn ansi_precision(t: &TypeInfo, len: i64) -> Option<i64> {
    match t.name {
        "SMALLINT" => Some(4),
        "INTEGER" => Some(9),
        "BIGINT" => Some(18),
        "DECIMAL" | "NUMERIC" => Some(len),
        "REAL" => Some(24),
        "DOUBLE" => Some(53),
        _ => None,
    }
}

/// The information schema's name of a type.
fn ansi_type(t: &TypeInfo) -> &'static str {
    match t.name {
        "CHAR" | "CHAR () FOR BIT DATA" => "CHARACTER",
        "VARCHAR" | "VARCHAR () FOR BIT DATA" => "CHARACTER VARYING",
        "VARGRAPHIC" => "GRAPHIC VARYING",
        "VARBINARY" => "BINARY VARYING",
        "CLOB" => "CHARACTER LARGE OBJECT",
        "BLOB" => "BINARY LARGE OBJECT",
        "DBCLOB" => "DOUBLE-BYTE CHARACTER LARGE OBJECT",
        "DOUBLE" => "DOUBLE PRECISION",
        n => n,
    }
}

#[allow(clippy::too_many_lines)] // the two views' columns
fn columns(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    alias: &str,
    ansi: bool,
) -> Result<(Vec<ColInfo>, Vec<Row>)> {
    let c = |n: &str, ty: Ty, null: bool| col(alias, n, ty, null);
    let v128 = Ty::Var(128);
    let mut v = View::load(txn, ctx, "SYSCOLUMNS")?;
    // An index's columns are not listed: only those of what SYSTABLES lists.
    let t = View::load(txn, ctx, "SYSTABLES")?;
    let listed: std::collections::HashSet<(String, String)> = t
        .rows
        .iter()
        .filter(|r| table_type(&t.text(r, "TABLE_TYPE")).is_some())
        .map(|r| (t.text(r, "TABLE_SCHEMA"), t.text(r, "TABLE_NAME")))
        .collect();
    let rows = std::mem::take(&mut v.rows);
    v.rows = rows
        .into_iter()
        .filter(|r| listed.contains(&(v.text(r, "TABLE_SCHEMA"), v.text(r, "TABLE_NAME"))))
        .collect();
    let v = v;
    let default = |r: &Row| match v.at(r, "COLUMN_DEFAULT") {
        Value::Null => N,
        x if x.as_text().trim() == "NULL" => N,
        x => s(x.as_text().trim_end()),
    };
    if ansi {
        let mut cols = vec![
            c("TABLE_CATALOG", v128, false),
            c("TABLE_SCHEMA", v128, false),
            c("TABLE_NAME", v128, false),
            c("COLUMN_NAME", v128, false),
            c("ORDINAL_POSITION", Ty::Int, false),
            c("COLUMN_DEFAULT", Ty::Graphic(2000), true),
            c("IS_NULLABLE", Ty::Var(3), false),
            c("DATA_TYPE", v128, false),
        ];
        for n in [
            "CHARACTER_MAXIMUM_LENGTH",
            "CHARACTER_OCTET_LENGTH",
            "NUMERIC_PRECISION",
            "NUMERIC_PRECISION_RADIX",
            "NUMERIC_SCALE",
            "DATETIME_PRECISION",
        ] {
            cols.push(c(n, Ty::Int, true));
        }
        cols.push(c("INTERVAL_TYPE", v128, true));
        cols.push(c("INTERVAL_PRECISION", Ty::Int, true));
        for n in [
            "CHARACTER_SET_CATALOG",
            "CHARACTER_SET_SCHEMA",
            "CHARACTER_SET_NAME",
            "COLLATION_CATALOG",
            "COLLATION_SCHEMA",
            "COLLATION_NAME",
            "DOMAIN_CATALOG",
            "DOMAIN_SCHEMA",
            "DOMAIN_NAME",
            "UDT_CATALOG",
            "UDT_SCHEMA",
            "UDT_NAME",
            "SCOPE_CATALOG",
            "SCOPE_SCHEMA",
            "SCOPE_NAME",
        ] {
            cols.push(c(n, v128, true));
        }
        cols.push(c("MAXIMUM_CARDINALITY", Ty::Int, true));
        cols.push(c("DTD_IDENTIFIER", v128, true));
        cols.push(c("IS_SELF_REFERENCING", Ty::Var(3), false));
        let rows = v
            .rows
            .iter()
            .enumerate()
            .map(|(k, r)| {
                let ti = column_type(&v, r);
                let len = v.num(r, "LENGTH").unwrap_or(0);
                let charlike = ti.octets.is_some() && ti.radix.is_none();
                let text_set =
                    charlike && !matches!(ti.jdbc, -2 | -3 | 2004) && v.num(r, "CCSID") == Some(37);
                let datetime = ti.datetime_sub.map(|_| ti.digits.unwrap_or(0));
                let mut row = vec![
                    s(SERVER),
                    s(v.text(r, "TABLE_SCHEMA")),
                    s(v.text(r, "TABLE_NAME")),
                    s(v.text(r, "COLUMN_NAME")),
                    v.at(r, "ORDINAL_POSITION").clone(),
                    default(r),
                    s(if v.text(r, "IS_NULLABLE") == "Y" {
                        "YES"
                    } else {
                        "NO"
                    }),
                    s(ansi_type(&ti)),
                    if charlike { int(len) } else { N },
                    if charlike {
                        ti.octets.map_or(N, int)
                    } else {
                        N
                    },
                    if ti.radix.is_some() {
                        ansi_precision(&ti, len).map_or(N, int)
                    } else {
                        N
                    },
                    ti.radix.map_or(N, int),
                    if ti.radix == Some(10) {
                        ti.digits.map_or(N, int)
                    } else {
                        N
                    },
                    datetime.map_or(N, int),
                    N,
                    N,
                ];
                if text_set {
                    row.extend([
                        s(SERVER),
                        s("SYSIBM"),
                        s("IBM37   "),
                        s(SERVER),
                        s("SYSIBM"),
                        s("IBMDEFAULT"),
                    ]);
                } else {
                    row.extend([N, N, N, N, N, N]);
                }
                row.extend([N, N, N, N, N, N, N, N, N, N]);
                row.push(s(format!("C{:015}", k + 1)));
                row.push(s("NO"));
                row
            })
            .collect();
        return Ok((cols, rows));
    }
    let cols = vec![
        c("TABLE_CAT", v128, false),
        c("TABLE_SCHEM", v128, false),
        c("TABLE_NAME", v128, false),
        c("COLUMN_NAME", v128, false),
        c("DATA_TYPE", Ty::Small, false),
        c("TYPE_NAME", Ty::Var(261), false),
        c("COLUMN_SIZE", Ty::Int, false),
        c("BUFFER_LENGTH", Ty::Int, false),
        c("DECIMAL_DIGITS", Ty::Small, true),
        c("NUM_PREC_RADIX", Ty::Small, true),
        c("NULLABLE", Ty::Small, false),
        c("REMARKS", Ty::Graphic(2000), true),
        c("COLUMN_DEF", Ty::Graphic(2000), true),
        c("SQL_DATA_TYPE", Ty::Small, false),
        c("SQL_DATETIME_SUB", Ty::Small, true),
        c("CHAR_OCTET_LENGTH", Ty::Int, true),
        c("ORDINAL_POSITION", Ty::Int, false),
        c("IS_NULLABLE", Ty::Var(3), false),
        c("JDBC_DATA_TYPE", Ty::Small, false),
        c("SCOPE_CATALOG", v128, true),
        c("SCOPE_SCHEMA", v128, true),
        c("SCOPE_TABLE", v128, true),
        c("SOURCE_DATA_TYPE", Ty::Small, true),
        c("DBNAME", Ty::Var(8), true),
        c("PSEUDO_COLUMN", Ty::Small, false),
        c("COLUMN_TEXT", Ty::Graphic(50), true),
        c("SYSTEM_COLUMN_NAME", Ty::Fixed(10), false),
        c("I_DATA_TYPE", Ty::Small, false),
    ];
    let rows = v
        .rows
        .iter()
        .map(|r| {
            let ti = column_type(&v, r);
            let nullable = v.text(r, "IS_NULLABLE") == "Y";
            let text = v.text(r, "COLUMN_TEXT");
            vec![
                s(SERVER),
                s(v.text(r, "TABLE_SCHEMA")),
                s(v.text(r, "TABLE_NAME")),
                s(v.text(r, "COLUMN_NAME")),
                int(ti.odbc),
                s(ti.name),
                ti.size.map_or(N, int),
                int(ti.buffer),
                ti.digits.map_or(N, int),
                ti.radix.map_or(N, int),
                int(i64::from(nullable)),
                N,
                default(r),
                int(if ti.datetime_sub.is_some() {
                    9
                } else {
                    ti.odbc
                }),
                ti.datetime_sub.map_or(N, int),
                ti.octets
                    .filter(|_| ti.radix.is_none() && ti.datetime_sub.is_none())
                    .map_or(N, int),
                v.at(r, "ORDINAL_POSITION").clone(),
                s(if nullable { "YES" } else { "NO" }),
                int(ti.jdbc),
                N,
                N,
                N,
                N,
                N,
                int(1),
                if text.is_empty() { N } else { s(text) },
                s(format!("{:<10}", v.text(r, "SYSTEM_COLUMN_NAME"))),
                int(ti.internal),
            ]
        })
        .collect();
    Ok((cols, rows))
}

/// One column of a primary key.
struct KeyCol {
    schema: String,
    table: String,
    column: String,
    seq: i64,
    name: String,
}

/// The columns of a constraint of one of `types`, in key order.
fn constraint_cols(txn: &mut Txn<'_>, ctx: &SqlContext, types: &[&str]) -> Result<Vec<KeyCol>> {
    let cst = View::load(txn, ctx, "SYSCST")?;
    let keys = View::load(txn, ctx, "SYSKEYCST")?;
    let mut out = Vec::new();
    for r in &cst.rows {
        if !types.contains(&cst.text(r, "CONSTRAINT_TYPE").as_str()) {
            continue;
        }
        let (cs, cn) = (
            cst.text(r, "CONSTRAINT_SCHEMA"),
            cst.text(r, "CONSTRAINT_NAME"),
        );
        let mut cols: Vec<KeyCol> = keys
            .rows
            .iter()
            .filter(|k| {
                keys.text(k, "CONSTRAINT_SCHEMA") == cs && keys.text(k, "CONSTRAINT_NAME") == cn
            })
            .map(|k| KeyCol {
                schema: cst.text(r, "TABLE_SCHEMA"),
                table: cst.text(r, "TABLE_NAME"),
                column: keys.text(k, "COLUMN_NAME"),
                seq: keys.num(k, "ORDINAL_POSITION").unwrap_or(0),
                name: cn.clone(),
            })
            .collect();
        cols.sort_by_key(|k| k.seq);
        out.extend(cols);
    }
    Ok(out)
}

fn primary_keys(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Vec<KeyCol>> {
    constraint_cols(txn, ctx, &["PRIMARY KEY"])
}

/// A referential rule as JDBC numbers it.
fn rule(r: &str) -> i64 {
    match r {
        "CASCADE" => 0,
        "RESTRICT" => 1,
        "SET NULL" => 2,
        "SET DEFAULT" => 4,
        _ => 3,
    }
}

fn foreign_keys(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    alias: &str,
) -> Result<(Vec<ColInfo>, Vec<Row>)> {
    let c = |n: &str, ty: Ty, null: bool| col(alias, n, ty, null);
    let v128 = Ty::Var(128);
    let mut cols = Vec::new();
    for n in [
        "PKTABLE_CAT",
        "PKTABLE_SCHEM",
        "PKTABLE_NAME",
        "PKCOLUMN_NAME",
        "FKTABLE_CAT",
        "FKTABLE_SCHEM",
        "FKTABLE_NAME",
        "FKCOLUMN_NAME",
    ] {
        cols.push(c(n, v128, false));
    }
    for n in ["KEY_SEQ", "UPDATE_RULE", "DELETE_RULE"] {
        cols.push(c(n, Ty::Small, false));
    }
    cols.push(c("FK_NAME", v128, false));
    cols.push(c("PK_NAME", v128, true));
    cols.push(c("DEFERRABILITY", Ty::Small, false));
    cols.push(c("UNIQUE_OR_PRIMARY", Ty::Fixed(7), false));
    let refs = View::load(txn, ctx, "SYSREFCST")?;
    let uniques = constraint_cols(txn, ctx, &["PRIMARY KEY", "UNIQUE"])?;
    let primary: Vec<String> = primary_keys(txn, ctx)?
        .into_iter()
        .map(|k| k.name)
        .collect();
    let fks = constraint_cols(txn, ctx, &["FOREIGN KEY"])?;
    let mut rows = Vec::new();
    for r in &refs.rows {
        let (fk, pk) = (
            refs.text(r, "CONSTRAINT_NAME"),
            refs.text(r, "UNIQUE_CONSTRAINT_NAME"),
        );
        let parent: Vec<&KeyCol> = uniques.iter().filter(|k| k.name == pk).collect();
        for f in fks.iter().filter(|k| k.name == fk) {
            let Some(p) = parent.iter().find(|p| p.seq == f.seq) else {
                continue;
            };
            rows.push(vec![
                s(SERVER),
                s(&p.schema),
                s(&p.table),
                s(&p.column),
                s(SERVER),
                s(&f.schema),
                s(&f.table),
                s(&f.column),
                int(f.seq),
                int(rule(&refs.text(r, "UPDATE_RULE"))),
                int(rule(&refs.text(r, "DELETE_RULE"))),
                s(&fk),
                s(&pk),
                int(7),
                s(if primary.contains(&pk) {
                    "PRIMARY"
                } else {
                    "UNIQUE "
                }),
            ]);
        }
    }
    Ok((cols, rows))
}

fn statistics(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    alias: &str,
) -> Result<(Vec<ColInfo>, Vec<Row>)> {
    let c = |n: &str, ty: Ty, null: bool| col(alias, n, ty, null);
    let v128 = Ty::Var(128);
    let cols = vec![
        c("TABLE_CAT", v128, false),
        c("TABLE_SCHEM", v128, false),
        c("TABLE_NAME", v128, false),
        c("NON_UNIQUE", Ty::Small, true),
        c("INDEX_QUALIFIER", v128, true),
        c("INDEX_NAME", v128, true),
        c("TYPE", Ty::Small, false),
        c("ORDINAL_POSITION", Ty::Small, true),
        c("COLUMN_NAME", Ty::Nclob, true),
        c("ASC_OR_DESC", Ty::Fixed(1), true),
        c("CARDINALITY", Ty::Big, true),
        c("PAGES", Ty::Big, true),
        c("FILTER_CONDITION", Ty::Graphic(2000), true),
        c("I_INDEXTYPE", Ty::Int, false),
    ];
    let mut rows = Vec::new();
    // The tables' primary keys: their own access paths.
    for k in primary_keys(txn, ctx)? {
        rows.push(vec![
            s(SERVER),
            s(&k.schema),
            s(&k.table),
            int(0),
            s(&k.schema),
            s(&k.table),
            int(3),
            int(k.seq),
            s(&k.column),
            s("A"),
            N,
            N,
            N,
            int(1),
        ]);
    }
    let idx = View::load(txn, ctx, "SYSINDEXES")?;
    let keys = View::load(txn, ctx, "SYSKEYS")?;
    for r in &idx.rows {
        let (is, iname) = (idx.text(r, "INDEX_SCHEMA"), idx.text(r, "INDEX_NAME"));
        let unique = matches!(idx.text(r, "IS_UNIQUE").as_str(), "U" | "P");
        let mut ks: Vec<&Row> = keys
            .rows
            .iter()
            .filter(|k| keys.text(k, "INDEX_SCHEMA") == is && keys.text(k, "INDEX_NAME") == iname)
            .collect();
        ks.sort_by_key(|k| keys.num(k, "ORDINAL_POSITION").unwrap_or(0));
        for (n, k) in ks.into_iter().enumerate() {
            let order = if keys.text(k, "ORDERING") == "D" {
                "D"
            } else {
                "A"
            };
            rows.push(vec![
                s(SERVER),
                s(idx.text(r, "TABLE_SCHEMA")),
                s(idx.text(r, "TABLE_NAME")),
                int(i64::from(!unique)),
                s(&is),
                s(&iname),
                int(3),
                int(i64::try_from(n).unwrap_or(0) + 1),
                s(keys.text(k, "COLUMN_NAME")),
                s(order),
                N,
                N,
                N,
                int(2),
            ]);
        }
    }
    // One row of each table's statistics.
    let t = View::load(txn, ctx, "SYSTABLES")?;
    let stat = View::load(txn, ctx, "SYSTABLESTAT")?;
    for r in &t.rows {
        if !matches!(t.text(r, "TABLE_TYPE").as_str(), "T" | "P") {
            continue;
        }
        let (sch, tab) = (t.text(r, "TABLE_SCHEMA"), t.text(r, "TABLE_NAME"));
        let count = stat
            .rows
            .iter()
            .find(|x| stat.text(x, "TABLE_SCHEMA") == sch && stat.text(x, "TABLE_NAME") == tab)
            .and_then(|x| stat.num(x, "NUMBER_ROWS"))
            .unwrap_or(0);
        rows.push(vec![
            s(SERVER),
            s(sch),
            s(tab),
            N,
            N,
            N,
            int(0),
            N,
            N,
            N,
            int(count),
            int(1),
            N,
            int(0),
        ]);
    }
    Ok((cols, rows))
}

/// `SQLTYPEINFO`: the primary's rows.
#[allow(clippy::type_complexity, clippy::items_after_statements)] // one tuple per row
fn type_rows(alias: &str) -> (Vec<ColInfo>, Vec<Row>) {
    let c = |n: &str, ty: Ty, null: bool| col(alias, n, ty, null);
    let v128 = Ty::Var(128);
    let cols = vec![
        c("TYPE_NAME", v128, false),
        c("DATA_TYPE", Ty::Small, false),
        c("COLUMN_SIZE", Ty::Int, true),
        c("LITERAL_PREFIX", v128, true),
        c("LITERAL_SUFFIX", v128, true),
        c("CREATE_PARAMS", v128, true),
        c("NULLABLE", Ty::Small, true),
        c("CASE_SENSITIVE", Ty::Small, true),
        c("SEARCHABLE", Ty::Small, true),
        c("UNSIGNED_ATTRIBUTE", Ty::Small, true),
        c("FIXED_PREC_SCALE", Ty::Small, true),
        c("AUTO_UNIQUE_VALUE", Ty::Small, true),
        c("LOCAL_TYPE_NAME", v128, true),
        c("MINIMUM_SCALE", Ty::Small, true),
        c("MAXIMUM_SCALE", Ty::Small, true),
        c("SQL_DATA_TYPE", Ty::Small, true),
        c("SQL_DATETIME_SUB", Ty::Small, true),
        c("NUM_PREC_RADIX", Ty::Int, true),
        c("INTERVAL_PRECISION", Ty::Small, true),
        c("JDBC_DATA_TYPE", Ty::Small, false),
        c("I_DATA_TYPE", Ty::Small, false),
    ];
    // name, type, size, prefix, suffix, params, nullable, case, searchable,
    // unsigned, fixed, auto, min scale, max scale, sql type, sub, radix, jdbc, internal.
    type T = (
        &'static str,
        i64,
        Option<i64>,
        Option<&'static str>,
        Option<&'static str>,
        Option<&'static str>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        i64,
        i64,
    );
    let q = Some("'");
    let len = Some("length");
    let ps = Some("precision,scale");
    let num = |n, t, size, us, fixed, max: i64, radix, jdbc, int_| -> T {
        (
            n,
            t,
            Some(size),
            None,
            None,
            None,
            Some(1),
            Some(0),
            Some(2),
            Some(us),
            Some(fixed),
            Some(0),
            Some(0),
            Some(max),
            Some(t),
            None,
            Some(radix),
            jdbc,
            int_,
        )
    };
    let chars = |n, t, size, pre, case, jdbc, int_| -> T {
        (
            n,
            t,
            Some(size),
            pre,
            q,
            len,
            Some(1),
            Some(case),
            Some(3),
            None,
            Some(0),
            Some(0),
            None,
            None,
            Some(t),
            None,
            None,
            jdbc,
            int_,
        )
    };
    let bins = |n, t, size, pre, suf, jdbc, int_| -> T {
        (
            n,
            t,
            Some(size),
            pre,
            suf,
            len,
            Some(1),
            Some(0),
            Some(3),
            None,
            Some(0),
            Some(0),
            None,
            None,
            Some(t),
            None,
            None,
            jdbc,
            int_,
        )
    };
    let dt = |n, t, size, min, max, sub| -> T {
        (
            n,
            t,
            Some(size),
            q,
            q,
            None,
            Some(1),
            Some(0),
            Some(2),
            None,
            Some(0),
            Some(0),
            min,
            max,
            Some(9),
            Some(sub),
            None,
            t,
            t,
        )
    };
    let rows: Vec<T> = vec![
        (
            "XML",
            -370,
            Some(2_147_483_647),
            q,
            q,
            len,
            Some(1),
            Some(1),
            Some(3),
            None,
            None,
            None,
            None,
            None,
            Some(-370),
            None,
            None,
            2009,
            -370,
        ),
        (
            "DECFLOAT",
            -360,
            Some(34),
            None,
            None,
            Some("precision"),
            Some(1),
            Some(0),
            Some(2),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(34),
            Some(-360),
            None,
            Some(10),
            1111,
            -360,
        ),
        chars("DBCLOB", -350, 1_073_741_823, q, 1, 2005, 15),
        (
            "ROWID",
            -100,
            Some(40),
            None,
            None,
            None,
            Some(0),
            Some(0),
            Some(2),
            None,
            Some(0),
            Some(0),
            None,
            None,
            Some(-100),
            None,
            None,
            -8,
            24,
        ),
        chars("CLOB", -99, 2_147_483_647, q, 1, 2005, 14),
        bins(
            "BLOB",
            -98,
            2_147_483_647,
            Some("BLOB(X'"),
            Some("')"),
            2004,
            13,
        ),
        chars("VARGRAPHIC", -96, 16369, q, 1, 12, 96),
        chars("GRAPHIC", -95, 16382, q, 1, 1, 95),
        chars("NCLOB", -10, 1_073_741_823, Some("N'"), 1, 2005, 15),
        chars("NVARCHAR", -9, 16369, Some("N'"), 1, 12, 96),
        chars("NCHAR", -8, 16382, Some("N'"), 1, 1, 95),
        num("BIGINT", -5, 19, 0, 1, 0, 10, -5, 19),
        bins(
            "VARBINARY",
            -3,
            32739,
            Some("VARBINARY(X'"),
            Some("')"),
            -3,
            -3,
        ),
        bins("VARCHAR () FOR BIT DATA", -3, 32739, Some("X'"), q, -3, -3),
        bins("BINARY", -2, 32765, Some("BINARY(X'"), Some("')"), -2, -2),
        bins("CHAR () FOR BIT DATA", -2, 32765, Some("X'"), q, -2, -2),
        chars("CHAR", 1, 32765, q, 1, 1, 1),
        (
            "NUMERIC",
            2,
            Some(63),
            None,
            None,
            ps,
            Some(1),
            Some(0),
            Some(2),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(63),
            Some(2),
            None,
            Some(10),
            2,
            2,
        ),
        (
            "DECIMAL",
            3,
            Some(63),
            None,
            None,
            ps,
            Some(1),
            Some(0),
            Some(2),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
            Some(63),
            Some(3),
            None,
            Some(10),
            3,
            3,
        ),
        num("INTEGER", 4, 10, 0, 1, 0, 10, 4, 4),
        num("SMALLINT", 5, 5, 0, 1, 0, 10, 5, 5),
        (
            "REAL",
            7,
            Some(24),
            None,
            None,
            None,
            Some(1),
            Some(0),
            Some(2),
            Some(0),
            Some(0),
            Some(0),
            None,
            None,
            Some(7),
            None,
            Some(2),
            7,
            7,
        ),
        (
            "DOUBLE",
            8,
            Some(53),
            None,
            None,
            None,
            Some(1),
            Some(0),
            Some(2),
            Some(0),
            Some(0),
            Some(0),
            None,
            None,
            Some(8),
            None,
            Some(2),
            8,
            8,
        ),
        chars("VARCHAR", 12, 32739, q, 1, 12, 12),
        (
            "DATALINK",
            70,
            Some(32717),
            None,
            None,
            len,
            Some(1),
            Some(0),
            Some(0),
            None,
            Some(0),
            Some(0),
            None,
            None,
            Some(70),
            None,
            None,
            70,
            16,
        ),
        dt("DATE", 91, 10, None, None, 1),
        dt("TIME", 92, 8, Some(0), Some(0), 2),
        dt("TIMESTAMP", 93, 26, Some(6), Some(6), 3),
        (
            "DISTINCT", 2001, None, None, None, None, None, None, None, None, None, None, None,
            None, None, None, None, 2001, 2001,
        ),
    ];
    let o = |v: Option<i64>| v.map_or(N, int);
    let t = |v: Option<&str>| v.map_or(N, s);
    let rows = rows
        .into_iter()
        .map(|r| {
            vec![
                s(r.0),
                int(r.1),
                o(r.2),
                t(r.3),
                t(r.4),
                t(r.5),
                o(r.6),
                o(r.7),
                o(r.8),
                o(r.9),
                o(r.10),
                o(r.11),
                N,
                o(r.12),
                o(r.13),
                o(r.14),
                o(r.15),
                o(r.16),
                N,
                int(r.17),
                int(r.18),
            ]
        })
        .collect();
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_are_described_as_the_primary_describes_them() {
        let i = type_info("INTEGER", 4, Some(0), None);
        assert_eq!(
            (i.odbc, i.size, i.buffer, i.digits, i.radix, i.jdbc),
            (4, Some(10), 4, Some(0), Some(10), 4)
        );
        let d = type_info("DECIMAL", 9, Some(2), None);
        assert_eq!((d.size, d.buffer, d.digits), (Some(9), 11, Some(2)));
        let dt = type_info("DATE", 4, None, None);
        assert_eq!(
            (dt.odbc, dt.size, dt.buffer, dt.digits, dt.datetime_sub),
            (91, Some(10), 6, Some(0), Some(1))
        );
        let c = type_info("CHAR", 4, None, Some(37));
        assert_eq!((c.odbc, c.name, c.octets), (1, "CHAR", Some(4)));
        assert_eq!(
            type_info("CHAR", 5, None, Some(65535)).name,
            "CHAR () FOR BIT DATA"
        );
    }
}

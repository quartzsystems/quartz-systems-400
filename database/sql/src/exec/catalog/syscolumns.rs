// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSCOLUMNS`: IBM's 7.1 columns in their order, a row per column
//! of every table, view and file. Lengths, storage, CCSIDs, defaults and
//! headings are as the primary shows them (`tools/conformance/oracle/
//! sql-catalog/`).

use db::file::{FileDesc, FileKind};
use db::sqlinf::seq::ColumnKind;
use dds::{DataType, DefaultValue, FieldDef};
use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use super::super::eval::{ColInfo, Row};
use super::sys::{Meta, col, dec, n, nclob, nv, s, type_of, vc};
use crate::error::Result;
use crate::value::Value;

/// The columns.
pub(super) fn columns(alias: &str) -> Vec<ColInfo> {
    let c = |nm: &str, ty: DataType, len: u32, nullable: bool| col(alias, nm, ty, len, 0, nullable);
    let int = |nm: &str, nullable: bool| col(alias, nm, DataType::Packed, 9, 0, nullable);
    vec![
        vc(alias, "COLUMN_NAME", 128, false),
        vc(alias, "TABLE_NAME", 128, false),
        vc(alias, "TABLE_OWNER", 128, false),
        int("ORDINAL_POSITION", false),
        vc(alias, "DATA_TYPE", 8, false),
        int("LENGTH", false),
        int("NUMERIC_SCALE", true),
        c("IS_NULLABLE", DataType::Char, 1, false),
        c("IS_UPDATABLE", DataType::Char, 1, false),
        nv(alias, "LONG_COMMENT", 2000, true),
        c("HAS_DEFAULT", DataType::Char, 1, false),
        nv(alias, "COLUMN_HEADING", 60, true),
        int("STORAGE", false),
        int("NUMERIC_PRECISION", true),
        int("CCSID", true),
        vc(alias, "TABLE_SCHEMA", 128, false),
        nv(alias, "COLUMN_DEFAULT", 2000, true),
        int("CHARACTER_MAXIMUM_LENGTH", true),
        int("CHARACTER_OCTET_LENGTH", true),
        int("NUMERIC_PRECISION_RADIX", true),
        int("DATETIME_PRECISION", true),
        nv(alias, "COLUMN_TEXT", 50, true),
        c("SYSTEM_COLUMN_NAME", DataType::Char, 10, false),
        c("SYSTEM_TABLE_NAME", DataType::Char, 10, false),
        c("SYSTEM_TABLE_SCHEMA", DataType::Char, 10, false),
        vc(alias, "USER_DEFINED_TYPE_SCHEMA", 128, true),
        vc(alias, "USER_DEFINED_TYPE_NAME", 128, true),
        vc(alias, "IS_IDENTITY", 3, false),
        vc(alias, "IDENTITY_GENERATION", 10, true),
        dec(alias, "IDENTITY_START", 31, true),
        dec(alias, "IDENTITY_INCREMENT", 31, true),
        dec(alias, "IDENTITY_MINIMUM", 31, true),
        dec(alias, "IDENTITY_MAXIMUM", 31, true),
        vc(alias, "IDENTITY_CYCLE", 3, true),
        int("IDENTITY_CACHE", true),
        vc(alias, "IDENTITY_ORDER", 3, true),
        nclob(alias, "COLUMN_EXPRESSION", 1200),
        vc(alias, "HIDDEN", 1, false),
        vc(alias, "HAS_FLDPROC", 1, false),
    ]
}

/// What `SYSCOLUMNS` says of a field's type.
struct Shape {
    length: u64,
    scale: Option<u64>,
    precision: Option<u64>,
    radix: Option<u64>,
    ccsid: Option<u64>,
    storage: u64,
    chars: Option<(u64, u64)>,
    datetime: Option<u64>,
}

/// The shape of `f` as the primary describes it.
fn shape(f: &FieldDef) -> Shape {
    let len = u64::from(f.length);
    let var = if f.varlen.is_some() { 2 } else { 0 };
    let ccsid = |dft: u16| Some(u64::from(f.ccsid.unwrap_or(dft)));
    let num = |length: u64, scale: Option<u64>, precision: u64, radix: u64, storage: u64| Shape {
        length,
        scale,
        precision: Some(precision),
        radix: Some(radix),
        ccsid: None,
        storage,
        chars: None,
        datetime: None,
    };
    let chr = |ccsid: Option<u64>, storage: u64, octets: u64| Shape {
        length: len,
        scale: None,
        precision: None,
        radix: None,
        ccsid,
        storage,
        chars: Some((len, octets)),
        datetime: None,
    };
    let dt = |length: u64, precision: u64| Shape {
        length,
        scale: None,
        precision: None,
        radix: None,
        ccsid: ccsid(37),
        storage: length,
        chars: None,
        datetime: Some(precision),
    };
    let dec = u64::from(f.decimals);
    match f.data_type {
        // An SQL integer's length is its bytes, its precision its digits.
        DataType::Binary => {
            let bytes = u64::from(f.data_len());
            num(
                if f.int_range { bytes } else { len },
                Some(dec),
                len,
                10,
                bytes,
            )
        }
        DataType::Packed => num(len, Some(dec), len, 10, len / 2 + 1),
        DataType::Zoned => num(len, Some(dec), len, 10, len),
        DataType::Float => {
            let single = f.precision == dds::FloatPrecision::Single;
            let bytes = if single { 4 } else { 8 };
            num(bytes, None, if single { 24 } else { 52 }, 2, bytes)
        }
        DataType::DecFloat => {
            let (bytes, digits) = if f.length <= 16 { (8, 16) } else { (16, 34) };
            num(bytes, None, digits, 10, bytes)
        }
        DataType::Date => dt(4, 0),
        DataType::Time => dt(3, 0),
        DataType::Timestamp => {
            let p = u64::from(DataType::timestamp_precision(f.length));
            dt(7 + p.div_ceil(2), p)
        }
        DataType::Char => chr(ccsid(37), len + var, len),
        DataType::Hex | DataType::Bytes => chr(Some(65535), len + var, len),
        DataType::Graphic | DataType::DbcsOpen => chr(ccsid(13488), 2 * len + var, 2 * len),
        DataType::Blob => chr(Some(65535), len + 29, len),
        DataType::Clob => chr(ccsid(37), len + 29, len),
        DataType::Dbclob => chr(ccsid(1200), len / 2 + 14, 2 * len),
        DataType::Boolean => Shape {
            length: 1,
            scale: None,
            precision: None,
            radix: None,
            ccsid: None,
            storage: 1,
            chars: None,
            datetime: None,
        },
    }
}

fn opt(v: Option<u64>) -> Value {
    v.map_or(Value::Null, n)
}

/// A heading as the catalog shows it: its lines of twenty characters
/// one after another, trailing blanks dropped.
fn heading(f: &FieldDef) -> Value {
    let mut out = String::new();
    for (i, line) in f.colhdg.iter().enumerate() {
        out.push_str(line);
        if i + 1 < f.colhdg.len() {
            out.extend(std::iter::repeat_n(
                ' ',
                20usize.saturating_sub(line.chars().count()),
            ));
        }
    }
    let out = out.trim_end();
    if out.is_empty() { Value::Null } else { s(out) }
}

/// A default as `COLUMN_DEFAULT` writes it: special registers with `_`.
fn default_text(d: &DefaultValue) -> String {
    let t = super::sys::default_text(d);
    match d {
        DefaultValue::Expr(_) => t
            .strip_prefix("CURRENT ")
            .map_or(t.clone(), |r| format!("CURRENT_{r}")),
        _ => t,
    }
}

/// The default a file DDS made has without `DFT`.
fn dds_default(f: &FieldDef) -> Option<&'static str> {
    match f.data_type {
        t if t.is_numeric() => Some("0"),
        DataType::Char | DataType::Graphic | DataType::Hex => Some("' '"),
        _ => None,
    }
}

/// The rows: every column of every file in `all`.
#[allow(clippy::too_many_lines)]
pub(super) fn rows(
    txn: &mut Txn<'_>,
    all: &[(Name, FileDesc)],
    meta: &std::collections::HashMap<ObjectId, Meta>,
) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    for (lib, d) in all {
        let Some(fmt) = d.formats.first() else {
            continue;
        };
        let m = meta.get(&d.id).cloned().unwrap_or_default();
        let kind = type_of(Some(&m), d);
        let view = kind == "V";
        let by_sql = m.kind.is_some();
        // Columns of a distinct type.
        let typed = if crate::exec::udt::registry::any() {
            crate::exec::udt::column::of(txn, d.id)?
        } else {
            Vec::new()
        };
        let generated = if by_sql {
            db::sqlinf::seq::columns(txn, d.id).unwrap_or_default()
        } else {
            Vec::new()
        };
        let updatable = if view {
            match crate::exec::view::definition(txn, d.id) {
                Some((v, _)) => !v.read_only,
                None => !fmt.logical.as_ref().is_some_and(|l| l.join),
            }
        } else {
            true
        };
        let owner = ccsid::decode_trimmed(&d.header.owner);
        for (i, f) in fmt.format.fields.iter().enumerate() {
            let udt = typed.iter().find(|c| c.field == f.name);
            let g = generated.iter().find(|c| c.field == f.name);
            let sh = shape(f);
            let (has_default, default) = match (&f.default, g.map(|g| &g.kind)) {
                (_, Some(ColumnKind::Identity(_))) => ("I", Value::Null),
                (_, Some(ColumnKind::RowChange(_))) => ("E", Value::Null),
                _ if view => ("N", Value::Null),
                (Some(dft), _) => ("Y", s(default_text(dft))),
                (None, _) if !by_sql && d.attr.kind != FileKind::Logical => {
                    ("Y", dds_default(f).map_or(Value::Null, s))
                }
                (None, _) if f.allow_null => ("Y", Value::Null),
                (None, _) => ("N", Value::Null),
            };
            let identity = match g.map(|g| &g.kind) {
                Some(ColumnKind::Identity(a)) => Some(a),
                _ => None,
            };
            let generation = match g.map(|g| &g.kind) {
                Some(ColumnKind::Identity(a)) => Some(a.always),
                Some(ColumnKind::RowChange(always) | ColumnKind::Audit(always, _)) => Some(*always),
                _ => None,
            };
            let big = |v: i128| Value::Num(qdecimal::Decimal::new(v, 0).expect("fits"));
            let yes_no = |b: bool| s(if b { "YES" } else { "NO " });
            let column = f.alias.clone().unwrap_or_else(|| f.name.to_string());
            rows.push(vec![
                s(column),
                s(m.sql.clone()),
                s(owner.clone()),
                n(i as u64 + 1),
                s(if udt.is_some() {
                    "DISTINCT".into()
                } else {
                    super::sys::data_type_name(f)
                }),
                n(sh.length),
                opt(sh.scale),
                s(if f.allow_null { "Y" } else { "N" }),
                s(if updatable { "Y" } else { "N" }),
                m.comment(Some(f.name)),
                s(has_default),
                heading(f),
                n(sh.storage),
                opt(sh.precision),
                opt(sh.ccsid),
                s(m.schema.clone()),
                default,
                opt(sh.chars.map(|c| c.0)),
                opt(sh.chars.map(|c| c.1)),
                opt(sh.radix),
                opt(sh.datetime),
                f.text
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .map_or(Value::Null, |t| s(t.trim_end())),
                s(f.name.to_string()),
                s(d.name().to_string()),
                s(lib.to_string()),
                udt.map_or(Value::Null, |c| {
                    s(c.library.to_string().trim_end().to_string())
                }),
                udt.map_or(Value::Null, |c| s(c.name.clone())),
                s(if identity.is_some() { "YES" } else { "NO" }),
                generation.map_or(Value::Null, |a| s(if a { "ALWAYS" } else { "BY DEFAULT" })),
                identity.map_or(Value::Null, |a| big(a.start)),
                identity.map_or(Value::Null, |a| big(a.increment)),
                identity.map_or(Value::Null, |a| big(a.min)),
                identity.map_or(Value::Null, |a| big(a.max)),
                identity.map_or(Value::Null, |a| yes_no(a.cycle)),
                identity.map_or(Value::Null, |a| n(u64::from(a.cache))),
                identity.map_or(Value::Null, |a| yes_no(a.order)),
                Value::Null,
                s(if g.is_some_and(|g| g.hidden) {
                    "I"
                } else {
                    "N"
                }),
                s("N"),
            ]);
        }
    }
    Ok(rows)
}

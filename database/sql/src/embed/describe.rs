// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DESCRIBE`: a prepared statement's result columns, or with `INPUT`
//! its parameter markers, as SQLDA entries (SQLTYPE, SQLLEN, SQLNAME,
//! the CCSID and the label). A column's name is its SQL name (the
//! field's `ALIAS`) when it has one, its label the heading.
//! Types are worked out from the statement and the tables' formats,
//! nothing is run. A null-capable entry has the odd type.

use store::txn::Txn;

use super::{Reply, code};
use crate::ast::{
    BinOp, DataType, Expr, InsertSource, Literal, Naming, QueryBody, SelectItem, Statement, TableRef, Where,
};
use crate::exec::{SqlContext, prepare};
use crate::error::SqlError;
use crate::sqlca::Sqlca;
use crate::value::Value;

/// One SQLDA entry's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Col {
    /// The even SQLTYPE.
    pub base: i16,
    /// SQLLEN: bytes, or precision * 256 + scale for a decimal.
    pub len: i32,
    /// Null-capable.
    pub null: bool,
    /// The CCSID of character data: 65535 for bytes (`FOR BIT DATA`,
    /// DDS `H`); 0 when the job's or not character.
    pub ccsid: u16,
    /// A column of a table, not an expression (`SQLRES` byte 13).
    pub column: bool,
}

impl Col {
    fn new(base: i16, len: i32, null: bool) -> Self {
        Self { base, len, null, ccsid: 0, column: false }
    }

    fn int() -> Self {
        Self::new(496, 4, false)
    }

    /// The precision and scale a number takes in arithmetic.
    fn digits(&self) -> Option<(i32, i32)> {
        match self.base {
            484 | 488 => Some((self.len >> 8, self.len & 0xFF)),
            500 => Some((5, 0)),
            496 => Some((11, 0)),
            492 => Some((19, 0)),
            _ => None,
        }
    }

    fn sqltype(&self) -> i16 {
        self.base + i16::from(self.null)
    }
}

/// The length of a date in the job's format (`*MDY`: QS/400 jobs have
/// no date format of their own yet).
const DATE_LEN: i32 = 8;

/// An entry for a DDS field.
fn of_field(f: &dds::FieldDef) -> Col {
    use dds::DataType as D;
    let len = i32::try_from(f.length).unwrap_or(i32::MAX);
    let dec = |p: u32, s: u8| i32::try_from(p * 256 + u32::from(s)).unwrap_or(i32::MAX);
    let n = f.allow_null;
    let mut c = match f.data_type {
        D::Char | D::Hex if f.varlen.is_some() => Col::new(448, len, n),
        D::Char | D::Hex => Col::new(452, len, n),
        D::Graphic | D::DbcsOpen if f.varlen.is_some() => Col::new(464, len, n),
        D::Graphic | D::DbcsOpen => Col::new(468, len, n),
        D::Zoned => Col::new(488, dec(f.length, f.decimals), n),
        D::Packed => Col::new(484, dec(f.length, f.decimals), n),
        D::Binary => match f.length {
            0..=4 => Col::new(500, 2, n),
            5..=9 => Col::new(496, 4, n),
            _ => Col::new(492, 8, n),
        },
        D::Float => Col::new(480, if f.precision == dds::FloatPrecision::Single { 4 } else { 8 }, n),
        D::Date => Col::new(384, DATE_LEN, n),
        D::Time => Col::new(388, 8, n),
        D::Timestamp => Col::new(392, len, n),
        D::DecFloat => Col::new(996, if f.length <= 16 { 8 } else { 16 }, n),
        D::Bytes if f.varlen.is_some() => Col::new(908, len, n),
        D::Bytes => Col::new(912, len, n),
        D::Boolean => Col::new(BOOLEAN, 1, n),
        D::Blob => Col::new(404, len, n),
        D::Clob => Col::new(408, len, n),
        D::Dbclob => Col::new(412, len, n),
    };
    c.ccsid = match f.data_type {
        D::Hex | D::Bytes | D::Blob => 65535,
        D::Dbclob => f.ccsid.unwrap_or(1200),
        _ => f.ccsid.unwrap_or(0),
    };
    c.column = true;
    c
}

/// The SQLTYPE of `BOOLEAN`.
const BOOLEAN: i16 = 2436;

/// The SQLLEN of `TIMESTAMP(p)`.
fn timestamp_len(p: u8) -> i32 {
    i32::try_from(dds::DataType::timestamp_length(p)).expect("small")
}

/// An entry for a `CAST` type.
fn of_type(t: &DataType) -> Col {
    let dec = |p: u8, s: u8| i32::from(p) * 256 + i32::from(s);
    let len = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
    match t {
        DataType::Char(n) => Col::new(452, len(*n), true),
        DataType::Varchar(n) => Col::new(448, len(*n), true),
        DataType::Graphic(n) => Col::new(468, len(*n), true),
        DataType::Vargraphic(n) => Col::new(464, len(*n), true),
        DataType::Decimal(p, s) => Col::new(484, dec(*p, *s), true),
        DataType::Numeric(p, s) => Col::new(488, dec(*p, *s), true),
        DataType::SmallInt => Col::new(500, 2, true),
        DataType::Integer => Col::new(496, 4, true),
        DataType::BigInt => Col::new(492, 8, true),
        DataType::Real => Col::new(480, 4, true),
        DataType::Double => Col::new(480, 8, true),
        DataType::Date => Col::new(384, DATE_LEN, true),
        DataType::Time => Col::new(388, 8, true),
        DataType::Timestamp(p) => Col::new(392, timestamp_len(*p), true),
        DataType::DecFloat(n) => Col::new(996, if *n == 16 { 8 } else { 16 }, true),
        DataType::Binary(n) => bits(Col::new(912, len(*n), true)),
        DataType::Varbinary(n) => bits(Col::new(908, len(*n), true)),
        DataType::Boolean => Col::new(BOOLEAN, 1, true),
        DataType::Blob(n) => bits(Col::new(404, len(*n), true)),
        DataType::Clob(n) => Col::new(408, len(*n), true),
        DataType::Dbclob(n) => Col { ccsid: 1200, ..Col::new(412, len(*n), true) },
    }
}

/// With CCSID 65535.
fn bits(c: Col) -> Col {
    Col { ccsid: 65535, ..c }
}

/// A field's SQL name: its `ALIAS` when it has one, else its system
/// name.
#[must_use]
pub fn sql_name(f: &dds::FieldDef) -> String {
    f.alias.as_deref().map(str::trim).filter(|a| !a.is_empty()).map_or_else(|| f.name.to_string(), str::to_string)
}

/// A field's label as an SQLDA gives it: the heading's lines, each
/// filled to 20 characters, as one text; empty when it has none.
#[must_use]
pub fn label(f: &dds::FieldDef) -> String {
    let mut joined = String::new();
    for l in &f.colhdg {
        joined.push_str(l);
        joined.extend(std::iter::repeat_n(' ', 20usize.saturating_sub(l.chars().count())));
    }
    joined.trim_end().to_string()
}

/// The tables of a `FROM`: correlation name and format fields.
struct Tables(Vec<(String, Vec<dds::FieldDef>)>);

impl Tables {
    fn load(txn: &mut Txn<'_>, ctx: &SqlContext, from: &[TableRef]) -> Self {
        fn walk(txn: &mut Txn<'_>, ctx: &SqlContext, r: &TableRef, out: &mut Vec<(String, Vec<dds::FieldDef>)>) {
            match r {
                TableRef::Table { name, alias, .. } => {
                    let a = alias.as_ref().map_or_else(|| name.name.value.clone(), |a| a.value.clone());
                    let fields = ctx.resolve_table(txn, name).map_or_else(
                        // A catalog view: its columns' fields.
                        |_| {
                            // A service view's columns without its rows.
                            crate::exec::catalog::services::columns(name, &a)
                                .map(Ok)
                                .or_else(|| crate::exec::catalog::virtual_table(txn, ctx, name, &a).transpose())
                                .and_then(std::result::Result::ok)
                                .map(|(sch, _)| sch.cols.iter().filter_map(|c| c.field.clone()).collect())
                                .unwrap_or_default()
                        },
                        |t| t.format.format.fields.clone(),
                    );
                    out.push((a, fields));
                }
                TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
                    walk(txn, ctx, left, out);
                    walk(txn, ctx, right, out);
                }
                TableRef::Lateral(inner) => walk(txn, ctx, inner, out),
                TableRef::Subquery { .. }
                | TableRef::Function { .. }
                | TableRef::Recursive { .. }
                | TableRef::DataChange { .. } => {}
            }
        }
        let mut out = Vec::new();
        for r in from {
            walk(txn, ctx, r, &mut out);
        }
        Self(out)
    }

    fn field(&self, table: Option<&str>, name: &str) -> Option<&dds::FieldDef> {
        self.0
            .iter()
            .filter(|(a, _)| table.is_none_or(|t| t == a))
            .find_map(|(_, fs)| fs.iter().find(|f| f.name.to_string() == name || f.alias.as_deref() == Some(name)))
    }

    /// The tables' columns as a query's schema.
    fn schema(&self) -> crate::exec::eval::Schema {
        let cols = self
            .0
            .iter()
            .flat_map(|(a, fs)| {
                fs.iter().map(|f| crate::exec::eval::ColInfo {
                    name: sql_name(f),
                    table: Some(a.clone()),
                    field: Some(f.clone()),
                    nullable: f.allow_null,
                    hidden: false,
                })
            })
            .collect();
        crate::exec::eval::Schema { cols }
    }

    /// An expression's type: the engine's result type when it works one
    /// out, else what the expression's form says.
    fn type_of(&self, e: &Expr) -> Col {
        if !matches!(e, Expr::Column(_) | Expr::Param(_))
            && let Ok(Some(t)) = crate::exec::restype::type_of(e, &self.schema())
        {
            let mut c = of_field(&crate::exec::restype::field_of(&t));
            c.null = t.nullable;
            c.column = false;
            return c;
        }
        let mut c = self.form_type(e);
        if !matches!(e, Expr::Column(_)) {
            c.column = false;
        }
        c
    }

    /// An expression's type from its form.
    fn form_type(&self, e: &Expr) -> Col {
        match e {
            Expr::Column(c) => self
                .field(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value)
                .map_or_else(|| Col::new(448, 32740, true), of_field),
            Expr::Literal(Literal::Num(n)) => num_literal(n),
            Expr::Literal(Literal::Str(s)) => Col::new(452, i32::try_from(s.len()).unwrap_or(i32::MAX).max(1), false),
            Expr::Literal(Literal::Date(_)) | Expr::Register(crate::ast::Register::Date) => Col::new(384, DATE_LEN, false),
            Expr::Literal(Literal::Time(_)) | Expr::Register(crate::ast::Register::Time) => Col::new(388, 8, false),
            Expr::Literal(Literal::Timestamp(s)) => Col::new(
                392,
                crate::value::Timestamp::parse(s).map_or(26, |t| timestamp_len(t.precision)),
                false,
            ),
            Expr::Literal(Literal::Bin(b)) => bits(Col::new(908, i32::try_from(b.len()).unwrap_or(i32::MAX), false)),
            Expr::Literal(Literal::Bool(_)) => Col::new(BOOLEAN, 1, false),
            Expr::And(..)
            | Expr::Or(..)
            | Expr::Not(_)
            | Expr::IsNull(..)
            | Expr::Compare(..)
            | Expr::Between(..)
            | Expr::InList(..)
            | Expr::InSubquery(..)
            | Expr::Like(..)
            | Expr::Exists(..) => Col::new(BOOLEAN, 1, true),
            Expr::Register(crate::ast::Register::Timestamp(p)) => Col::new(392, timestamp_len(*p), false),
            Expr::Neg(x) => self.type_of(x),
            Expr::Binary(a, op, b) => arith(&self.type_of(a), *op, &self.type_of(b)),
            Expr::Cast(_, t) => of_type(&t.data_type),
            Expr::Case { whens, .. } => whens.first().map_or_else(Col::int, |(_, v)| {
                let mut c = self.type_of(v);
                c.null = true;
                c
            }),
            Expr::Function(f) => {
                let arg = f.args.first().map(|a| self.type_of(a));
                match f.name.as_str() {
                    "COUNT" => Col::int(),
                    "COUNT_BIG" => Col::new(492, 8, false),
                    // DB2 widens a decimal sum to 31 digits.
                    "SUM" => arg.map_or_else(Col::int, |mut c| {
                        c.null = true;
                        if matches!(c.base, 484 | 488) {
                            c = Col::new(484, 31 * 256 + (c.len & 0xFF), true);
                        }
                        c
                    }),
                    "MAX" | "MIN" | "AVG" => arg.map_or_else(Col::int, |mut c| {
                        c.null = true;
                        c
                    }),
                    "DATE" => Col::new(384, DATE_LEN, true),
                    "TIME" => Col::new(388, 8, true),
                    "TIMESTAMP" | "NOW" => Col::new(392, 26, true),
                    "BIGINT" => Col::new(492, 8, true),
                    "INTEGER" | "INT" | "LENGTH" | "CHAR_LENGTH" | "YEAR" | "MONTH" | "DAY" | "DAYOFMONTH" | "HOUR"
                    | "MINUTE" | "SECOND" | "MICROSECOND" | "DAYS" | "POSSTR" | "LOCATE" | "OCTET_LENGTH" => {
                        Col::new(496, 4, true)
                    }
                    "SMALLINT" => Col::new(500, 2, true),
                    "DOUBLE" | "DOUBLE_PRECISION" | "FLOAT" => Col::new(480, 8, true),
                    "REAL" => Col::new(480, 4, true),
                    "DECFLOAT" => {
                        let narrow = matches!(f.args.get(1), Some(Expr::Literal(Literal::Num(n))) if n == "16");
                        Col::new(996, if narrow { 8 } else { 16 }, true)
                    }
                    "NORMALIZE_DECFLOAT" => arg.unwrap_or_else(|| Col::new(996, 16, true)),
                    "BOOLEAN" => Col::new(BOOLEAN, 1, true),
                    "BINARY" | "VARBINARY" => {
                        bits(Col::new(908, arg.map_or(32740, |c| c.len), true))
                    }
                    _ => arg.unwrap_or_else(|| Col::new(448, 32740, true)),
                }
            }
            _ => Col::new(448, 32740, true),
        }
    }
}

/// A numeric literal: an integer, a double with an exponent, or a
/// decimal of its digits.
fn num_literal(n: &str) -> Col {
    if n.contains(['E', 'e']) {
        return Col::new(480, 8, false);
    }
    if !n.contains('.') {
        return if n.trim_start_matches(['-', '+']).len() > 9 { Col::new(492, 8, false) } else { Col::int() };
    }
    let digits = n.chars().filter(char::is_ascii_digit).count();
    let scale = n.split_once('.').map_or(0, |(_, f)| f.chars().filter(char::is_ascii_digit).count());
    let p = i32::try_from(digits).unwrap_or(31);
    let s = i32::try_from(scale).unwrap_or(0);
    Col::new(484, p * 256 + s, false)
}

/// The type of `a op b` (DB2's rules for integers and decimals).
fn arith(a: &Col, op: BinOp, b: &Col) -> Col {
    let null = a.null || b.null;
    if op == BinOp::Concat {
        let len = a.len.saturating_add(b.len);
        return Col::new(448, len, null);
    }
    if a.base == 996 || b.base == 996 {
        let wide = [a, b].iter().any(|c| (c.base == 996 && c.len == 16) || c.digits().is_some_and(|(p, _)| p > 16));
        return Col::new(996, if wide { 16 } else { 8 }, null);
    }
    let ints = [500, 496, 492];
    if ints.contains(&a.base) && ints.contains(&b.base) {
        let base = a.base.min(b.base).min(496);
        let len = if base == 492 { 8 } else { 4 };
        return Col::new(base, len, null);
    }
    if a.base == 480 || b.base == 480 {
        return Col::new(480, 8, null);
    }
    let (Some((p1, s1)), Some((p2, s2))) = (a.digits(), b.digits()) else {
        return Col::new(480, 8, null);
    };
    let (p, s) = match op {
        BinOp::Mul => (p1 + p2, s1 + s2),
        BinOp::Div => (31, (31 - p1 + s1 - s2).max(0)),
        _ => {
            let s = s1.max(s2);
            ((p1 - s1).max(p2 - s2) + s + 1, s)
        }
    };
    let p = p.min(63);
    Col::new(484, p * 256 + s.min(p), null)
}

/// The markers of a statement, typed by what they stand beside.
#[allow(clippy::too_many_lines)] // one arm per statement kind
fn markers(t: &Tables, st: &Statement) -> Vec<(usize, Col, Option<String>)> {
    fn walk(t: &Tables, e: &Expr, out: &mut Vec<(usize, Col, Option<String>)>) {
        let beside = |x: &Expr, other: &Expr, out: &mut Vec<(usize, Col, Option<String>)>| {
            if let Expr::Param(i) = x {
                let mut c = t.type_of(other);
                c.null = true;
                c.column = false;
                // A marker beside a date takes the ISO length.
                if c.base == 384 {
                    c.len = 10;
                }
                out.push((*i, c, None));
            }
        };
        match e {
            Expr::Compare(a, _, b, _) => {
                beside(a, b, out);
                beside(b, a, out);
                walk(t, a, out);
                walk(t, b, out);
            }
            Expr::Between(x, lo, hi, _) => {
                beside(lo, x, out);
                beside(hi, x, out);
                walk(t, x, out);
            }
            Expr::InList(x, items, _) => {
                for i in items {
                    beside(i, x, out);
                }
                walk(t, x, out);
            }
            Expr::And(a, b) | Expr::Or(a, b) => {
                walk(t, a, out);
                walk(t, b, out);
            }
            Expr::Not(a) => walk(t, a, out),
            // A typed marker: CAST(? AS type).
            Expr::Cast(x, ty) if matches!(**x, Expr::Param(_)) => {
                if let Expr::Param(i) = **x {
                    out.push((i, of_type(&ty.data_type), None));
                }
            }
            Expr::Cast(x, _) => walk(t, x, out),
            Expr::Binary(a, _, b) => {
                beside(a, b, out);
                beside(b, a, out);
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let column = |name: &str| t.field(None, name).map_or_else(|| Col::new(448, 32740, true), of_field);
    match st {
        Statement::Select(q) => {
            if let QueryBody::Select(s) = &q.body {
                for item in &s.items {
                    if let SelectItem::Expr(e, _) = item {
                        walk(t, e, &mut out);
                    }
                }
                if let Some(w) = &s.where_clause {
                    walk(t, w, &mut out);
                }
                if let Some(h) = &s.having {
                    walk(t, h, &mut out);
                }
            }
            if let QueryBody::Values(rows) = &q.body {
                for e in rows.iter().flatten() {
                    walk(t, e, &mut out);
                }
            }
        }
        Statement::Update(u) => {
            for a in &u.set {
                for (c, v) in a.columns.iter().zip(&a.values) {
                    if let Expr::Param(i) = v {
                        let mut col = column(&c.value);
                        col.column = true;
                        out.push((*i, col, Some(c.value.clone())));
                    }
                }
            }
            if let Where::Cond(w) = &u.where_clause {
                walk(t, w, &mut out);
            }
        }
        Statement::Delete(d) => {
            if let Where::Cond(w) = &d.where_clause {
                walk(t, w, &mut out);
            }
        }
        Statement::Insert(ins) => {
            if let InsertSource::Values(rows) = &ins.source {
                let names: Vec<String> = if ins.columns.is_empty() {
                    t.0.first().map(|(_, fs)| fs.iter().map(|f| f.name.to_string()).collect()).unwrap_or_default()
                } else {
                    ins.columns.iter().map(|c| c.value.clone()).collect()
                };
                for row in rows {
                    for (v, n) in row.iter().zip(&names) {
                        if let Expr::Param(i) = v {
                            let mut col = column(n);
                            col.column = true;
                            out.push((*i, col, Some(n.clone())));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out.sort_by_key(|(i, _, _)| *i);
    out.dedup_by_key(|(i, _, _)| *i);
    out
}

/// The tables a statement reads or changes, for its types.
fn tables_of(txn: &mut Txn<'_>, ctx: &SqlContext, st: &Statement) -> Tables {
    let one = |name: &crate::ast::ObjectName| TableRef::Table {
        name: name.clone(),
        alias: None,
        columns: Vec::new(),
    };
    match st {
        Statement::Select(q) => match &q.body {
            QueryBody::Select(s) => Tables::load(txn, ctx, &s.from),
            _ => Tables(Vec::new()),
        },
        Statement::Update(u) => Tables::load(txn, ctx, &[one(&u.table)]),
        Statement::Delete(d) => Tables::load(txn, ctx, &[one(&d.table)]),
        Statement::Insert(i) => Tables::load(txn, ctx, &[one(&i.table)]),
        _ => Tables(Vec::new()),
    }
}
/// One SQLDA entry: its type and the names `USING` chooses among.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The type.
    pub col: Col,
    /// The SQL name (`USING NAMES`).
    pub name: String,
    /// The heading as one text of 20-character lines (column labels).
    pub label: String,
    /// The heading's words as `USING LABELS` gives them: the name when
    /// the column has no heading.
    pub label_name: String,
    /// The system column name (`USING SYSTEM NAMES`).
    pub system: String,
}

impl Entry {
    fn of_field(f: &dds::FieldDef) -> Self {
        Self {
            col: of_field(f),
            name: sql_name(f),
            label: label(f),
            label_name: label_name(f),
            system: f.name.to_string(),
        }
    }

    fn expr(col: Col, name: String) -> Self {
        Self {
            col,
            label: String::new(),
            label_name: name.clone(),
            system: name.clone(),
            name,
        }
    }
}

/// A column's heading lines joined by one blank, or its name.
fn label_name(f: &dds::FieldDef) -> String {
    let words: Vec<&str> = f.colhdg.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    if words.is_empty() { sql_name(f) } else { words.join(" ") }
}

/// The CCSID of the job: what dates, times and markers of character
/// data describe with. QS/400 jobs run in CCSID 37.
pub const JOB_CCSID: u16 = 37;

impl Col {
    /// The CCSID an SQLDA entry gives: the column's, the job's for dates,
    /// times and character data without one, 0 for numbers.
    #[must_use]
    pub fn sqlda_ccsid(&self) -> u16 {
        match self.base {
            384 | 388 | 392 | 448 | 452 | 456 | 408 | 464 | 468 | 412 if self.ccsid == 0 => JOB_CCSID,
            404 | 908 | 912 => 65535,
            384 | 388 | 392 | 448 | 452 | 456 | 408 | 464 | 468 | 412 => self.ccsid,
            _ => 0,
        }
    }

    /// `SQLRES` byte 15: 0x40 for bit data, 0x10 for graphic data.
    #[must_use]
    pub fn data_flags(&self) -> u8 {
        match self.base {
            448 | 452 | 456 if self.ccsid == 65535 => 0x40,
            464 | 468 | 412 => 0x10,
            _ => 0,
        }
    }

    /// A large object: its length is in the extended entry.
    #[must_use]
    pub fn is_lob(&self) -> bool {
        matches!(self.base, 404 | 408 | 412)
    }
}

/// The entries of a statement: its result columns, or with `input` its
/// markers. A statement that is not a query has no result columns.
#[must_use]
pub fn entries(txn: &mut Txn<'_>, ctx: &SqlContext, st: &Statement, input: bool) -> Vec<Entry> {
    let tabs = tables_of(txn, ctx, st);
    if input {
        return markers(&tabs, st)
            .into_iter()
            .enumerate()
            .map(|(k, (_, c, name))| Entry::expr(c, name.unwrap_or_else(|| format!("{:05}", k + 1))))
            .collect();
    }
    let Statement::Select(query) = st else { return Vec::new() };
    let QueryBody::Select(sel) = &query.body else { return Vec::new() };
    let mut out = Vec::new();
    for item in &sel.items {
        match item {
            SelectItem::Star => {
                for (_, fs) in &tabs.0 {
                    out.extend(fs.iter().map(Entry::of_field));
                }
            }
            SelectItem::TableStar(a) => {
                for (_, fs) in tabs.0.iter().filter(|(n, _)| *n == a.value) {
                    out.extend(fs.iter().map(Entry::of_field));
                }
            }
            SelectItem::Expr(e, alias) => {
                // A column keeps its labels under a new name.
                let field = match e {
                    Expr::Column(c) => tabs.field(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value),
                    _ => None,
                };
                let mut entry = if let Some(f) = field {
                    Entry { col: tabs.type_of(e), ..Entry::of_field(f) }
                } else {
                    let name = match e {
                        Expr::Column(c) => c.column.value.clone(),
                        _ => format!("{:05}", out.len() + 1),
                    };
                    Entry::expr(tabs.type_of(e), name)
                };
                if let Some(a) = alias {
                    entry.name.clone_from(&a.value);
                }
                out.push(entry);
            }
        }
    }
    out
}

/// The columns of a table (`DESCRIBE TABLE`): every field, none of them
/// marked as a result column.
pub fn table_entries(txn: &mut Txn<'_>, ctx: &SqlContext, name: &crate::ast::ObjectName) -> Result<Vec<Entry>, SqlError> {
    let fields = match ctx.resolve_table(txn, name) {
        Ok(t) => t.format.format.fields.clone(),
        Err(e) => match crate::exec::catalog::virtual_table(txn, ctx, name, &name.name.value) {
            Ok(Some((sch, _))) => sch.cols.iter().filter_map(|c| c.field.clone()).collect(),
            _ => return Err(e),
        },
    };
    Ok(fields
        .iter()
        .map(|f| {
            let mut e = Entry::of_field(f);
            e.col.column = false;
            e
        })
        .collect())
}

/// The reply rows of entries: SQLTYPE, SQLLEN (a large object's whole
/// length), the SQL name, the CCSID, the label, the column flag, the
/// data flags, the label as `USING LABELS` gives it, the system name.
#[must_use]
pub fn rows(entries: Vec<Entry>) -> Vec<Vec<Value>> {
    entries
        .into_iter()
        .map(|e| {
            vec![
                Value::int(i64::from(e.col.sqltype())),
                Value::int(i64::from(e.col.len)),
                Value::Str(e.name),
                Value::int(i64::from(e.col.sqlda_ccsid())),
                Value::Str(e.label),
                Value::int(i64::from(e.col.column)),
                Value::int(i64::from(e.col.data_flags())),
                Value::Str(e.label_name),
                Value::Str(e.system),
            ]
        })
        .collect()
}

/// `DESCRIBE` of prepared `text` as a reply: [`rows`].
pub fn reply(txn: &mut Txn<'_>, ctx: &mut SqlContext, text: &str, naming: Naming, input: bool) -> Reply {
    let p = match prepare(text, naming) {
        Ok(p) => p,
        Err(e) => return Reply::of(Sqlca::from_error(&e)),
    };
    ctx.naming = naming;
    let rows = rows(entries(txn, ctx, &p.statement, input));
    Reply { sqlca: code(0, "00000"), rows, call: None }
}

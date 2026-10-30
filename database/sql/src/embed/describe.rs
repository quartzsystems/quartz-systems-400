// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DESCRIBE`: a prepared statement's result columns, or with `INPUT`
//! its parameter markers, as SQLDA entries (SQLTYPE, SQLLEN, SQLNAME).
//! Types are worked out from the statement and the tables' formats,
//! nothing is run. A null-capable entry has the odd type.

use store::txn::Txn;

use super::{Reply, code};
use crate::ast::{
    BinOp, DataType, Expr, InsertSource, Literal, Naming, QueryBody, SelectItem, Statement, TableRef, Where,
};
use crate::exec::{SqlContext, prepare};
use crate::sqlca::Sqlca;
use crate::value::Value;

/// One SQLDA entry's type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Col {
    /// The even SQLTYPE.
    pub base: i16,
    /// SQLLEN: bytes, or precision * 256 + scale for a decimal.
    pub len: i16,
    /// Null-capable.
    pub null: bool,
}

impl Col {
    fn new(base: i16, len: i16, null: bool) -> Self {
        Self { base, len, null }
    }

    fn int() -> Self {
        Self::new(496, 4, false)
    }

    /// The precision and scale a number takes in arithmetic.
    fn digits(&self) -> Option<(i16, i16)> {
        match self.base {
            484 | 488 => Some((self.len >> 8, self.len & 0xFF)),
            500 => Some((5, 0)),
            496 => Some((10, 0)),
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
const DATE_LEN: i16 = 8;

/// An entry for a DDS field.
fn of_field(f: &dds::FieldDef) -> Col {
    use dds::DataType as D;
    let len = i16::try_from(f.length).unwrap_or(i16::MAX);
    let dec = |p: u32, s: u8| i16::try_from(p * 256 + u32::from(s)).unwrap_or(i16::MAX);
    let n = f.allow_null;
    match f.data_type {
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
        D::Timestamp => Col::new(392, 26, n),
    }
}

/// An entry for a `CAST` type.
fn of_type(t: &DataType) -> Col {
    let dec = |p: u8, s: u8| i16::from(p) * 256 + i16::from(s);
    let len = |n: u32| i16::try_from(n).unwrap_or(i16::MAX);
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
        DataType::Timestamp => Col::new(392, 26, true),
    }
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
                            crate::exec::catalog::virtual_table(txn, ctx, name, &a)
                                .ok()
                                .flatten()
                                .map(|(sch, _)| sch.cols.iter().filter_map(|c| c.field.clone()).collect())
                                .unwrap_or_default()
                        },
                        |t| t.format.format.fields.clone(),
                    );
                    out.push((a, fields));
                }
                TableRef::Join { left, right, .. } => {
                    walk(txn, ctx, left, out);
                    walk(txn, ctx, right, out);
                }
                TableRef::Subquery { .. } => {}
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

    /// An expression's type.
    fn type_of(&self, e: &Expr) -> Col {
        match e {
            Expr::Column(c) => self
                .field(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value)
                .map_or_else(|| Col::new(448, 32740, true), of_field),
            Expr::Literal(Literal::Num(n)) => num_literal(n),
            Expr::Literal(Literal::Str(s)) => Col::new(452, i16::try_from(s.len()).unwrap_or(i16::MAX).max(1), false),
            Expr::Literal(Literal::Date(_)) => Col::new(384, DATE_LEN, false),
            Expr::Literal(Literal::Time(_)) => Col::new(388, 8, false),
            Expr::Literal(Literal::Timestamp(_)) => Col::new(392, 26, false),
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
                    | "MINUTE" | "SECOND" | "MICROSECOND" | "DAYS" | "POSSTR" | "LOCATE" => Col::new(496, 4, true),
                    "SMALLINT" => Col::new(500, 2, true),
                    "DOUBLE" | "DOUBLE_PRECISION" | "FLOAT" => Col::new(480, 8, true),
                    "REAL" => Col::new(480, 4, true),
                    _ => arg.unwrap_or_else(|| Col::new(448, 32740, true)),
                }
            }
            _ => Col::new(448, 32740, true),
        }
    }
}

/// A numeric literal: an integer, or a decimal of its digits.
fn num_literal(n: &str) -> Col {
    if !n.contains(['.', 'E', 'e']) {
        return if n.trim_start_matches(['-', '+']).len() > 9 { Col::new(492, 8, false) } else { Col::int() };
    }
    let digits = n.chars().filter(char::is_ascii_digit).count();
    let scale = n.split_once('.').map_or(0, |(_, f)| f.chars().filter(char::is_ascii_digit).count());
    let p = i16::try_from(digits).unwrap_or(31);
    let s = i16::try_from(scale).unwrap_or(0);
    Col::new(484, p * 256 + s, false)
}

/// The type of `a op b` (DB2's rules for integers and decimals).
fn arith(a: &Col, op: BinOp, b: &Col) -> Col {
    let null = a.null || b.null;
    if op == BinOp::Concat {
        let len = a.len.saturating_add(b.len);
        return Col::new(448, len, null);
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
fn markers(t: &Tables, st: &Statement) -> Vec<(usize, Col)> {
    fn walk(t: &Tables, e: &Expr, out: &mut Vec<(usize, Col)>) {
        let beside = |x: &Expr, other: &Expr, out: &mut Vec<(usize, Col)>| {
            if let Expr::Param(i) = x {
                let mut c = t.type_of(other);
                c.null = true;
                out.push((*i, c));
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
        }
        Statement::Update(u) => {
            for a in &u.set {
                for (c, v) in a.columns.iter().zip(&a.values) {
                    if let Expr::Param(i) = v {
                        let mut col = column(&c.value);
                        col.null = true;
                        out.push((*i, col));
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
                            col.null = true;
                            out.push((*i, col));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out.sort_by_key(|(i, _)| *i);
    out.dedup_by_key(|(i, _)| *i);
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

/// The entries of `text`: its result columns, or with `input` its
/// markers. A statement that is not a query has no result columns.
#[must_use]
pub fn entries(txn: &mut Txn<'_>, ctx: &SqlContext, st: &Statement, input: bool) -> Vec<(Col, String)> {
    let tabs = tables_of(txn, ctx, st);
    if input {
        return markers(&tabs, st).into_iter().enumerate().map(|(k, (_, c))| (c, format!("{:05}", k + 1))).collect();
    }
    let Statement::Select(query) = st else { return Vec::new() };
    let QueryBody::Select(sel) = &query.body else { return Vec::new() };
    let mut out = Vec::new();
    for item in &sel.items {
        match item {
            SelectItem::Star => {
                for (_, fs) in &tabs.0 {
                    out.extend(fs.iter().map(|f| (of_field(f), f.name.to_string())));
                }
            }
            SelectItem::TableStar(a) => {
                for (_, fs) in tabs.0.iter().filter(|(n, _)| *n == a.value) {
                    out.extend(fs.iter().map(|f| (of_field(f), f.name.to_string())));
                }
            }
            SelectItem::Expr(e, alias) => {
                let name = match (alias, e) {
                    (Some(a), _) => a.value.clone(),
                    (None, Expr::Column(c)) => c.column.value.clone(),
                    _ => format!("{:05}", out.len() + 1),
                };
                out.push((tabs.type_of(e), name));
            }
        }
    }
    out
}

/// `DESCRIBE` of prepared `text` as a reply: one row per entry
/// (SQLTYPE, SQLLEN, SQLNAME).
pub fn reply(txn: &mut Txn<'_>, ctx: &mut SqlContext, text: &str, naming: Naming, input: bool) -> Reply {
    let p = match prepare(text, naming) {
        Ok(p) => p,
        Err(e) => return Reply::of(Sqlca::from_error(&e)),
    };
    ctx.naming = naming;
    let rows = entries(txn, ctx, &p.statement, input)
        .into_iter()
        .map(|(c, n)| vec![Value::int(i64::from(c.sqltype())), Value::int(i64::from(c.len)), Value::Str(n)])
        .collect();
    Reply { sqlca: code(0, "00000"), rows }
}

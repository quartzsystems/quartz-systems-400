// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Result types of expressions, worked out before any row is read: what
//! a result column describes as, and what its values are fitted to.
//! Constants, `CAST`, arithmetic, concatenation, `CASE` and `COALESCE`
//! follow IBM's rules (`tools/conformance/sql/cases/types/`); aggregates,
//! OLAP specifications, special registers and date arithmetic theirs
//! (`rules`, `exec::agg`, `exec::olap`); a function family gives its own
//! through [`FUNCTION_TYPES`]. `None` is a type not worked out: the
//! column describes as `VARCHAR(32767)` and its values stay as computed.

#![allow(clippy::many_single_char_names)]

pub mod rules;
pub mod shape;
pub mod unify;

use dds::{FieldDef, FloatPrecision, VarLen};
use qsname::Name;

use super::eval::Schema;
use crate::ast::{BinOp, DataType, Expr, Function, Literal, Register, TypeSpec};
use crate::error::Result;
use crate::value::Value;

pub use rules::infer;
pub use shape::Kind;
pub use unify::{arith, unify};

/// A result type: the type and whether it may be null.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlType {
    /// The type.
    pub spec: TypeSpec,
    /// Null-capable.
    pub nullable: bool,
}

impl SqlType {
    /// A type that is never null.
    #[must_use]
    pub fn not_null(t: DataType) -> Self {
        Self { spec: TypeSpec { data_type: t, ccsid: None, bit_data: false }, nullable: false }
    }

    /// The data type.
    #[must_use]
    pub fn data_type(&self) -> &DataType {
        &self.spec.data_type
    }

    fn or_null(mut self, nullable: bool) -> Self {
        self.nullable |= nullable;
        self
    }
}

/// A function family's result types: the type of `name` called with
/// arguments of `args` (`None` where unknown), or `None` when the
/// family does not know the function.
pub type FunctionType = fn(&str, &[Option<SqlType>], &[Expr]) -> Option<SqlType>;

/// The families' result types, asked in order.
pub const FUNCTION_TYPES: &[FunctionType] = &[
    cast_functions,
    super::json::types,
    super::func::string::result_type,
    super::func::datetime::result_type,
    super::func::math::result_type,
    super::func::misc::result_type,
    super::func::services::result_type,
    rules::builtin,
    super::routine::udf::result_type,
];

/// The frames an expression's columns are looked up in, innermost last,
/// and the types of the sequences it names.
#[derive(Default)]
pub struct Scope<'a> {
    /// The frames.
    pub frames: Vec<&'a Schema>,
    /// `NEXT VALUE` and `PREVIOUS VALUE` of each sequence: its type.
    pub sequences: Vec<(crate::ast::ObjectName, DataType)>,
}

impl<'a> Scope<'a> {
    /// The scope of one row's columns.
    #[must_use]
    pub fn of(schema: &'a Schema) -> Self {
        Self { frames: vec![schema], sequences: Vec::new() }
    }

    /// The scope of an environment's frames.
    #[must_use]
    pub fn of_env(env: &'a super::eval::Env) -> Self {
        Self { frames: env.frames.iter().map(|(s, _)| s.as_ref()).collect(), sequences: Vec::new() }
    }

    /// A column's type: `Ok(None)` when it is found without a type or not
    /// found; an error when the innermost frame finds it twice.
    pub fn column(&self, table: Option<&str>, name: &str) -> Result<Option<SqlType>> {
        for (k, s) in self.frames.iter().rev().enumerate() {
            let found = match s.find(table, name) {
                Ok(f) => f,
                Err(e) if k == 0 => return Err(e),
                Err(_) => continue,
            };
            if let Some(i) = found {
                let col = &s.cols[i];
                return Ok(col.field.as_ref().map(|f| of_field(f).or_null(col.nullable)));
            }
        }
        Ok(None)
    }

    /// Whether a column of that name is in scope.
    #[must_use]
    pub fn has(&self, table: Option<&str>, name: &str) -> bool {
        self.frames.iter().any(|s| !matches!(s.find(table, name), Ok(None)))
    }
}

/// The type of a field.
#[must_use]
pub fn of_field(f: &FieldDef) -> SqlType {
    use dds::DataType as D;
    let len = f.length;
    let t = match f.data_type {
        D::Char | D::Hex if f.varlen.is_some() => DataType::Varchar(len),
        D::Char | D::Hex => DataType::Char(len),
        D::Graphic | D::DbcsOpen if f.varlen.is_some() => DataType::Vargraphic(len),
        D::Graphic | D::DbcsOpen => DataType::Graphic(len),
        D::Zoned => DataType::Numeric(u8::try_from(len).unwrap_or(63), f.decimals),
        D::Binary if f.int_range || f.decimals == 0 => match f.data_len() {
            2 => DataType::SmallInt,
            4 => DataType::Integer,
            _ => DataType::BigInt,
        },
        D::Packed | D::Binary => DataType::Decimal(u8::try_from(len).unwrap_or(63), f.decimals),
        D::Float if f.precision == FloatPrecision::Single => DataType::Real,
        D::Float => DataType::Double,
        D::Date => DataType::Date,
        D::Time => DataType::Time,
        D::Timestamp => DataType::Timestamp(D::timestamp_precision(len)),
        D::DecFloat => DataType::DecFloat(if len <= 16 { 16 } else { 34 }),
        D::Bytes if f.varlen.is_some() => DataType::Varbinary(len),
        D::Bytes => DataType::Binary(len),
        D::Boolean => DataType::Boolean,
        D::Blob => DataType::Blob(len),
        D::Clob => DataType::Clob(len),
        D::Dbclob => DataType::Dbclob(len),
    };
    let ccsid = f.ccsid.filter(|c| *c != 65535 || !matches!(f.data_type, D::Bytes | D::Blob));
    SqlType {
        spec: TypeSpec { data_type: t, bit_data: ccsid == Some(65535), ccsid: ccsid.filter(|c| *c != 65535) },
        nullable: f.allow_null,
    }
}

/// The field a result column of `t` describes as.
#[must_use]
pub fn field_of(t: &SqlType) -> FieldDef {
    use dds::DataType as D;
    let name = Name::parse("QSEXPR").expect("valid");
    let varying = Some(VarLen { allocated: None });
    let (dt, len, dec, var) = match *t.data_type() {
        DataType::Char(n) => (D::Char, n, 0, None),
        DataType::Varchar(n) => (D::Char, n, 0, varying),
        DataType::Graphic(n) => (D::Graphic, n, 0, None),
        DataType::Vargraphic(n) => (D::Graphic, n, 0, varying),
        DataType::Decimal(p, s) => (D::Packed, u32::from(p), s, None),
        DataType::Numeric(p, s) => (D::Zoned, u32::from(p), s, None),
        DataType::SmallInt => (D::Binary, 4, 0, None),
        DataType::Integer => (D::Binary, 9, 0, None),
        DataType::BigInt => (D::Binary, 18, 0, None),
        DataType::Real => (D::Float, 9, 0, None),
        DataType::Double => (D::Float, 17, 0, None),
        DataType::Date => (D::Date, 10, 0, None),
        DataType::Time => (D::Time, 8, 0, None),
        DataType::Timestamp(p) => (D::Timestamp, D::timestamp_length(p), 0, None),
        DataType::DecFloat(n) => (D::DecFloat, u32::from(n), 0, None),
        DataType::Binary(n) => (D::Bytes, n, 0, None),
        DataType::Varbinary(n) => (D::Bytes, n, 0, varying),
        DataType::Boolean => (D::Boolean, 1, 0, None),
        DataType::Blob(n) => (D::Blob, n, 0, None),
        DataType::Clob(n) => (D::Clob, n, 0, None),
        DataType::Dbclob(n) => (D::Dbclob, n, 0, None),
    };
    let mut f = FieldDef::new(name, dt, len, dec);
    f.varlen = var;
    f.allow_null = t.nullable;
    f.int_range = matches!(t.data_type(), DataType::SmallInt | DataType::Integer | DataType::BigInt);
    match t.data_type() {
        DataType::Real => {
            f.precision = FloatPrecision::Single;
            f.byte_len = 4;
        }
        DataType::Double => {
            f.precision = FloatPrecision::Double;
            f.byte_len = 8;
        }
        _ => {}
    }
    f.ccsid = if t.spec.bit_data {
        Some(65535)
    } else {
        match dt {
            D::Bytes | D::Blob => Some(65535),
            D::Graphic | D::Dbclob => Some(t.spec.ccsid.unwrap_or(1200)),
            _ => t.spec.ccsid,
        }
    };
    f
}

/// The type of a number literal as written: `INTEGER`, `BIGINT`, else
/// `DECIMAL` of its digits; `DOUBLE` with an exponent.
fn number_literal(n: &str) -> Option<DataType> {
    if n.contains(['E', 'e']) {
        return Some(DataType::Double);
    }
    let (int, frac) = n.split_once('.').unwrap_or((n, ""));
    if frac.is_empty() && !n.contains('.')
        && let Ok(v) = int.parse::<i64>()
    {
        return Some(if i32::try_from(v).is_ok() { DataType::Integer } else { DataType::BigInt });
    }
    let p = u8::try_from(int.len() + frac.len()).ok()?.max(1);
    Some(DataType::Decimal(p, u8::try_from(frac.len()).ok()?))
}

/// The type of an expression over `schema`; an error for operands IBM
/// refuses to combine (`SQL0581` in a `CASE`).
pub fn type_of(e: &Expr, schema: &Schema) -> Result<Option<SqlType>> {
    type_in(e, &Scope::of(schema))
}

/// The type of an expression in a scope.
#[allow(clippy::too_many_lines)]
pub fn type_in(e: &Expr, scope: &Scope<'_>) -> Result<Option<SqlType>> {
    let nn = |t: DataType| Ok(Some(SqlType::not_null(t)));
    match e {
        Expr::Literal(l) => match l {
            Literal::Str(s) => nn(DataType::Varchar(u32::try_from(s.chars().count()).unwrap_or(u32::MAX))),
            Literal::Hex(h) => nn(DataType::Varchar(u32::try_from(h.len()).unwrap_or(u32::MAX))),
            Literal::Bin(b) => nn(DataType::Varbinary(u32::try_from(b.len()).unwrap_or(u32::MAX))),
            Literal::Bool(_) => nn(DataType::Boolean),
            Literal::Num(n) => Ok(number_literal(n).map(SqlType::not_null)),
            Literal::Null => Ok(None),
            Literal::Date(_) => nn(DataType::Date),
            Literal::Time(_) => nn(DataType::Time),
            Literal::Timestamp(s) => Ok(crate::value::Timestamp::parse(s).map(|t| SqlType::not_null(DataType::Timestamp(t.precision)))),
        },
        Expr::Column(c) => scope.column(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value),
        Expr::Register(r) => match r {
            Register::Date => nn(DataType::Date),
            Register::Time => nn(DataType::Time),
            Register::Timestamp(p) => nn(DataType::Timestamp(*p)),
            Register::User | Register::Server => nn(DataType::Varchar(18)),
            Register::Schema | Register::DecfloatRounding => nn(DataType::Varchar(128)),
            Register::Path => nn(DataType::Varchar(3483)),
            Register::Named(n) => Ok(Some(super::registers::result_type(n))),
            Register::Timezone => nn(DataType::Decimal(6, 0)),
        },
        Expr::Cast(a, t) => {
            let inner = type_in(a, scope)?;
            let mut spec = t.clone();
            if let Some(i) = &inner {
                check_cast(i, t)?;
                // Binary data cast to a character type is bit data.
                if unify::is_binary(i.data_type()) && matches!(t.data_type, DataType::Char(_) | DataType::Varchar(_)) {
                    spec.bit_data = true;
                }
            }
            if spec.ccsid == Some(65535) {
                spec.bit_data = true;
            }
            Ok(Some(SqlType { spec, nullable: inner.is_none_or(|i| i.nullable) }))
        }
        Expr::Neg(a) => Ok(type_in(a, scope)?.filter(|t| unify::is_number(t.data_type()))),
        Expr::Duration(a, _) => type_in(a, scope),
        Expr::Binary(a, op, b) => {
            // A labeled duration takes the type of the other side.
            if matches!(**b, Expr::Duration(..)) {
                return type_in(a, scope);
            }
            if matches!(**a, Expr::Duration(..)) {
                return type_in(b, scope);
            }
            let (Some(x), Some(y)) = (type_in(a, scope)?, type_in(b, scope)?) else { return Ok(None) };
            if let Some(t) = rules::datetime(&x, *op, &y) {
                return Ok(Some(t));
            }
            Ok(if *op == BinOp::Concat { unify::concat(&x, &y) } else { arith(&x, *op, &y) })
        }
        Expr::Case { whens, otherwise, .. } => {
            let mut results = Vec::with_capacity(whens.len() + 1);
            for (_, t) in whens {
                results.push(t);
            }
            if let Some(o) = otherwise {
                results.push(o);
            }
            let t = unify::all(results.into_iter(), scope, true)?;
            Ok(t.map(|t| t.or_null(otherwise.is_none())))
        }
        Expr::Function(f) => function(f, scope),
        Expr::WithinGroup(w) => {
            if w.func.name == "PERCENTILE_DISC"
                && let Some(o) = w.order.first()
            {
                return Ok(type_in(&o.expr, scope)?.map(|t| t.nullable(true)));
            }
            function(&w.func, scope)
        }
        Expr::Over(o) => Ok(Some(super::olap::result_type(o, scope))),
        Expr::Row(items) if items.len() == 1 => type_in(&items[0], scope),
        Expr::NextValue(n) | Expr::PrevValue(n) => Ok(scope
            .sequences
            .iter()
            .find(|(s, _)| s == n)
            .map(|(_, t)| SqlType::not_null(t.clone()))),
        // A predicate as a value is a BOOLEAN (IBM i 7.5).
        Expr::IsNull(..) | Expr::Exists(..) => nn(DataType::Boolean),
        Expr::Compare(..) | Expr::And(..) | Expr::Or(..) | Expr::Not(_) | Expr::Between(..) | Expr::InList(..) | Expr::InSubquery(..) | Expr::Like(..) => {
            Ok(Some(SqlType { spec: TypeSpec { data_type: DataType::Boolean, ccsid: None, bit_data: false }, nullable: true }))
        }
        _ => Ok(None),
    }
}

fn function(f: &Function, scope: &Scope<'_>) -> Result<Option<SqlType>> {
    if matches!(f.name.as_str(), "COALESCE" | "VALUE" | "IFNULL") && !f.args.is_empty() {
        let t = unify::all(f.args.iter(), scope, false)?;
        let mut all_null = true;
        for a in &f.args {
            all_null &= type_in(a, scope)?.is_none_or(|t| t.nullable);
        }
        return Ok(t.map(|mut t| {
            t.nullable = all_null;
            t
        }));
    }
    // A name that is no column is a keyword argument (EXTRACT's part,
    // CHAR's format, STRIP's end); INTERPRET's type is a CAST of null.
    let mut args = Vec::with_capacity(f.args.len());
    for a in &f.args {
        args.push(match a {
            Expr::Column(c) if c.table.is_none() && !scope.has(None, &c.column.value) => Some(SqlType::varchar(10, false)),
            Expr::Cast(x, t) if matches!(**x, Expr::Literal(Literal::Null)) && f.name == "INTERPRET" => {
                Some(SqlType::of_data_type(&t.data_type, false))
            }
            a => type_in(a, scope)?,
        });
    }
    if super::agg::is_aggregate(f) {
        return Ok(super::agg::result_type(f, &args));
    }
    if f.star || f.distinct {
        return Ok(None);
    }
    Ok(FUNCTION_TYPES.iter().find_map(|g| g(&f.name, &args, &f.args)))
}

/// The characters a value of `t` takes as text: `CHAR(x)` of a number,
/// the length of a string.
#[must_use]
pub fn char_length(t: &DataType) -> Option<u32> {
    Some(match *t {
        DataType::Char(n)
        | DataType::Varchar(n)
        | DataType::Graphic(n)
        | DataType::Vargraphic(n)
        | DataType::Clob(n)
        | DataType::Dbclob(n)
        | DataType::Binary(n)
        | DataType::Varbinary(n)
        | DataType::Blob(n) => n,
        DataType::SmallInt => 6,
        DataType::Integer => 11,
        DataType::BigInt => 20,
        DataType::Decimal(p, _) | DataType::Numeric(p, _) => u32::from(p) + 2,
        DataType::Real | DataType::Double => 24,
        DataType::DecFloat(_) => 42,
        DataType::Date => 10,
        DataType::Time => 8,
        DataType::Timestamp(p) => dds::DataType::timestamp_length(p),
        DataType::Boolean => 5,
    })
}

/// The types of the casting functions and the `DECFLOAT` family.
fn cast_functions(name: &str, args: &[Option<SqlType>], exprs: &[Expr]) -> Option<SqlType> {
    let first = args.first()?.as_ref()?;
    let null = first.nullable;
    let int_arg = |i: usize| match exprs.get(i) {
        Some(Expr::Literal(Literal::Num(n))) => n.parse::<u32>().ok(),
        _ => None,
    };
    let df = |t: &SqlType| match t.data_type() {
        DataType::DecFloat(16) => 16,
        _ => 34,
    };
    let t = match name {
        "SMALLINT" => DataType::SmallInt,
        "INTEGER" | "INT" => DataType::Integer,
        "BIGINT" => DataType::BigInt,
        "DOUBLE" | "FLOAT" | "DOUBLE_PRECISION" => DataType::Double,
        "REAL" => DataType::Real,
        "DECFLOAT" => DataType::DecFloat(if int_arg(1) == Some(16) { 16 } else { 34 }),
        "NORMALIZE_DECFLOAT" => DataType::DecFloat(df(first)),
        "DECFLOAT_FORMAT" => DataType::DecFloat(34),
        "QUANTIZE" => {
            let second = args.get(1)?.as_ref()?;
            let n = if df(first) == 16 && df(second) == 16 { 16 } else { 34 };
            return Some(SqlType::not_null(DataType::DecFloat(n)).or_null(null || second.nullable));
        }
        "COMPARE_DECFLOAT" | "TOTALORDER" => {
            let second = args.get(1)?.as_ref()?;
            return Some(SqlType::not_null(DataType::SmallInt).or_null(null || second.nullable));
        }
        "DATE" if exprs.len() == 1 => DataType::Date,
        "TIME" if exprs.len() == 1 => DataType::Time,
        "TIMESTAMP" if exprs.len() == 1 => DataType::Timestamp(6),
        "BINARY" | "VARBINARY" if unify::is_text(first.data_type()) || unify::is_binary(first.data_type()) => {
            let n = int_arg(1).or_else(|| char_length(first.data_type()))?;
            if name == "BINARY" { DataType::Binary(n) } else { DataType::Varbinary(n) }
        }
        "CHAR" | "VARCHAR" if unify::is_number(first.data_type()) => {
            let n = char_length(first.data_type())?;
            if name == "CHAR" { DataType::Char(n) } else { DataType::Varchar(n) }
        }
        _ => return None,
    };
    Some(SqlType::not_null(t).or_null(null))
}

/// A computed value fitted to its column's type: numbers to the type
/// (an integer division's fraction cut off, a decimal to its scale), a
/// fixed-length string padded. A value the type cannot hold is the cast's
/// error.
pub fn conform(v: Value, t: &SqlType, mode: qdecimal::DfRounding) -> Result<Value> {
    if v.is_null() {
        return Ok(v);
    }
    match (t.data_type(), &v) {
        (
            DataType::SmallInt
            | DataType::Integer
            | DataType::BigInt
            | DataType::Decimal(..)
            | DataType::Numeric(..)
            | DataType::Real
            | DataType::Double
            | DataType::DecFloat(_),
            // Text in a number's column is read as the number.
            Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_) | Value::Str(_),
        ) => super::cast::cast(&v, &t.spec, mode),
        // Bit data is bytes.
        (DataType::Char(n) | DataType::Varchar(n), Value::Str(s)) if t.spec.bit_data => {
            let mut b = v.as_bytes()?;
            if matches!(t.data_type(), DataType::Char(_)) {
                b.resize(usize::try_from(*n).unwrap_or(0).max(b.len()), 0x40);
            }
            let _ = s;
            Ok(Value::Bin(b))
        }
        // A timestamp to the column's precision.
        (DataType::Timestamp(p), Value::Timestamp(ts)) if ts.precision != *p => Ok(Value::Timestamp(ts.with_precision(*p))),
        (DataType::Char(n) | DataType::Graphic(n), Value::Str(s)) => {
            let have = s.chars().count();
            let n = usize::try_from(*n).unwrap_or(usize::MAX);
            if have >= n {
                return Ok(v);
            }
            let mut s = s.clone();
            s.extend(std::iter::repeat_n(' ', n - have));
            Ok(Value::Str(s))
        }
        _ => Ok(v),
    }
}

/// The casts IBM refuses by type alone, before a row is read: a number,
/// date or time and a binary string, a number and a date or time, a
/// LOB and a date or time (`SQL0461`); binary or bit data to a graphic
/// string (`SQL0332`); a time to a timestamp (`SQL0171`).
pub fn check_cast(from: &SqlType, to: &TypeSpec) -> Result<()> {
    let (x, y) = (from.data_type(), &to.data_type);
    let datetime = |t: &DataType| matches!(t, DataType::Date | DataType::Time | DataType::Timestamp(_));
    let lob = |t: &DataType| matches!(t, DataType::Clob(_) | DataType::Dbclob(_) | DataType::Blob(_));
    let graphic = |t: &DataType| matches!(t, DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_));
    let refused = (unify::is_number(x) && (unify::is_binary(y) || datetime(y)))
        || ((unify::is_binary(x) || datetime(x)) && unify::is_number(y))
        || (lob(x) && datetime(y))
        || (matches!(x, DataType::Blob(_)) && unify::is_text(y))
        || (datetime(x) && unify::is_binary(y))
        || (unify::is_binary(x) && datetime(y));
    if refused {
        return Err(crate::error::SqlError::Coded {
            code: -461,
            state: "42846",
            what: format!(
                "a {} value cannot be cast to {}",
                super::cast::type_name(x),
                super::cast::type_name(y)
            ),
        });
    }
    if graphic(y) && (unify::is_binary(x) || from.spec.bit_data) {
        return Err(crate::error::SqlError::Coded {
            code: -332,
            state: "57017",
            what: format!("data in CCSID 65535 cannot be converted to CCSID {}", to.ccsid.unwrap_or(1200)),
        });
    }
    if *x == DataType::Time && matches!(y, DataType::Timestamp(_)) {
        return Err(crate::error::SqlError::Coded {
            code: -171,
            state: "42815",
            what: "argument 1 of CAST is a TIME, which has no timestamp".into(),
        });
    }
    if datetime(x) && unify::is_text(y) {
        let need = char_length(x).unwrap_or(0);
        if char_length(y).is_some_and(|n| n < need.min(19)) {
            return Err(crate::error::SqlError::Coded {
                code: -461,
                state: "42846",
                what: format!("{} needs more characters than {}", super::cast::type_name(x), super::cast::type_name(y)),
            });
        }
    }
    Ok(())
}

/// The columns of a `UNION`: each the type both sides' columns have in
/// common, written into `left`; the types the rows are fitted to (`None`
/// where a side's type is not known).
pub fn union_types(left: &mut [super::eval::ColInfo], right: &[&super::eval::ColInfo]) -> Vec<Option<SqlType>> {
    let mut out = Vec::with_capacity(left.len());
    for (l, r) in left.iter_mut().zip(right) {
        let t = match (&l.field, &r.field) {
            (Some(a), Some(b)) => unify(&of_field(a).or_null(l.nullable), &of_field(b).or_null(r.nullable)),
            _ => None,
        };
        if let Some(t) = &t {
            l.field = Some(field_of(t));
            l.nullable = t.nullable;
        }
        out.push(t);
    }
    out
}

/// The columns of a `VALUES`: each the rows' common type.
pub fn values_types(rows: &[Vec<Expr>], width: usize, scope: &Scope<'_>) -> Result<Vec<Option<SqlType>>> {
    (0..width).map(|i| unify::all(rows.iter().filter_map(|r| r.get(i)), scope, false)).collect()
}

/// The types of the sequences `exprs` name, for their `NEXT VALUE` and
/// `PREVIOUS VALUE`; a sequence not found is left out.
pub fn sequences<'e>(
    txn: &mut store::txn::Txn<'_>,
    ctx: &super::SqlContext,
    exprs: impl Iterator<Item = &'e Expr>,
) -> Vec<(crate::ast::ObjectName, DataType)> {
    fn names(e: &Expr, out: &mut Vec<crate::ast::ObjectName>) {
        if let Expr::NextValue(n) | Expr::PrevValue(n) = e
            && !out.contains(n)
        {
            out.push(n.clone());
        }
        for c in crate::ast::visit::children(e) {
            names(c, out);
        }
    }
    let mut found = Vec::new();
    for e in exprs {
        names(e, &mut found);
    }
    found.into_iter().filter_map(|n| super::seq::data_type(txn, ctx, &n).map(|t| (n, t))).collect()
}

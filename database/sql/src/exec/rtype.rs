// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Result types of expressions: what a result column describes as, and
//! the value conformed to it (a decimal to its scale, an integer
//! truncated). Columns keep their field; an expression gets a field made
//! from the type worked out here. Function families add their own rules
//! through [`FUNCTION_TYPES`].

use dds::{DataType as D, FieldDef, FloatPrecision, VarLen};
use qdecimal::{Decimal, Rounding, Shape};
use qsname::Name;

use super::eval::Schema;
use crate::ast::{BinOp, DataType, Expr, Function, Literal, Register};
use crate::value::{Timestamp, Value};

/// The kinds of result types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `SMALLINT`.
    SmallInt,
    /// `INTEGER`.
    Integer,
    /// `BIGINT`.
    BigInt,
    /// `DECIMAL(p, s)`.
    Decimal,
    /// `NUMERIC(p, s)`.
    Numeric,
    /// `REAL`.
    Real,
    /// `DOUBLE`.
    Double,
    /// `DECFLOAT(16|34)`: the length is the digits.
    DecFloat,
    /// `CHAR(n)`.
    Char,
    /// `VARCHAR(n)`.
    Varchar,
    /// `GRAPHIC(n)`.
    Graphic,
    /// `VARGRAPHIC(n)`.
    Vargraphic,
    /// `BINARY(n)`.
    Binary,
    /// `VARBINARY(n)`.
    Varbinary,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP(p)`: the scale is the precision.
    Timestamp,
    /// `BOOLEAN`.
    Boolean,
    /// `CLOB(n)`.
    Clob,
    /// `BLOB(n)`.
    Blob,
    /// `DBCLOB(n)`.
    Dbclob,
}

/// A result type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlType {
    /// The kind.
    pub kind: Kind,
    /// Characters, bytes or digits.
    pub len: u32,
    /// Decimal places; a timestamp's precision.
    pub scale: u8,
    /// Null-capable.
    pub null: bool,
}

/// A rule for function result types, by name and argument types; `None`
/// when the family does not know the name.
pub type TypeRule = fn(&str, &[SqlType], &Function) -> Option<SqlType>;

/// The rules of the function families, asked before the built-in ones.
pub(crate) const FUNCTION_TYPES: &[TypeRule] = &[
    super::func::string::result_type,
    super::func::datetime::result_type,
    super::func::math::result_type,
    super::func::misc::result_type,
];

impl SqlType {
    /// A type.
    #[must_use]
    pub const fn new(kind: Kind, len: u32, scale: u8, null: bool) -> Self {
        Self { kind, len, scale, null }
    }

    /// `INTEGER`.
    #[must_use]
    pub const fn int(null: bool) -> Self {
        Self::new(Kind::Integer, 10, 0, null)
    }

    /// `BIGINT`.
    #[must_use]
    pub const fn bigint(null: bool) -> Self {
        Self::new(Kind::BigInt, 19, 0, null)
    }

    /// `SMALLINT`.
    #[must_use]
    pub const fn smallint(null: bool) -> Self {
        Self::new(Kind::SmallInt, 5, 0, null)
    }

    /// `DOUBLE`.
    #[must_use]
    pub const fn double(null: bool) -> Self {
        Self::new(Kind::Double, 53, 0, null)
    }

    /// `DECFLOAT(34)`.
    #[must_use]
    pub const fn decfloat(null: bool) -> Self {
        Self::new(Kind::DecFloat, 34, 0, null)
    }

    /// `DECIMAL(p, s)`.
    #[must_use]
    pub const fn decimal(p: u32, s: u8, null: bool) -> Self {
        Self::new(Kind::Decimal, p, s, null)
    }

    /// `VARCHAR(n)`.
    #[must_use]
    pub const fn varchar(n: u32, null: bool) -> Self {
        Self::new(Kind::Varchar, n, 0, null)
    }

    /// `CHAR(n)`.
    #[must_use]
    pub const fn char(n: u32, null: bool) -> Self {
        Self::new(Kind::Char, n, 0, null)
    }

    /// `TIMESTAMP(p)`.
    #[must_use]
    pub const fn timestamp(p: u8, null: bool) -> Self {
        Self::new(Kind::Timestamp, 0, p, null)
    }

    /// `DATE`.
    #[must_use]
    pub const fn date(null: bool) -> Self {
        Self::new(Kind::Date, 10, 0, null)
    }

    /// `TIME`.
    #[must_use]
    pub const fn time(null: bool) -> Self {
        Self::new(Kind::Time, 8, 0, null)
    }

    /// The same type, null-capable or not.
    #[must_use]
    pub const fn nullable(self, null: bool) -> Self {
        Self { null, ..self }
    }

    /// The type when nothing is known: `VARCHAR(32740)`.
    #[must_use]
    pub const fn unknown() -> Self {
        Self::varchar(32740, true)
    }

    /// Whether a number.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self.kind,
            Kind::SmallInt | Kind::Integer | Kind::BigInt | Kind::Decimal | Kind::Numeric | Kind::Real | Kind::Double | Kind::DecFloat
        )
    }

    /// Whether an integer.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(self.kind, Kind::SmallInt | Kind::Integer | Kind::BigInt)
    }

    /// Whether a character string.
    #[must_use]
    pub fn is_char(&self) -> bool {
        matches!(self.kind, Kind::Char | Kind::Varchar | Kind::Clob | Kind::Graphic | Kind::Vargraphic | Kind::Dbclob)
    }

    /// The precision and scale it has in decimal arithmetic.
    #[must_use]
    pub fn digits(&self) -> Option<(u32, u8)> {
        match self.kind {
            Kind::SmallInt => Some((5, 0)),
            Kind::Integer => Some((11, 0)),
            Kind::BigInt => Some((19, 0)),
            Kind::Decimal | Kind::Numeric => Some((self.len, self.scale)),
            _ => None,
        }
    }

    /// The type of a field.
    #[must_use]
    pub fn of_field(f: &FieldDef, null: bool) -> Self {
        let var = f.varlen.is_some();
        let k = match f.data_type {
            D::Char | D::Hex => {
                if var {
                    Kind::Varchar
                } else {
                    Kind::Char
                }
            }
            D::Graphic | D::DbcsOpen => {
                if var {
                    Kind::Vargraphic
                } else {
                    Kind::Graphic
                }
            }
            D::Zoned => Kind::Numeric,
            D::Packed => Kind::Decimal,
            D::Binary => match f.length {
                0..=4 => Kind::SmallInt,
                5..=9 => Kind::Integer,
                _ => Kind::BigInt,
            },
            D::Float => {
                if f.precision == FloatPrecision::Single {
                    Kind::Real
                } else {
                    Kind::Double
                }
            }
            D::Date => Kind::Date,
            D::Time => Kind::Time,
            D::Timestamp => return Self::timestamp(D::timestamp_precision(f.length), null),
            D::DecFloat => return Self::new(Kind::DecFloat, if f.length <= 16 { 16 } else { 34 }, 0, null),
            D::Bytes => {
                if var {
                    Kind::Varbinary
                } else {
                    Kind::Binary
                }
            }
            D::Boolean => Kind::Boolean,
            D::Blob => Kind::Blob,
            D::Clob => Kind::Clob,
            D::Dbclob => Kind::Dbclob,
        };
        match k {
            Kind::SmallInt => Self::smallint(null),
            Kind::Integer => Self::int(null),
            Kind::BigInt => Self::bigint(null),
            _ => Self::new(k, f.length, f.decimals, null),
        }
    }

    /// The type a `CAST` names.
    #[must_use]
    pub fn of_data_type(t: &DataType, null: bool) -> Self {
        match *t {
            DataType::Char(n) => Self::char(n, null),
            DataType::Varchar(n) => Self::varchar(n, null),
            DataType::Graphic(n) => Self::new(Kind::Graphic, n, 0, null),
            DataType::Vargraphic(n) => Self::new(Kind::Vargraphic, n, 0, null),
            DataType::Decimal(p, s) => Self::decimal(u32::from(p), s, null),
            DataType::Numeric(p, s) => Self::new(Kind::Numeric, u32::from(p), s, null),
            DataType::SmallInt => Self::smallint(null),
            DataType::Integer => Self::int(null),
            DataType::BigInt => Self::bigint(null),
            DataType::Real => Self::new(Kind::Real, 24, 0, null),
            DataType::Double => Self::double(null),
            DataType::Date => Self::date(null),
            DataType::Time => Self::time(null),
            DataType::Timestamp(p) => Self::timestamp(p, null),
            DataType::DecFloat(n) => Self::new(Kind::DecFloat, u32::from(n), 0, null),
            DataType::Binary(n) => Self::new(Kind::Binary, n, 0, null),
            DataType::Varbinary(n) => Self::new(Kind::Varbinary, n, 0, null),
            DataType::Boolean => Self::new(Kind::Boolean, 1, 0, null),
            DataType::Blob(n) => Self::new(Kind::Blob, n, 0, null),
            DataType::Clob(n) => Self::new(Kind::Clob, n, 0, null),
            DataType::Dbclob(n) => Self::new(Kind::Dbclob, n, 0, null),
        }
    }

    /// A field of this type, as an expression's result column carries it.
    #[must_use]
    pub fn to_field(&self) -> FieldDef {
        let name = Name::parse("EXPR").expect("valid");
        let n = self.len.max(1);
        let (t, len, dec) = match self.kind {
            Kind::SmallInt => (D::Binary, 4, 0),
            Kind::Integer => (D::Binary, 9, 0),
            Kind::BigInt => (D::Binary, 18, 0),
            Kind::Decimal => (D::Packed, n, self.scale),
            Kind::Numeric => (D::Zoned, n, self.scale),
            Kind::Real | Kind::Double => (D::Float, if self.kind == Kind::Real { 9 } else { 17 }, 0),
            Kind::DecFloat => (D::DecFloat, if self.len <= 16 { 16 } else { 34 }, 0),
            Kind::Char | Kind::Varchar => (D::Char, n, 0),
            Kind::Graphic | Kind::Vargraphic => (D::Graphic, n, 0),
            Kind::Binary | Kind::Varbinary => (D::Bytes, n, 0),
            Kind::Date => (D::Date, 10, 0),
            Kind::Time => (D::Time, 8, 0),
            Kind::Timestamp => (D::Timestamp, D::timestamp_length(self.scale), 0),
            Kind::Boolean => (D::Boolean, 1, 0),
            Kind::Clob => (D::Clob, n, 0),
            Kind::Blob => (D::Blob, n, 0),
            Kind::Dbclob => (D::Dbclob, n, 0),
        };
        let mut f = FieldDef::new(name, t, len, dec);
        if matches!(self.kind, Kind::Real) {
            f.precision = FloatPrecision::Single;
        } else if matches!(self.kind, Kind::Double) {
            f.precision = FloatPrecision::Double;
        }
        if matches!(self.kind, Kind::SmallInt | Kind::Integer | Kind::BigInt) {
            f.int_range = true;
        }
        if matches!(self.kind, Kind::Varchar | Kind::Vargraphic | Kind::Varbinary) {
            f.varlen = Some(VarLen { allocated: None });
        }
        f.byte_len = t.byte_len(len, f.precision);
        f.allow_null = self.null;
        f
    }
}

/// The type both of two types fit: the result of a `CASE`, `COALESCE`,
/// `UNION` or `VALUES` column.
#[must_use]
pub fn unify(a: SqlType, b: SqlType) -> SqlType {
    let null = a.null || b.null;
    if a.is_numeric() && b.is_numeric() {
        let rank = |k: Kind| match k {
            Kind::SmallInt => 0,
            Kind::Integer => 1,
            Kind::BigInt => 2,
            Kind::Decimal | Kind::Numeric => 3,
            Kind::Real => 4,
            Kind::Double => 5,
            _ => 6,
        };
        if let (Some((p1, s1)), Some((p2, s2))) = (a.digits(), b.digits())
            && (!a.is_integer() || !b.is_integer())
        {
            let s = s1.max(s2);
            let p = ((p1 - u32::from(s1)).max(p2 - u32::from(s2)) + u32::from(s)).min(63);
            return SqlType::decimal(p, s, null);
        }
        let t = if rank(a.kind) >= rank(b.kind) { a } else { b };
        return t.nullable(null);
    }
    if a.is_char() && b.is_char() {
        let var = a.kind != b.kind || a.kind == Kind::Varchar;
        let len = a.len.max(b.len);
        return SqlType::new(if var && a.kind == Kind::Char { Kind::Varchar } else { a.kind }, len, 0, null);
    }
    if a.kind == b.kind {
        return SqlType::new(a.kind, a.len.max(b.len), a.scale.max(b.scale), null);
    }
    a.nullable(null)
}

/// The type of `a op b`, by DB2 for i's rules for integers, decimals,
/// floats and strings.
#[must_use]
pub fn arith(a: SqlType, op: BinOp, b: SqlType) -> SqlType {
    let null = a.null || b.null;
    if op == BinOp::Concat {
        let var = a.kind == Kind::Varchar || b.kind == Kind::Varchar || a.kind == Kind::Clob;
        let kind = if a.kind == Kind::Clob || b.kind == Kind::Clob {
            Kind::Clob
        } else if matches!(a.kind, Kind::Binary | Kind::Varbinary) {
            Kind::Varbinary
        } else if var {
            Kind::Varchar
        } else {
            Kind::Char
        };
        return SqlType::new(kind, a.len.saturating_add(b.len).min(32740), 0, null);
    }
    // Date, time and timestamp arithmetic.
    match (a.kind, op, b.kind) {
        (Kind::Date, BinOp::Sub, Kind::Date) => return SqlType::decimal(8, 0, null),
        (Kind::Time, BinOp::Sub, Kind::Time) => return SqlType::decimal(6, 0, null),
        (Kind::Timestamp, BinOp::Sub, Kind::Timestamp) => {
            let p = a.scale.max(b.scale);
            return SqlType::decimal(14 + u32::from(p), p, null);
        }
        (Kind::Date | Kind::Time | Kind::Timestamp, _, _) => return a.nullable(null),
        (_, BinOp::Add, Kind::Date | Kind::Time | Kind::Timestamp) => return b.nullable(null),
        _ => {}
    }
    if a.kind == Kind::DecFloat || b.kind == Kind::DecFloat {
        let wide = [a, b].iter().any(|c| (c.kind == Kind::DecFloat && c.len > 16) || c.digits().is_some_and(|(p, _)| p > 16));
        return SqlType::new(Kind::DecFloat, if wide { 34 } else { 16 }, 0, null);
    }
    if a.is_integer() && b.is_integer() {
        return if a.kind == Kind::BigInt || b.kind == Kind::BigInt { SqlType::bigint(null) } else { SqlType::int(null) };
    }
    if matches!(a.kind, Kind::Real | Kind::Double) || matches!(b.kind, Kind::Real | Kind::Double) || a.is_char() || b.is_char() {
        return SqlType::double(null);
    }
    let (Some((p1, s1)), Some((p2, s2))) = (a.digits(), b.digits()) else {
        return SqlType::double(null);
    };
    let mp = if p1 > 31 || p2 > 31 { 63 } else { 31 };
    let (s1, s2) = (u32::from(s1), u32::from(s2));
    let (p, s) = match op {
        BinOp::Mul => {
            let p = (p1 + p2).min(mp);
            (p, (s1 + s2).min(p))
        }
        BinOp::Div => (mp, (mp + s1).saturating_sub(p1 + s2)),
        _ => {
            let s = s1.max(s2);
            (((p1 - s1).max(p2 - s2) + s + 1).min(mp), s)
        }
    };
    SqlType::decimal(p, u8::try_from(s.min(p)).expect("small"), null)
}

/// The frames an expression's columns are looked up in, innermost last.
pub struct Scope<'a> {
    /// The frames.
    pub frames: Vec<&'a Schema>,
}

impl Scope<'_> {
    pub(crate) fn column(&self, table: Option<&str>, name: &str) -> Option<SqlType> {
        for s in self.frames.iter().rev() {
            if let Ok(Some(i)) = s.find(table, name) {
                let c = &s.cols[i];
                return Some(c.field.as_ref().map_or_else(SqlType::unknown, |f| SqlType::of_field(f, c.nullable)).nullable(c.nullable));
            }
        }
        None
    }
}

/// A number literal's type: an integer, a double with an exponent, or a
/// decimal of its digits.
#[must_use]
pub fn num_literal(n: &str) -> SqlType {
    if n.contains(['E', 'e']) {
        return SqlType::double(false);
    }
    let digits = n.chars().filter(char::is_ascii_digit).count();
    if !n.contains('.') {
        return match digits {
            0..=9 => SqlType::int(false),
            10..=18 => SqlType::bigint(false),
            _ => SqlType::decimal(u32::try_from(digits).unwrap_or(63).min(63), 0, false),
        };
    }
    let scale = n.split_once('.').map_or(0, |(_, f)| f.chars().filter(char::is_ascii_digit).count());
    let p = u32::try_from(digits.max(1)).unwrap_or(63).min(63);
    SqlType::decimal(p, u8::try_from(scale).unwrap_or(0).min(63), false)
}

/// The type of an expression in a scope.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn infer(e: &Expr, scope: &Scope<'_>) -> SqlType {
    match e {
        Expr::Column(c) => scope
            .column(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value)
            .unwrap_or_else(SqlType::unknown),
        Expr::Literal(l) => match l {
            Literal::Num(n) => num_literal(n),
            Literal::Str(s) => SqlType::varchar(u32::try_from(s.chars().count()).unwrap_or(32740), false),
            Literal::Hex(h) => SqlType::varchar(u32::try_from(h.len()).unwrap_or(32740), false),
            Literal::Bin(b) => SqlType::new(Kind::Varbinary, u32::try_from(b.len()).unwrap_or(32740), 0, false),
            Literal::Bool(_) => SqlType::new(Kind::Boolean, 1, 0, false),
            Literal::Null => SqlType::int(true),
            Literal::Date(_) => SqlType::date(false),
            Literal::Time(_) => SqlType::time(false),
            Literal::Timestamp(s) => SqlType::timestamp(Timestamp::parse(s).map_or(6, |t| t.precision), false),
        },
        Expr::Register(r) => match r {
            Register::Date => SqlType::date(false),
            Register::Time => SqlType::time(false),
            Register::Timestamp(p) => SqlType::timestamp(*p, false),
            Register::Timezone => SqlType::decimal(6, 0, false),
            Register::User | Register::Schema => SqlType::varchar(128, false),
            Register::Server => SqlType::varchar(18, false),
            Register::Path => SqlType::varchar(8843, false),
            Register::Named(n) => super::registers::result_type(n),
            Register::DecfloatRounding => SqlType::varchar(128, false),
        },
        Expr::Neg(a) | Expr::Duration(a, _) => infer(a, scope),
        Expr::Binary(a, op, b) => {
            // A labeled duration takes the type of the other side.
            if let Expr::Duration(..) = &**b {
                return infer(a, scope);
            }
            if let Expr::Duration(..) = &**a {
                return infer(b, scope);
            }
            arith(infer(a, scope), *op, infer(b, scope))
        }
        Expr::And(..)
        | Expr::Or(..)
        | Expr::Not(_)
        | Expr::IsNull(..)
        | Expr::Compare(..)
        | Expr::Between(..)
        | Expr::InList(..)
        | Expr::InSubquery(..)
        | Expr::Like(..)
        | Expr::Exists(..) => SqlType::new(Kind::Boolean, 1, 0, true),
        Expr::Case { whens, otherwise, .. } => {
            let mut t: Option<SqlType> = None;
            for (_, v) in whens {
                if matches!(v, Expr::Literal(Literal::Null)) {
                    continue;
                }
                let x = infer(v, scope);
                t = Some(t.map_or(x, |t| unify(t, x)));
            }
            let mut null = otherwise.is_none() || whens.iter().any(|(_, v)| matches!(v, Expr::Literal(Literal::Null)));
            if let Some(o) = otherwise {
                if matches!(**o, Expr::Literal(Literal::Null)) {
                    null = true;
                } else {
                    let x = infer(o, scope);
                    t = Some(t.map_or(x, |t| unify(t, x)));
                }
            }
            let t = t.unwrap_or_else(|| SqlType::int(true));
            t.nullable(t.null || null)
        }
        Expr::Cast(a, t) => SqlType::of_data_type(&t.data_type, infer(a, scope).null),
        Expr::Function(f) => function(f, scope),
        Expr::WithinGroup(w) => {
            if w.func.name == "PERCENTILE_DISC" && let Some(o) = w.order.first() {
                return infer(&o.expr, scope).nullable(true);
            }
            function(&w.func, scope)
        }
        Expr::Over(o) => super::olap::result_type(o, scope),
        Expr::Row(items) if items.len() == 1 => infer(&items[0], scope),
        _ => SqlType::unknown(),
    }
}

#[allow(clippy::too_many_lines)]
fn function(f: &Function, scope: &Scope<'_>) -> SqlType {
    // A name that is no column is a keyword argument (EXTRACT's part,
    // CHAR's format, STRIP's end); the type CAST names for INTERPRET.
    let args: Vec<SqlType> = f
        .args
        .iter()
        .map(|a| match a {
            Expr::Column(c) if c.table.is_none() && scope.column(None, &c.column.value).is_none() => SqlType::varchar(10, false),
            Expr::Cast(x, t) if matches!(**x, Expr::Literal(Literal::Null)) && f.name == "INTERPRET" => SqlType::of_data_type(&t.data_type, false),
            a => infer(a, scope),
        })
        .collect();
    let null = args.iter().any(|a| a.null);
    if let Some(t) = super::agg::result_type(f, &args) {
        return t;
    }
    for rule in FUNCTION_TYPES {
        if let Some(t) = rule(&f.name, &args, f) {
            return t;
        }
    }
    let first = args.first().copied();
    match f.name.as_str() {
        "COALESCE" | "VALUE" | "IFNULL" | "NVL" => {
            // The null constants take the others' type.
            let typed: Vec<SqlType> =
                args.iter().zip(&f.args).filter(|(_, e)| !matches!(e, Expr::Literal(Literal::Null))).map(|(t, _)| *t).collect();
            let mut t = typed.first().copied().unwrap_or_else(SqlType::unknown);
            for a in typed.iter().skip(1) {
                t = unify(t, *a);
            }
            t.nullable(typed.iter().all(|a| a.null) || typed.is_empty())
        }
        "NULLIF" => first.unwrap_or_else(SqlType::unknown).nullable(true),
        "MAX" | "MIN" | "GREATEST" | "LEAST" => {
            let mut t = first.unwrap_or_else(SqlType::unknown);
            for a in &args[1.min(args.len())..] {
                t = unify(t, *a);
            }
            t.nullable(null)
        }
        "DATE" => SqlType::date(null),
        "TIME" => SqlType::time(null),
        "TIMESTAMP" => {
            let p = match f.args.get(1) {
                Some(Expr::Literal(Literal::Num(n))) => n.parse().unwrap_or(6),
                _ => 6,
            };
            SqlType::timestamp(p, null)
        }
        "NOW" => SqlType::timestamp(6, false),
        "BIGINT" => SqlType::bigint(null),
        "SMALLINT" => SqlType::smallint(null),
        "LENGTH" | "CHAR_LENGTH" | "CHARACTER_LENGTH" | "OCTET_LENGTH" | "YEAR" | "MONTH" | "DAY" | "DAYOFMONTH"
        | "HOUR" | "MINUTE" | "SECOND" | "MICROSECOND" | "DAYS" | "POSSTR" | "LOCATE" | "POSITION" | "DAYOFWEEK"
        | "DAYOFWEEK_ISO" | "DAYOFYEAR" | "WEEK" | "LEAP" | "JULIAN_DAY" | "TIMESTAMPDIFF" | "INTEGER" | "INT" => {
            SqlType::int(null)
        }
        "DOUBLE" | "DOUBLE_PRECISION" | "FLOAT" | "POWER" | "POW" | "SQRT" => SqlType::double(null),
        "REAL" => SqlType::new(Kind::Real, 24, 0, null),
        "DECFLOAT" => {
            let narrow = matches!(f.args.get(1), Some(Expr::Literal(Literal::Num(n))) if n == "16");
            SqlType::new(Kind::DecFloat, if narrow { 16 } else { 34 }, 0, null)
        }
        "NORMALIZE_DECFLOAT" => first.unwrap_or_else(|| SqlType::decfloat(null)),
        "BOOLEAN" => SqlType::new(Kind::Boolean, 1, 0, null),
        "DECIMAL" | "DEC" | "ZONED" => {
            let num = |i: usize, d: u32| match f.args.get(i) {
                Some(Expr::Literal(Literal::Num(n))) => n.parse().unwrap_or(d),
                _ => d,
            };
            let (dp, ds) = first.and_then(|t| t.digits()).unwrap_or((15, 0));
            let p = num(1, if f.args.len() > 1 { 15 } else { dp });
            let s = num(2, if f.args.len() > 1 { 0 } else { u32::from(ds) });
            let kind = if f.name == "ZONED" { Kind::Numeric } else { Kind::Decimal };
            SqlType::new(kind, p.clamp(1, 63), u8::try_from(s.min(p)).unwrap_or(0), null)
        }
        "BINARY" | "VARBINARY" => {
            let n = match f.args.get(1) {
                Some(Expr::Literal(Literal::Num(n))) => n.parse().unwrap_or(1),
                _ => first.map_or(1, |t| t.len),
            };
            SqlType::new(if f.name == "BINARY" { Kind::Binary } else { Kind::Varbinary }, n, 0, null)
        }
        "CHAR" | "VARCHAR" => {
            let kind = if f.name == "CHAR" { Kind::Char } else { Kind::Varchar };
            let n = match (f.args.get(1), first) {
                (Some(Expr::Literal(Literal::Num(n))), _) => n.parse().unwrap_or(1),
                (_, Some(t)) => match t.kind {
                    Kind::Date => 10,
                    Kind::Time => 8,
                    Kind::Timestamp => crate::exec::rtype::ts_chars(t.scale),
                    Kind::SmallInt => 6,
                    Kind::Integer => 11,
                    Kind::BigInt => 20,
                    Kind::Decimal | Kind::Numeric => t.len + 2,
                    Kind::Double | Kind::Real => 24,
                    _ => t.len,
                },
                _ => 1,
            };
            SqlType::new(kind, n, 0, null)
        }
        "DIGITS" => SqlType::char(first.and_then(|t| t.digits()).map_or(15, |(p, _)| if p == 11 { 10 } else { p }), null),
        "SUBSTR" | "SUBSTRING" | "LEFT" | "RIGHT" | "TRIM" | "LTRIM" | "RTRIM" | "STRIP" | "UPPER" | "UCASE" | "LOWER"
        | "LCASE" => {
            // The source: last after TRIM's end word, else first.
            let moded = matches!(f.args.first(), Some(Expr::Literal(Literal::Str(m))) if matches!(m.as_str(), "BOTH" | "LEADING" | "TRAILING" | "B" | "L" | "T"));
            let t = if moded { args.last().copied() } else { first };
            let t = t.unwrap_or_else(SqlType::unknown);
            let kind = match t.kind {
                Kind::Char if matches!(f.name.as_str(), "UPPER" | "UCASE" | "LOWER" | "LCASE") => Kind::Char,
                Kind::Binary | Kind::Varbinary => Kind::Varbinary,
                Kind::Clob => Kind::Clob,
                _ => Kind::Varchar,
            };
            SqlType::new(kind, t.len, 0, null)
        }
        "CONCAT" => arith(first.unwrap_or_else(SqlType::unknown), BinOp::Concat, args.get(1).copied().unwrap_or_else(SqlType::unknown)),
        "REPLACE" => SqlType::varchar(first.map_or(4000, |t| t.len.max(1)).saturating_mul(4).min(32740), null),
        "HEX" => SqlType::varchar(first.map_or(2, |t| byte_size(&t) * 2), null),
        "ABS" | "ABSVAL" | "MOD" | "ROUND" | "TRUNCATE" | "TRUNC" | "CEILING" | "CEIL" | "FLOOR" => {
            let t = first.unwrap_or_else(|| SqlType::int(null));
            if f.name == "MOD" {
                let b = args.get(1).copied().unwrap_or(t);
                if t.is_integer() && b.is_integer() {
                    return arith(t, BinOp::Add, b).nullable(null);
                }
            }
            t.nullable(null)
        }
        "RRN" => SqlType::int(false),
        "GROUPING" => SqlType::smallint(true),
        "INTERPRET" => match f.args.get(1) {
            Some(Expr::Cast(_, t)) => SqlType::of_data_type(&t.data_type, null),
            _ => SqlType::unknown(),
        },
        "*DISTINCT" | "*NOTDISTINCT" => SqlType::new(Kind::Boolean, 1, 0, false),
        "ADD_MONTHS" | "LAST_DAY" | "NEXT_DAY" => first.unwrap_or_else(|| SqlType::date(null)).nullable(null),
        "DAYNAME" | "MONTHNAME" => SqlType::varchar(100, null),
        _ => SqlType::unknown(),
    }
}

/// The characters of `TIMESTAMP(p)` as text.
#[must_use]
pub fn ts_chars(p: u8) -> u32 {
    if p == 0 { 19 } else { 20 + u32::from(p) }
}

/// Bytes a value of the type takes, for `HEX`.
#[must_use]
pub fn byte_size(t: &SqlType) -> u32 {
    match t.kind {
        Kind::SmallInt => 2,
        Kind::Integer | Kind::Real | Kind::Date => 4,
        Kind::BigInt | Kind::Double => 8,
        Kind::Decimal => t.len / 2 + 1,
        Kind::DecFloat => if t.len <= 16 { 8 } else { 16 },
        Kind::Time => 3,
        Kind::Timestamp => 10,
        Kind::Graphic | Kind::Vargraphic | Kind::Dbclob => t.len * 2,
        _ => t.len,
    }
}

/// A value as a result column of type `t` holds it: a number to the
/// type's scale (truncated), an integer whole, a float or `DECFLOAT`
/// converted. Anything that does not convert is left as it is.
#[must_use]
pub fn conform(v: Value, t: &SqlType) -> Value {
    match (&v, t.kind) {
        (Value::Num(d), Kind::Decimal | Kind::Numeric) => rescale(d, t.scale).map_or(v, Value::Num),
        (Value::Num(d), Kind::SmallInt | Kind::Integer | Kind::BigInt) if d.scale() > 0 => rescale(d, 0).map_or(v, Value::Num),
        (Value::Num(_) | Value::DecFloat(_) | Value::Real(_), Kind::Double) => v.as_f64().map_or(v, Value::Double),
        (Value::Num(_), Kind::DecFloat) => {
            let fmt = if t.len <= 16 { qdecimal::DfFormat::D16 } else { qdecimal::DfFormat::D34 };
            v.as_decfloat(fmt).map_or(v, Value::DecFloat)
        }
        (Value::Timestamp(ts), Kind::Timestamp) if ts.precision != t.scale => Value::Timestamp(ts.with_precision(t.scale)),
        (Value::Str(s), Kind::Char) if t.len > 0 && t.len < 32741 => {
            let n = usize::try_from(t.len).unwrap_or(0);
            let have = s.chars().count();
            if have < n {
                let mut s = s.clone();
                s.extend(std::iter::repeat_n(' ', n - have));
                Value::Str(s)
            } else {
                v
            }
        }
        _ => v,
    }
}

fn rescale(d: &Decimal, scale: u8) -> Option<Decimal> {
    if d.scale() == scale {
        return Some(d.clone());
    }
    let r = d.rescale(scale, Rounding::Truncate).ok()?;
    // Past 63 digits the value stays as it is.
    Shape::new(63, scale).ok()?;
    Some(r)
}

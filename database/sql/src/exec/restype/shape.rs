// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Short ways to make and read a [`SqlType`]: `SqlType::int(null)`,
//! `t.kind()`, `t.len()`, `t.digits()`. The rules that work by kind,
//! length and scale (aggregates, OLAP, the function families) read a
//! type through these.

use dds::FieldDef;

use super::{SqlType, field_of, of_field};
use crate::ast::{BinOp, DataType, TypeSpec};

/// A type's kind, its data type without length or scale.
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

fn p8(n: u32) -> u8 {
    u8::try_from(n.min(63)).unwrap_or(63)
}

impl SqlType {
    /// A type of `kind` with its length (characters, bytes or digits) and
    /// scale (decimal places, a timestamp's precision).
    #[must_use]
    pub fn new(kind: Kind, len: u32, scale: u8, null: bool) -> Self {
        let t = match kind {
            Kind::SmallInt => DataType::SmallInt,
            Kind::Integer => DataType::Integer,
            Kind::BigInt => DataType::BigInt,
            Kind::Decimal => DataType::Decimal(p8(len.max(1)), scale.min(p8(len.max(1)))),
            Kind::Numeric => DataType::Numeric(p8(len.max(1)), scale.min(p8(len.max(1)))),
            Kind::Real => DataType::Real,
            Kind::Double => DataType::Double,
            Kind::DecFloat => DataType::DecFloat(if len <= 16 { 16 } else { 34 }),
            Kind::Char => DataType::Char(len.max(1)),
            Kind::Varchar => DataType::Varchar(len.max(1)),
            Kind::Graphic => DataType::Graphic(len.max(1)),
            Kind::Vargraphic => DataType::Vargraphic(len.max(1)),
            Kind::Binary => DataType::Binary(len.max(1)),
            Kind::Varbinary => DataType::Varbinary(len.max(1)),
            Kind::Date => DataType::Date,
            Kind::Time => DataType::Time,
            Kind::Timestamp => DataType::Timestamp(scale.min(12)),
            Kind::Boolean => DataType::Boolean,
            Kind::Clob => DataType::Clob(len.max(1)),
            Kind::Blob => DataType::Blob(len.max(1)),
            Kind::Dbclob => DataType::Dbclob(len.max(1)),
        };
        Self::of_data_type(&t, null)
    }

    /// The type a data type names.
    #[must_use]
    pub fn of_data_type(t: &DataType, null: bool) -> Self {
        Self {
            spec: TypeSpec {
                data_type: t.clone(),
                ccsid: None,
                bit_data: false,
                udt: None,
            },
            nullable: null,
        }
    }

    /// A column's type, null-capable as `null` says.
    #[must_use]
    pub fn of_column(f: &FieldDef, null: bool) -> Self {
        of_field(f).nullable(null)
    }

    /// The field an expression's result column carries.
    #[must_use]
    pub fn to_field(&self) -> FieldDef {
        field_of(self)
    }

    /// `INTEGER`.
    #[must_use]
    pub fn int(null: bool) -> Self {
        Self::of_data_type(&DataType::Integer, null)
    }

    /// `BIGINT`.
    #[must_use]
    pub fn bigint(null: bool) -> Self {
        Self::of_data_type(&DataType::BigInt, null)
    }

    /// `SMALLINT`.
    #[must_use]
    pub fn smallint(null: bool) -> Self {
        Self::of_data_type(&DataType::SmallInt, null)
    }

    /// `DOUBLE`.
    #[must_use]
    pub fn double(null: bool) -> Self {
        Self::of_data_type(&DataType::Double, null)
    }

    /// `DECFLOAT(34)`.
    #[must_use]
    pub fn decfloat(null: bool) -> Self {
        Self::of_data_type(&DataType::DecFloat(34), null)
    }

    /// `DECIMAL(p, s)`.
    #[must_use]
    pub fn decimal(p: u32, s: u8, null: bool) -> Self {
        Self::new(Kind::Decimal, p, s, null)
    }

    /// `VARCHAR(n)`.
    #[must_use]
    pub fn varchar(n: u32, null: bool) -> Self {
        Self::new(Kind::Varchar, n, 0, null)
    }

    /// `CHAR(n)`.
    #[must_use]
    pub fn char(n: u32, null: bool) -> Self {
        Self::new(Kind::Char, n, 0, null)
    }

    /// `TIMESTAMP(p)`.
    #[must_use]
    pub fn timestamp(p: u8, null: bool) -> Self {
        Self::new(Kind::Timestamp, 0, p, null)
    }

    /// `DATE`.
    #[must_use]
    pub fn date(null: bool) -> Self {
        Self::of_data_type(&DataType::Date, null)
    }

    /// `TIME`.
    #[must_use]
    pub fn time(null: bool) -> Self {
        Self::of_data_type(&DataType::Time, null)
    }

    /// `BOOLEAN`.
    #[must_use]
    pub fn boolean(null: bool) -> Self {
        Self::of_data_type(&DataType::Boolean, null)
    }

    /// What a rule takes for an argument of no known type:
    /// `VARCHAR(32740)`.
    #[must_use]
    pub fn unknown() -> Self {
        Self::varchar(32740, true)
    }

    /// The same type, null-capable or not.
    #[must_use]
    pub fn nullable(mut self, null: bool) -> Self {
        self.nullable = null;
        self
    }

    /// The kind.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self.spec.data_type {
            DataType::Char(_) => Kind::Char,
            DataType::Varchar(_) => Kind::Varchar,
            DataType::Graphic(_) => Kind::Graphic,
            DataType::Vargraphic(_) => Kind::Vargraphic,
            DataType::Decimal(..) => Kind::Decimal,
            DataType::Numeric(..) => Kind::Numeric,
            DataType::SmallInt => Kind::SmallInt,
            DataType::Integer => Kind::Integer,
            DataType::BigInt => Kind::BigInt,
            DataType::Real => Kind::Real,
            DataType::Double => Kind::Double,
            DataType::Date => Kind::Date,
            DataType::Time => Kind::Time,
            DataType::Timestamp(_) => Kind::Timestamp,
            DataType::DecFloat(_) => Kind::DecFloat,
            DataType::Binary(_) => Kind::Binary,
            DataType::Varbinary(_) => Kind::Varbinary,
            DataType::Boolean => Kind::Boolean,
            DataType::Blob(_) => Kind::Blob,
            DataType::Clob(_) => Kind::Clob,
            DataType::Dbclob(_) => Kind::Dbclob,
        }
    }

    /// Characters, bytes or digits: a type's length, not a collection's.
    #[must_use]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u32 {
        match self.spec.data_type {
            DataType::Char(n)
            | DataType::Varchar(n)
            | DataType::Graphic(n)
            | DataType::Vargraphic(n)
            | DataType::Binary(n)
            | DataType::Varbinary(n)
            | DataType::Blob(n)
            | DataType::Clob(n)
            | DataType::Dbclob(n) => n,
            DataType::Decimal(p, _) | DataType::Numeric(p, _) => u32::from(p),
            DataType::DecFloat(n) => u32::from(n),
            DataType::SmallInt => 5,
            DataType::Integer | DataType::Date => 10,
            DataType::BigInt => 19,
            DataType::Real => 24,
            DataType::Double => 53,
            DataType::Time => 8,
            DataType::Timestamp(_) => 0,
            DataType::Boolean => 1,
        }
    }

    /// Decimal places; a timestamp's precision.
    #[must_use]
    pub fn scale(&self) -> u8 {
        match self.spec.data_type {
            DataType::Decimal(_, s) | DataType::Numeric(_, s) => s,
            DataType::Timestamp(p) => p,
            _ => 0,
        }
    }

    /// Whether a number.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        super::unify::is_number(&self.spec.data_type)
    }

    /// Whether an integer.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(
            self.spec.data_type,
            DataType::SmallInt | DataType::Integer | DataType::BigInt
        )
    }

    /// Whether a character string.
    #[must_use]
    pub fn is_char(&self) -> bool {
        super::unify::is_text(&self.spec.data_type)
    }

    /// The precision and scale it has in decimal arithmetic.
    #[must_use]
    pub fn digits(&self) -> Option<(u32, u8)> {
        match self.spec.data_type {
            DataType::SmallInt => Some((5, 0)),
            DataType::Integer => Some((11, 0)),
            DataType::BigInt => Some((19, 0)),
            DataType::Decimal(p, s) | DataType::Numeric(p, s) => Some((u32::from(p), s)),
            _ => None,
        }
    }
}

/// [`super::unify`], or `a` (null-capable when either is) when the two
/// have no common type.
#[must_use]
pub fn unify_or(a: SqlType, b: &SqlType) -> SqlType {
    let null = a.nullable || b.nullable;
    super::unify(&a, b).unwrap_or_else(|| a.nullable(null))
}

/// [`super::arith`] or [`super::unify::concat`], `DOUBLE` when IBM's
/// rules give none.
#[must_use]
pub fn arith_or(a: &SqlType, op: BinOp, b: &SqlType) -> SqlType {
    let t = if op == BinOp::Concat {
        super::unify::concat(a, b)
    } else {
        super::arith(a, op, b)
    };
    t.unwrap_or_else(|| SqlType::double(a.nullable || b.nullable))
}

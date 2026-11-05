// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's rules for the type of an operation over two types: arithmetic
//! (integers widen, decimals by precision and scale to 31 digits,
//! floats are doubles, `DECFLOAT(34)` when either side needs it),
//! concatenation and the result of `CASE`, `COALESCE` and `UNION`.

#![allow(clippy::many_single_char_names)]

use super::{Scope, SqlType, char_length, type_in};
use crate::ast::{BinOp, DataType, Expr, TypeSpec};
use crate::error::{Result, SqlError};

/// The largest decimal precision of a result (`QAQQINI`'s default 31).
const MAX_PRECISION: u8 = 31;

/// Whether a number type.
#[must_use]
pub fn is_number(t: &DataType) -> bool {
    matches!(
        t,
        DataType::SmallInt
            | DataType::Integer
            | DataType::BigInt
            | DataType::Decimal(..)
            | DataType::Numeric(..)
            | DataType::Real
            | DataType::Double
            | DataType::DecFloat(_)
    )
}

pub(crate) fn is_text(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Char(_)
            | DataType::Varchar(_)
            | DataType::Graphic(_)
            | DataType::Vargraphic(_)
            | DataType::Clob(_)
            | DataType::Dbclob(_)
    )
}

pub(crate) fn is_binary(t: &DataType) -> bool {
    matches!(t, DataType::Binary(_) | DataType::Varbinary(_) | DataType::Blob(_))
}

/// An integer or decimal's precision and scale in decimal arithmetic.
fn as_decimal(t: &DataType) -> Option<(u8, u8)> {
    Some(match *t {
        DataType::SmallInt => (5, 0),
        DataType::Integer => (11, 0),
        DataType::BigInt => (19, 0),
        DataType::Decimal(p, s) | DataType::Numeric(p, s) => (p, s),
        _ => return None,
    })
}

/// The `DECFLOAT` digits of an operation with a `DECFLOAT`: 34 when
/// either is 34, or the other is a `BIGINT` or a decimal of more than 16
/// digits; else 16.
fn decfloat_digits(a: &DataType, b: &DataType) -> u8 {
    let wide = |t: &DataType| match t {
        DataType::DecFloat(n) => *n == 34,
        DataType::BigInt => true,
        DataType::Decimal(p, _) | DataType::Numeric(p, _) => *p > 16,
        _ => false,
    };
    if wide(a) || wide(b) { 34 } else { 16 }
}

fn integer_rank(t: &DataType) -> Option<u8> {
    match t {
        DataType::SmallInt => Some(0),
        DataType::Integer => Some(1),
        DataType::BigInt => Some(2),
        _ => None,
    }
}

/// The type of `a op b` for numbers; `None` for operands that are not.
#[must_use]
pub fn arith(a: &SqlType, op: BinOp, b: &SqlType) -> Option<SqlType> {
    let (x, y) = (a.data_type(), b.data_type());
    let nullable = a.nullable || b.nullable;
    // Text beside a number takes the number's type.
    let (x, y) = match (is_number(x), is_number(y)) {
        (true, true) => (x, y),
        (true, false) if is_text(y) => (x, x),
        (false, true) if is_text(x) => (y, y),
        _ => return None,
    };
    let t = if matches!(x, DataType::DecFloat(_)) || matches!(y, DataType::DecFloat(_)) {
        DataType::DecFloat(decfloat_digits(x, y))
    } else if matches!(x, DataType::Real | DataType::Double) || matches!(y, DataType::Real | DataType::Double) {
        DataType::Double
    } else if let (Some(i), Some(j)) = (integer_rank(x), integer_rank(y)) {
        if i.max(j) == 2 { DataType::BigInt } else { DataType::Integer }
    } else {
        let ((p1, s1), (p2, s2)) = (as_decimal(x)?, as_decimal(y)?);
        let (p, s) = match op {
            BinOp::Add | BinOp::Sub => {
                let s = s1.max(s2);
                ((p1 - s1).max(p2 - s2) + s + 1, s)
            }
            BinOp::Mul => (p1 + p2, s1 + s2),
            BinOp::Div => (MAX_PRECISION, (i32::from(MAX_PRECISION) - i32::from(p1) + i32::from(s1) - i32::from(s2)).max(0).try_into().ok()?),
            BinOp::Concat => return None,
        };
        DataType::Decimal(p.min(MAX_PRECISION), s.min(MAX_PRECISION).min(p.min(MAX_PRECISION)))
    };
    Some(SqlType { spec: plain(t), nullable })
}

fn plain(t: DataType) -> TypeSpec {
    TypeSpec { data_type: t, ccsid: None, bit_data: false, udt: None }
}

/// The type of `a || b`: a string as long as both (a number counted as
/// its text), fixed-length only when both are.
#[must_use]
pub fn concat(a: &SqlType, b: &SqlType) -> Option<SqlType> {
    let (x, y) = (a.data_type(), b.data_type());
    let n = char_length(x)?.saturating_add(char_length(y)?);
    let lob = |t: &DataType| matches!(t, DataType::Clob(_) | DataType::Dbclob(_) | DataType::Blob(_));
    let graphic = |t: &DataType| matches!(t, DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_));
    if matches!(x, DataType::Date | DataType::Time | DataType::Timestamp(_))
        || matches!(y, DataType::Date | DataType::Time | DataType::Timestamp(_))
    {
        return None;
    }
    let t = if is_binary(x) && is_binary(y) {
        if lob(x) || lob(y) {
            DataType::Blob(n)
        } else if matches!((x, y), (DataType::Binary(_), DataType::Binary(_))) {
            DataType::Binary(n)
        } else {
            DataType::Varbinary(n)
        }
    } else if lob(x) || lob(y) {
        if graphic(x) || graphic(y) { DataType::Dbclob(n) } else { DataType::Clob(n) }
    } else if graphic(x) || graphic(y) {
        if matches!((x, y), (DataType::Graphic(_), DataType::Graphic(_))) {
            DataType::Graphic(n)
        } else {
            DataType::Vargraphic(n)
        }
    } else if matches!((x, y), (DataType::Char(_), DataType::Char(_))) {
        DataType::Char(n)
    } else {
        DataType::Varchar(n.min(32740))
    };
    Some(SqlType {
        spec: TypeSpec { data_type: t, ccsid: None, bit_data: a.spec.bit_data && b.spec.bit_data, udt: None },
        nullable: a.nullable || b.nullable,
    })
}

/// The type both of two results may take (`CASE`, `COALESCE`, `UNION`),
/// `None` when they have none.
#[must_use]
pub fn unify(a: &SqlType, b: &SqlType) -> Option<SqlType> {
    // A distinct type goes only with itself.
    if a.spec.udt != b.spec.udt {
        return None;
    }
    if a.spec.udt.is_some() {
        return Some(SqlType { spec: a.spec.clone(), nullable: a.nullable || b.nullable });
    }
    let (x, y) = (a.data_type(), b.data_type());
    let nullable = a.nullable || b.nullable;
    let t = if is_number(x) && is_number(y) {
        if matches!(x, DataType::DecFloat(_)) || matches!(y, DataType::DecFloat(_)) {
            DataType::DecFloat(decfloat_digits(x, y))
        } else if matches!(x, DataType::Real | DataType::Double) || matches!(y, DataType::Real | DataType::Double) {
            if x == y { x.clone() } else { DataType::Double }
        } else if let (Some(i), Some(j)) = (integer_rank(x), integer_rank(y)) {
            if i >= j { x.clone() } else { y.clone() }
        } else {
            let ((p1, s1), (p2, s2)) = (as_decimal(x)?, as_decimal(y)?);
            let s = s1.max(s2);
            let p = ((p1 - s1).max(p2 - s2) + s).min(MAX_PRECISION);
            DataType::Decimal(p, s.min(p))
        }
    } else if is_number(x) && is_text(y) {
        x.clone()
    } else if is_text(x) && is_number(y) {
        y.clone()
    } else if is_text(x) && is_text(y) {
        let n = char_length(x)?.max(char_length(y)?);
        let graphic = |t: &DataType| matches!(t, DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_));
        let lob = |t: &DataType| matches!(t, DataType::Clob(_) | DataType::Dbclob(_));
        match (x, y) {
            (DataType::Char(_), DataType::Char(_)) => DataType::Char(n),
            (DataType::Graphic(_), DataType::Graphic(_)) => DataType::Graphic(n),
            _ if lob(x) || lob(y) => {
                if graphic(x) || graphic(y) { DataType::Dbclob(n) } else { DataType::Clob(n) }
            }
            _ if graphic(x) || graphic(y) => DataType::Vargraphic(n),
            _ => DataType::Varchar(n),
        }
    } else if is_binary(x) && is_binary(y) {
        let n = char_length(x)?.max(char_length(y)?);
        match (x, y) {
            (DataType::Binary(_), DataType::Binary(_)) => DataType::Binary(n),
            (DataType::Blob(_), _) | (_, DataType::Blob(_)) => DataType::Blob(n),
            _ => DataType::Varbinary(n),
        }
    } else if x == y {
        x.clone()
    } else if matches!(x, DataType::Timestamp(_)) && matches!(y, DataType::Timestamp(_)) {
        let p = |t: &DataType| if let DataType::Timestamp(p) = t { *p } else { 0 };
        DataType::Timestamp(p(x).max(p(y)))
    } else if matches!(x, DataType::Date | DataType::Time | DataType::Timestamp(_)) && is_text(y) {
        x.clone()
    } else if is_text(x) && matches!(y, DataType::Date | DataType::Time | DataType::Timestamp(_)) {
        y.clone()
    } else {
        return None;
    };
    Some(SqlType {
        spec: TypeSpec { data_type: t, ccsid: a.spec.ccsid.or(b.spec.ccsid), bit_data: a.spec.bit_data && b.spec.bit_data, udt: None },
        nullable,
    })
}

/// The common type of several results; `None` when one is unknown. In a
/// `CASE`, results with no common type are `SQL0581`.
pub fn all<'a>(es: impl Iterator<Item = &'a Expr>, scope: &Scope<'_>, case: bool) -> Result<Option<SqlType>> {
    let mut acc: Option<SqlType> = None;
    let mut nulls = false;
    for e in es {
        if matches!(e, Expr::Literal(crate::ast::Literal::Null)) {
            nulls = true;
            continue;
        }
        let Some(t) = type_in(e, scope)? else { return Ok(None) };
        acc = match acc {
            None => Some(t),
            Some(a) => match unify(&a, &t) {
                Some(u) => Some(u),
                None if let Some(e) = crate::exec::udt::typing::mixed(&a, &t, if case { crate::exec::udt::typing::Mixed::Case } else { crate::exec::udt::typing::Mixed::Coalesce }) => return Err(e),
                None if case && (is_binary(a.data_type()) != is_binary(t.data_type())) => {
                    return Err(SqlError::Coded {
                        code: -171,
                        state: "42815",
                        what: "argument 1 of CASE mixes binary and character results".into(),
                    });
                }
                None if case => {
                    return Err(SqlError::Coded {
                        code: -581,
                        state: "42804",
                        what: "the results of the CASE expression have no common data type".into(),
                    });
                }
                None => return Ok(None),
            },
        };
    }
    Ok(acc.map(|mut t| {
        t.nullable |= nulls;
        t
    }))
}

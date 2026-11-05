// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Column defaults as the primary checks them: a default that does not
//! fit its column is `SQL0574`, a date that is not one `SQL0180` or
//! `SQL0181`; and the default `WITH DEFAULT` gives a date, time or
//! timestamp column, the current one.

use crate::ast::{ColumnDef, ColumnDefault, DataType, Expr, Literal, Register};
use crate::error::{Result, SqlError};

/// The register `WITH DEFAULT` gives a column of type `t`, if any.
#[must_use]
pub fn current_register(t: &DataType) -> Option<Register> {
    match t {
        DataType::Date => Some(Register::Date),
        DataType::Time => Some(Register::Time),
        DataType::Timestamp(p) => Some(Register::Timestamp(*p)),
        _ => None,
    }
}

fn not_fit(c: &ColumnDef) -> SqlError {
    SqlError::Coded { code: -574, state: "42894", what: format!("the default of column {} does not fit its type", c.name.value) }
}

/// The longest string a character or binary type holds.
fn string_len(t: &DataType) -> Option<u32> {
    match t {
        DataType::Char(n)
        | DataType::Varchar(n)
        | DataType::Graphic(n)
        | DataType::Vargraphic(n)
        | DataType::Clob(n)
        | DataType::Dbclob(n)
        | DataType::Binary(n)
        | DataType::Varbinary(n)
        | DataType::Blob(n) => Some(*n),
        _ => None,
    }
}

fn numeric(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Decimal(..)
            | DataType::Numeric(..)
            | DataType::SmallInt
            | DataType::Integer
            | DataType::BigInt
            | DataType::Real
            | DataType::Double
            | DataType::DecFloat(_)
    )
}

/// Whether the number `text` fits `t` without losing a digit.
fn number_fits(text: &str, t: &DataType) -> bool {
    // A float constant converts; its range is the type's concern at insert.
    if text.contains(['E', 'e']) {
        return true;
    }
    // Trailing zeros of the fraction carry no digit.
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.') } else { text };
    let Ok(d) = qdecimal::Decimal::parse(text) else {
        return false;
    };
    let range = |p: u8, s: u8| d.scale() <= s && d.integer_digits() <= p - s;
    match t {
        DataType::Decimal(p, s) | DataType::Numeric(p, s) => range(*p, *s),
        DataType::SmallInt | DataType::Integer | DataType::BigInt => {
            let Ok(v) = text.parse::<i128>() else { return false };
            super::column::type_range(t).is_some_and(|(lo, hi)| v >= lo && v <= hi)
        }
        _ => true,
    }
}

/// A date default: `SQL0180` when it is not in a date's form, `SQL0181`
/// when it is but names no day.
fn check_date(text: &str) -> Result<()> {
    if crate::value::parse_date(text).is_some() {
        return Ok(());
    }
    let b = text.as_bytes();
    let shaped = b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    if shaped {
        Err(SqlError::Coded { code: -181, state: "22007", what: format!("'{text}' is not a valid date") })
    } else {
        Err(SqlError::Coded { code: -180, state: "22007", what: format!("'{text}' is not in a date's form") })
    }
}

/// Refuse a default that does not fit column `c`.
#[allow(clippy::many_single_char_names)]
pub fn check(c: &ColumnDef) -> Result<()> {
    let Some(ColumnDefault::Value(e)) = &c.default else {
        return Ok(());
    };
    let t = &c.type_spec.data_type;
    let (e, negative) = match e {
        Expr::Neg(inner) => (inner.as_ref(), true),
        e => (e, false),
    };
    let ok = match e {
        Expr::Literal(Literal::Str(v)) => match t {
            DataType::Date => return check_date(v),
            t if numeric(t) => false,
            t => string_len(t).is_none_or(|n| v.trim_end_matches(' ').chars().count() <= n as usize),
        },
        Expr::Literal(Literal::Hex(b) | Literal::Bin(b)) => string_len(t).is_none_or(|n| b.len() <= n as usize),
        Expr::Literal(Literal::Num(v)) => {
            let text = if negative { format!("-{v}") } else { v.clone() };
            numeric(t) && number_fits(&text, t)
        }
        Expr::Register(r) => {
            let chars = |min: u32| matches!(t, DataType::Char(n) | DataType::Varchar(n) if *n >= min);
            match r {
                Register::Date => matches!(t, DataType::Date | DataType::Timestamp(_)) || chars(10),
                Register::Time => matches!(t, DataType::Time) || chars(8),
                Register::Timestamp(_) => matches!(t, DataType::Timestamp(_)) || chars(26),
                Register::User => chars(18),
                _ => true,
            }
        }
        // NULL and the other constants fit any column.
        _ => true,
    };
    if ok { Ok(()) } else { Err(not_fit(c)) }
}

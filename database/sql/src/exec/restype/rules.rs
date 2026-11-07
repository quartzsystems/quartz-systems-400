// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The result types of the built-in scalar functions the casting
//! functions and the families leave (as pub400 describes them), of date,
//! time and timestamp arithmetic, and of the special registers.

use super::shape::{Kind, arith_or, unify_or};
use super::{Scope, SqlType, type_in};
use crate::ast::{BinOp, Expr, Literal};

/// An expression's type for a rule that needs one: the type worked out,
/// [`SqlType::unknown`] when there is none.
#[must_use]
pub fn infer(e: &Expr, scope: &Scope<'_>) -> SqlType {
    type_in(e, scope)
        .ok()
        .flatten()
        .unwrap_or_else(SqlType::unknown)
}

/// Date, time and timestamp arithmetic: the durations of a subtraction,
/// the date, time or timestamp of an addition; `None` for other operands.
#[must_use]
pub fn datetime(a: &SqlType, op: BinOp, b: &SqlType) -> Option<SqlType> {
    let null = a.nullable || b.nullable;
    Some(match (a.kind(), op, b.kind()) {
        (Kind::Date, BinOp::Sub, Kind::Date) => SqlType::decimal(8, 0, null),
        (Kind::Time, BinOp::Sub, Kind::Time) => SqlType::decimal(6, 0, null),
        (Kind::Timestamp, BinOp::Sub, Kind::Timestamp) => {
            let p = a.scale().max(b.scale());
            SqlType::decimal(14 + u32::from(p), p, null)
        }
        (Kind::Date | Kind::Time | Kind::Timestamp, BinOp::Add | BinOp::Sub, _) => {
            a.clone().nullable(null)
        }
        (_, BinOp::Add, Kind::Date | Kind::Time | Kind::Timestamp) => b.clone().nullable(null),
        _ => return None,
    })
}

/// The characters of `TIMESTAMP(p)` as text.
#[must_use]
pub fn ts_chars(p: u8) -> u32 {
    if p == 0 { 19 } else { 20 + u32::from(p) }
}

/// Bytes a value of the type takes, for `HEX`.
#[must_use]
pub fn byte_size(t: &SqlType) -> u32 {
    match t.kind() {
        Kind::SmallInt => 2,
        Kind::Integer | Kind::Real | Kind::Date => 4,
        Kind::BigInt | Kind::Double => 8,
        Kind::Decimal => t.len() / 2 + 1,
        Kind::DecFloat => {
            if t.len() <= 16 {
                8
            } else {
                16
            }
        }
        Kind::Time => 3,
        Kind::Timestamp => 10,
        Kind::Graphic | Kind::Vargraphic | Kind::Dbclob => t.len() * 2,
        _ => t.len(),
    }
}

/// Whether a varying-length string.
fn is_varying(t: &SqlType) -> bool {
    matches!(
        t.kind(),
        Kind::Varchar | Kind::Vargraphic | Kind::Varbinary | Kind::Clob | Kind::Blob | Kind::Dbclob
    )
}

/// The arguments with an unknown one as [`SqlType::unknown`].
#[must_use]
pub fn known(args: &[Option<SqlType>]) -> Vec<SqlType> {
    args.iter()
        .map(|a| a.clone().unwrap_or_else(SqlType::unknown))
        .collect()
}

/// The built-in scalar functions' result types (a [`super::FunctionType`]).
#[allow(clippy::too_many_lines)]
pub fn builtin(name: &str, given: &[Option<SqlType>], exprs: &[Expr]) -> Option<SqlType> {
    let args = known(given);
    let null = args.iter().any(|a| a.nullable);
    let first = args.first().cloned();
    let num = |i: usize| match exprs.get(i) {
        Some(Expr::Literal(Literal::Num(n))) => n.parse::<u32>().ok(),
        _ => None,
    };
    Some(match name {
        "NULLIF" => first?.nullable(true),
        "MAX" | "MIN" | "GREATEST" | "LEAST" => {
            let mut t = first?;
            for a in &args[1..] {
                t = unify_or(t, a);
            }
            t.nullable(null)
        }
        "NVL" => {
            let typed: Vec<SqlType> = given
                .iter()
                .zip(exprs)
                .filter(|(_, e)| !matches!(e, Expr::Literal(Literal::Null)))
                .filter_map(|(t, _)| t.clone())
                .collect();
            let mut t = typed.first()?.clone();
            for a in &typed[1..] {
                t = unify_or(t, a);
            }
            t.nullable(typed.iter().all(|a| a.nullable))
        }
        "DATE" => SqlType::date(null),
        "TIME" => SqlType::time(null),
        "TIMESTAMP" => {
            SqlType::timestamp(num(1).and_then(|p| u8::try_from(p).ok()).unwrap_or(6), null)
        }
        "NOW" => SqlType::timestamp(6, false),
        "CARDINALITY" | "MAX_CARDINALITY" => SqlType::bigint(true),
        "LENGTH" | "CHAR_LENGTH" | "CHARACTER_LENGTH" | "OCTET_LENGTH" | "YEAR" | "MONTH"
        | "DAY" | "DAYOFMONTH" | "HOUR" | "MINUTE" | "SECOND" | "MICROSECOND" | "DAYS"
        | "POSSTR" | "LOCATE" | "POSITION" | "DAYOFWEEK" | "DAYOFWEEK_ISO" | "DAYOFYEAR"
        | "WEEK" | "LEAP" | "JULIAN_DAY" => SqlType::int(null),
        "POWER" | "POW" | "SQRT" => SqlType::double(null),
        "BOOLEAN" => SqlType::boolean(null),
        "DECIMAL" | "DEC" | "ZONED" => {
            let (dp, ds) = first.and_then(|t| t.digits()).unwrap_or((15, 0));
            let p = num(1).unwrap_or(if exprs.len() > 1 { 15 } else { dp });
            let s = num(2).unwrap_or(if exprs.len() > 1 { 0 } else { u32::from(ds) });
            let kind = if name == "ZONED" {
                Kind::Numeric
            } else {
                Kind::Decimal
            };
            SqlType::new(
                kind,
                p.clamp(1, 63),
                u8::try_from(s.min(p)).unwrap_or(0),
                null,
            )
        }
        "BINARY" | "VARBINARY" => {
            let n = num(1).unwrap_or_else(|| first.map_or(1, |t| t.len()));
            SqlType::new(
                if name == "BINARY" {
                    Kind::Binary
                } else {
                    Kind::Varbinary
                },
                n,
                0,
                null,
            )
        }
        "CHAR" | "VARCHAR" => {
            let kind = if name == "CHAR" {
                Kind::Char
            } else {
                Kind::Varchar
            };
            let n = match (num(1), first) {
                (Some(n), _) => n,
                (_, Some(t)) => match t.kind() {
                    Kind::Date => 10,
                    Kind::Time => 8,
                    Kind::Timestamp => ts_chars(t.scale()),
                    _ => super::char_length(t.data_type()).unwrap_or(1),
                },
                _ => 1,
            };
            SqlType::new(kind, n, 0, null)
        }
        "DIGITS" => SqlType::char(
            first
                .and_then(|t| t.digits())
                .map_or(15, |(p, _)| if p == 11 { 10 } else { p }),
            null,
        ),
        "SUBSTR" | "SUBSTRING" | "LEFT" | "RIGHT" | "TRIM" | "LTRIM" | "RTRIM" | "STRIP"
        | "UPPER" | "UCASE" | "LOWER" | "LCASE" => {
            // The source: last after TRIM's end word, else first.
            let moded = matches!(exprs.first(), Some(Expr::Literal(Literal::Str(m))) if matches!(m.as_str(), "BOTH" | "LEADING" | "TRAILING" | "B" | "L" | "T"));
            let t = if moded { args.last().cloned() } else { first };
            let t = t.unwrap_or_else(SqlType::unknown);
            let kind = match t.kind() {
                Kind::Char if matches!(name, "UPPER" | "UCASE" | "LOWER" | "LCASE") => Kind::Char,
                Kind::Binary | Kind::Varbinary => Kind::Varbinary,
                Kind::Clob => Kind::Clob,
                _ => Kind::Varchar,
            };
            SqlType::new(kind, t.len(), 0, null)
        }
        "CONCAT" => arith_or(&first?, BinOp::Concat, args.get(1)?),
        "REPLACE" => SqlType::varchar(
            first
                .map_or(4000, |t| t.len().max(1))
                .saturating_mul(4)
                .min(32740),
            null,
        ),
        // Fixed length for a fixed-length argument, as on the primary.
        "HEX" => match first {
            Some(t) if !is_varying(&t) && byte_size(&t) * 2 <= 32766 => {
                SqlType::char(byte_size(&t) * 2, null)
            }
            t => SqlType::varchar(t.map_or(2, |t| byte_size(&t) * 2).min(32740), null),
        },
        "ABS" | "ABSVAL" | "MOD" | "ROUND" | "TRUNCATE" | "TRUNC" | "CEILING" | "CEIL"
        | "FLOOR" => {
            let t = first.unwrap_or_else(|| SqlType::int(null));
            if name == "MOD" {
                let b = args.get(1).cloned().unwrap_or_else(|| t.clone());
                if t.is_integer() && b.is_integer() {
                    return Some(arith_or(&t, BinOp::Add, &b).nullable(null));
                }
            }
            t.nullable(null)
        }
        "RRN" => SqlType::int(false),
        "GROUPING" => SqlType::smallint(true),
        "INTERPRET" => match exprs.get(1) {
            Some(Expr::Cast(_, t)) => SqlType::of_data_type(&t.data_type, null),
            _ => return None,
        },
        "*DISTINCT" | "*NOTDISTINCT" => SqlType::boolean(false),
        "ADD_MONTHS" | "LAST_DAY" | "NEXT_DAY" => {
            first.unwrap_or_else(|| SqlType::date(null)).nullable(null)
        }
        "DAYNAME" | "MONTHNAME" => SqlType::varchar(100, null),
        // The identity value the job last gave (the primary).
        "IDENTITY_VAL_LOCAL" => SqlType::decimal(31, 0, true),
        _ => return None,
    })
}

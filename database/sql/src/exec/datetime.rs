// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Date, time and timestamp arithmetic as Db2 for i does it: the
//! subtractions give durations (`yyyymmdd`, `hhmmss`,
//! `yyyymmddhhmmss.f`) by IBM's borrowing rules, a labeled duration is
//! truncated to a whole number except `SECONDS` on a timestamp, and a
//! number beside a date, time or timestamp without a unit is refused
//! (`SQL0182`), as a unit on the wrong type is (`SQL0187`).

#![allow(clippy::many_single_char_names)]

use std::cmp::Ordering;

use db::record::datetime::{Date, Time, days_in_month};
use qdecimal::Decimal;

use super::eval::add_duration;
use crate::ast::{BinOp, DurationUnit};
use crate::error::{Result, SqlError};
use crate::value::{PICOS, Timestamp, Value};

/// `DATE1 - DATE2` by IBM's rules, `DATE1` not before `DATE2`: years,
/// months, days. The second date's day may be one past its month (a
/// timestamp's borrowed day).
fn date_parts(y1: i64, m1: i64, d1: i64, y2: i64, mut m2: i64, d2: i64) -> (i64, i64, i64) {
    let mut y2 = y2;
    let days = if d2 <= d1 {
        d1 - d2
    } else {
        let n = i64::from(days_in_month(
            u16::try_from(y2).unwrap_or(1),
            u8::try_from(m2.clamp(1, 12)).unwrap_or(1),
        ));
        m2 += 1;
        n + d1 - d2
    };
    let months = if m2 <= m1 {
        m1 - m2
    } else {
        y2 += 1;
        12 + m1 - m2
    };
    (y1 - y2, months, days)
}

fn dec(n: i128, scale: u8) -> Value {
    Value::Num(Decimal::new(n, scale).expect("fits"))
}

/// `date - date` as `DECIMAL(8,0)`.
#[must_use]
pub fn date_minus(a: Date, b: Date) -> Value {
    let neg = crate::value::cmp_date(&a, &b) == Ordering::Less;
    let (hi, lo) = if neg { (b, a) } else { (a, b) };
    let (y, m, d) = date_parts(
        i64::from(hi.year),
        i64::from(hi.month),
        i64::from(hi.day),
        i64::from(lo.year),
        i64::from(lo.month),
        i64::from(lo.day),
    );
    let n = i128::from(y * 10_000 + m * 100 + d);
    dec(if neg { -n } else { n }, 0)
}

fn secs(t: Time) -> i64 {
    i64::from(t.hour) * 3600 + i64::from(t.minute) * 60 + i64::from(t.second)
}

/// `time - time` as `DECIMAL(6,0)`.
#[must_use]
pub fn time_minus(a: Time, b: Time) -> Value {
    let d = secs(a) - secs(b);
    let x = d.abs();
    let n = i128::from(x / 3600 * 10_000 + x % 3600 / 60 * 100 + x % 60);
    dec(if d < 0 { -n } else { n }, 0)
}

/// `timestamp - timestamp` as `DECIMAL(14+p, p)`, `p` the larger
/// precision.
#[must_use]
pub fn timestamp_minus(a: &Timestamp, b: &Timestamp) -> Value {
    let p = a.precision.max(b.precision);
    let neg = a < b;
    let (hi, lo) = if neg { (b, a) } else { (a, b) };
    let unit = 10u64.pow(12 - u32::from(p));
    let (f1, f2) = (i128::from(hi.picos / unit), i128::from(lo.picos / unit));
    let one = i128::from(PICOS / unit);
    let mut s2 = i64::from(lo.time.second);
    let mut mi2 = i64::from(lo.time.minute);
    let mut h2 = i64::from(lo.time.hour);
    let mut d2 = i64::from(lo.date.day);
    let frac = if f2 <= f1 {
        f1 - f2
    } else {
        s2 += 1;
        one + f1 - f2
    };
    let s1 = i64::from(hi.time.second);
    let sec = if s2 <= s1 {
        s1 - s2
    } else {
        mi2 += 1;
        60 + s1 - s2
    };
    let mi1 = i64::from(hi.time.minute);
    let min = if mi2 <= mi1 {
        mi1 - mi2
    } else {
        h2 += 1;
        60 + mi1 - mi2
    };
    let h1 = i64::from(hi.time.hour);
    let hour = if h2 <= h1 {
        h1 - h2
    } else {
        d2 += 1;
        24 + h1 - h2
    };
    let (y, m, d) = date_parts(
        i64::from(hi.date.year),
        i64::from(hi.date.month),
        i64::from(hi.date.day),
        i64::from(lo.date.year),
        i64::from(lo.date.month),
        d2,
    );
    let whole = i128::from(((((y * 100 + m) * 100 + d) * 100 + hour) * 100 + min) * 100 + sec);
    let n = whole * one + frac;
    dec(if neg { -n } else { n }, p)
}

/// Arithmetic with a date, time or timestamp operand and no labeled
/// duration; `None` when neither operand is one.
pub fn binary(a: &Value, op: BinOp, b: &Value) -> Option<Result<Value>> {
    let dt = |v: &Value| matches!(v, Value::Date(_) | Value::Time(_) | Value::Timestamp(_));
    if !dt(a) && !dt(b) || op == BinOp::Concat {
        return None;
    }
    Some(match (a, op, b) {
        (Value::Date(x), BinOp::Sub, Value::Date(y)) => Ok(date_minus(*x, *y)),
        (Value::Time(x), BinOp::Sub, Value::Time(y)) => Ok(time_minus(*x, *y)),
        (Value::Timestamp(x), BinOp::Sub, Value::Timestamp(y)) => Ok(timestamp_minus(x, y)),
        // A date or time beside a timestamp.
        (Value::Timestamp(x), BinOp::Sub, Value::Date(y)) => {
            Ok(timestamp_minus(x, &Timestamp::midnight(*y)))
        }
        (Value::Date(x), BinOp::Sub, Value::Timestamp(y)) => {
            Ok(timestamp_minus(&Timestamp::midnight(*x), y))
        }
        // A date, time or timestamp in text beside one.
        (Value::Date(x), BinOp::Sub, Value::Str(s)) => crate::value::parse_date(s)
            .map(|y| date_minus(*x, y))
            .ok_or_else(|| bad("a date minus text that is not a date")),
        (Value::Time(x), BinOp::Sub, Value::Str(s)) => crate::value::parse_time(s)
            .map(|y| time_minus(*x, y))
            .ok_or_else(|| bad("a time minus text that is not a time")),
        (Value::Timestamp(x), BinOp::Sub, Value::Str(s)) => Timestamp::parse(s)
            .map(|y| timestamp_minus(x, &y))
            .ok_or_else(|| bad("a timestamp minus text that is not a timestamp")),
        _ => Err(bad(
            "an operand of the arithmetic is a date, time or timestamp and the other is not a duration",
        )),
    })
}

fn bad(what: &str) -> SqlError {
    SqlError::DateTimeExpr { what: what.into() }
}

/// `v + n unit` (or `-`): the duration truncated to a whole number,
/// except seconds on a timestamp, whose fraction is kept.
pub fn labeled(v: &Value, n: &Value, negate: bool, unit: DurationUnit) -> Result<Value> {
    if v.is_null() || n.is_null() {
        return Ok(Value::Null);
    }
    let kind_ok = match (v, unit) {
        (Value::Date(_), DurationUnit::Years | DurationUnit::Months | DurationUnit::Days)
        | (Value::Time(_), DurationUnit::Hours | DurationUnit::Minutes | DurationUnit::Seconds)
        | (Value::Timestamp(_) | Value::Str(_), _) => true,
        (Value::Date(_) | Value::Time(_), _) => false,
        _ => {
            return Err(SqlError::Duration {
                what: format!("a labeled duration added to a {} value", v.type_name()),
            });
        }
    };
    if !kind_ok {
        return Err(SqlError::Duration {
            what: format!("{unit:?} with a {} value", v.type_name()),
        });
    }
    // Text: as the date, time or timestamp it spells.
    let v = match v {
        Value::Str(s) => {
            if let Some(t) = Timestamp::parse(s) {
                Value::Timestamp(t)
            } else if let Some(d) = crate::value::parse_date(s) {
                Value::Date(d)
            } else if let Some(t) = crate::value::parse_time(s) {
                Value::Time(t)
            } else {
                return Err(bad(
                    "text that is not a date, time or timestamp with a labeled duration",
                ));
            }
        }
        v => v.clone(),
    };
    if let (Value::Timestamp(_), DurationUnit::Seconds) = (&v, unit) {
        let d = n.as_decimal()?;
        let d = if negate { d.negated() } else { d };
        let whole = d
            .rescale(0, qdecimal::Rounding::Truncate)
            .map_err(|e| SqlError::Overflow {
                what: e.to_string(),
            })?;
        let frac = d.sub_exact(&whole).map_err(|e| SqlError::Overflow {
            what: e.to_string(),
        })?;
        let picos = frac
            .mul_exact(&Decimal::new(i128::from(PICOS), 0).expect("fits"))
            .map_err(|e| SqlError::Overflow {
                what: e.to_string(),
            })?
            .rescale(0, qdecimal::Rounding::Truncate)
            .map_err(|e| SqlError::Overflow {
                what: e.to_string(),
            })?
            .to_i128()
            .and_then(|x| i64::try_from(x).ok())
            .unwrap_or(0);
        let w = whole
            .to_i128()
            .and_then(|x| i64::try_from(x).ok())
            .ok_or_else(|| SqlError::Overflow {
                what: "the duration is too large".into(),
            })?;
        let t = add_duration(&v, w, DurationUnit::Seconds)?;
        if picos == 0 {
            return Ok(t);
        }
        // The fraction, in microseconds' place and below.
        return add_picos(&t, picos);
    }
    let k = n.as_i64()?;
    add_duration(&v, if negate { -k } else { k }, unit)
}

/// A timestamp moved by `picos` picoseconds.
fn add_picos(v: &Value, picos: i64) -> Result<Value> {
    let Value::Timestamp(t) = v else {
        return Ok(v.clone());
    };
    let total = i128::from(t.picos) + i128::from(picos);
    let unit = i128::from(PICOS);
    let shift = i64::try_from(total.div_euclid(unit)).expect("small");
    let rest = u64::try_from(total.rem_euclid(unit)).expect("below a second");
    let moved = add_duration(
        &Value::Timestamp(Timestamp { picos: 0, ..*t }),
        shift,
        DurationUnit::Seconds,
    )?;
    match moved {
        Value::Timestamp(m) => Ok(Value::Timestamp(Timestamp { picos: rest, ..m })),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_as_on_pub400() {
        let d = |y, m, dd| Date::new(y, m, dd).unwrap();
        assert_eq!(
            date_minus(d(2024, 3, 1), d(2023, 1, 31)).to_string(),
            "10101"
        );
        assert_eq!(
            date_minus(d(2024, 1, 31), d(2024, 3, 1)).to_string(),
            "-101"
        );
        let t = |h, m, s| Time::new(h, m, s).unwrap();
        assert_eq!(time_minus(t(10, 30, 15), t(8, 45, 50)).to_string(), "14425");
        assert_eq!(time_minus(t(8, 0, 0), t(9, 30, 1)).to_string(), "-13001");
        let a = Timestamp::parse("2024-03-01-10.30.15.000005").unwrap();
        let b = Timestamp::parse("2023-01-31-12.00.00.000010").unwrap();
        assert_eq!(timestamp_minus(&a, &b).to_string(), "10100223014.999995");
        let a = Timestamp::parse("2024-01-01-10.00.00.123456789")
            .unwrap()
            .with_precision(9);
        let b = Timestamp::parse("2024-01-01-09.00.00.000000001")
            .unwrap()
            .with_precision(9);
        assert_eq!(timestamp_minus(&a, &b).to_string(), "10000.123456788");
    }
}

/// What an unlabeled duration of a decimal type stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationKind {
    /// `DECIMAL(8,0)`: `yyyymmdd`.
    Date,
    /// `DECIMAL(6,0)`: `hhmmss`.
    Time,
    /// `DECIMAL(14+s,s)`: `yyyymmddhhmmss.f`.
    Timestamp,
}

/// `v + d` or `v - d` with `d` a date, time or timestamp duration: the
/// parts added largest first, or taken away smallest first.
pub fn with_duration(v: &Value, d: &Decimal, sub: bool, kind: DurationKind) -> Result<Value> {
    let neg = d.is_negative() != sub;
    let a = d.abs();
    let whole = a
        .rescale(0, qdecimal::Rounding::Truncate)
        .map_err(|e| SqlError::Overflow {
            what: e.to_string(),
        })?;
    let n = whole
        .to_i128()
        .and_then(|x| i64::try_from(x).ok())
        .ok_or_else(|| SqlError::Overflow {
            what: "the duration is too large".into(),
        })?;
    let frac = a.sub_exact(&whole).map_err(|e| SqlError::Overflow {
        what: e.to_string(),
    })?;
    let s = |x: i64| if neg { -x } else { x };
    let (y, mo, dd, h, mi, se) = match kind {
        DurationKind::Date => (n / 10_000, n / 100 % 100, n % 100, 0, 0, 0),
        DurationKind::Time => (0, 0, 0, n / 10_000, n / 100 % 100, n % 100),
        DurationKind::Timestamp => (
            n / 10_000_000_000,
            n / 100_000_000 % 100,
            n / 1_000_000 % 100,
            n / 10_000 % 100,
            n / 100 % 100,
            n % 100,
        ),
    };
    let mut parts = vec![
        (y, DurationUnit::Years),
        (mo, DurationUnit::Months),
        (dd, DurationUnit::Days),
        (h, DurationUnit::Hours),
        (mi, DurationUnit::Minutes),
        (se, DurationUnit::Seconds),
    ];
    if neg {
        parts.reverse();
    }
    let mut out = v.clone();
    if neg && !frac.is_zero() {
        out = labeled(&out, &Value::Num(frac.clone()), true, DurationUnit::Seconds)?;
    }
    for (x, u) in parts {
        if x != 0 {
            out = add_duration(&out, s(x), u)?;
        }
    }
    if !neg && !frac.is_zero() {
        out = labeled(&out, &Value::Num(frac), false, DurationUnit::Seconds)?;
    }
    Ok(out)
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Date and time scalar functions: `TIMESTAMP_FORMAT`/`TO_DATE`,
//! `VARCHAR_FORMAT`/`TO_CHAR` (format elements in `format`), `EXTRACT`,
//! `TIMESTAMPDIFF(_BIG)` over IBM's duration text, `TIMESTAMP_ISO`,
//! `ROUND_TIMESTAMP`/`TRUNC_TIMESTAMP`, `WEEK_ISO`, `QUARTER`,
//! `MIDNIGHT_SECONDS`, `MONTHS_BETWEEN`, `ADD_DAYS` and its kin, and the
//! 14-character `TIMESTAMP` and 7-character `DATE` strings.

#![allow(clippy::many_single_char_names)]

use db::record::datetime::{Date, Time, days_in_month};
use qdecimal::{Decimal, Rounding};

use super::super::SqlContext;
use super::super::eval::{add_months, date_from_days, days_of};
use super::super::restype::{Kind, SqlType};
use super::{arity, day_of_week, int, overflow};
use crate::ast::DurationUnit;
use crate::error::{Result, SqlError};
use crate::value::{PICOS, Timestamp, Value};

pub(super) use super::format::{parse_timestamp, timestamp_text};

/// Calls `name` when this family has it.
#[allow(clippy::too_many_lines)]
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    // The 14-character timestamp and 7-character date strings.
    match (name, args) {
        ("TIMESTAMP", [Value::Str(s)])
            if s.trim().len() == 14 && s.trim().bytes().all(|b| b.is_ascii_digit()) =>
        {
            return Some(stamp14(s.trim()));
        }
        ("DATE", [Value::Str(s)])
            if s.trim().len() == 7 && s.trim().bytes().all(|b| b.is_ascii_digit()) =>
        {
            return Some(julian7(s.trim()));
        }
        _ => {}
    }
    if !matches!(
        name,
        "TIMESTAMP_FORMAT"
            | "TO_DATE"
            | "TO_TIMESTAMP"
            | "VARCHAR_FORMAT"
            | "TO_CHAR"
            | "EXTRACT"
            | "TIMESTAMPDIFF"
            | "TIMESTAMPDIFF_BIG"
            | "TIMESTAMP_ISO"
            | "ROUND_TIMESTAMP"
            | "TRUNC_TIMESTAMP"
            | "WEEK_ISO"
            | "QUARTER"
            | "MIDNIGHT_SECONDS"
            | "MONTHS_BETWEEN"
            | "ADD_DAYS"
            | "ADD_HOURS"
            | "ADD_MINUTES"
            | "ADD_SECONDS"
            | "ADD_YEARS"
    ) {
        return None;
    }
    if args.iter().any(Value::is_null) {
        return Some(Ok(Value::Null));
    }
    Some(match name {
        "TIMESTAMP_FORMAT" | "TO_DATE" | "TO_TIMESTAMP" => (|| {
            arity(name, args, 2, 3)?;
            let p = match args.get(2) {
                Some(v) => precision(v)?,
                None => 6,
            };
            Ok(Value::Timestamp(
                parse_timestamp(&args[0].as_text(), &args[1].as_text(), ctx)?.with_precision(p),
            ))
        })(),
        "VARCHAR_FORMAT" | "TO_CHAR" => (|| {
            arity(name, args, 1, 3)?;
            match &args[0] {
                Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_) => {
                    let f = args.get(1).map(Value::as_text);
                    Ok(Value::Str(super::format::number_text(
                        &args[0],
                        f.as_deref(),
                    )?))
                }
                v => {
                    let ts = v.as_timestamp()?;
                    let f = args
                        .get(1)
                        .map_or_else(|| "YYYY-MM-DD HH24:MI:SS".to_string(), Value::as_text);
                    Ok(Value::Str(timestamp_text(&ts, &f)?))
                }
            }
        })(),
        "EXTRACT" => arity(name, args, 2, 2).and_then(|()| extract(&args[0].as_text(), &args[1])),
        "TIMESTAMPDIFF" | "TIMESTAMPDIFF_BIG" => arity(name, args, 2, 2).and_then(|()| {
            let n = timestampdiff(args[0].as_i64()?, &args[1])?;
            if name == "TIMESTAMPDIFF" && i32::try_from(n).is_err() {
                return Err(SqlError::Overflow {
                    what: format!("TIMESTAMPDIFF result {n} is past an INTEGER"),
                });
            }
            Ok(int(n))
        }),
        "TIMESTAMP_ISO" => arity(name, args, 1, 1).and_then(|()| {
            Ok(Value::Timestamp(match &args[0] {
                Value::Time(t) => Timestamp::new(Date::from_unix(ctx.clock()), *t, 0),
                v => v.as_timestamp()?,
            }))
        }),
        "ROUND_TIMESTAMP" | "TRUNC_TIMESTAMP" => arity(name, args, 1, 2).and_then(|()| {
            let fmt = args.get(1).map_or_else(
                || "DD".to_string(),
                |f| f.as_text().trim().to_ascii_uppercase(),
            );
            let (ts, p) = match &args[0] {
                Value::Date(d) => (Timestamp::midnight(*d), 0),
                v => {
                    let t = v.as_timestamp()?;
                    (t, t.precision)
                }
            };
            Ok(Value::Timestamp(
                round_trunc(&ts, &fmt, name == "ROUND_TIMESTAMP")?.with_precision(p),
            ))
        }),
        "WEEK_ISO" => {
            arity(name, args, 1, 1).and_then(|()| Ok(int(iso_week(args[0].as_date()?).1)))
        }
        "QUARTER" => arity(name, args, 1, 1)
            .and_then(|()| Ok(int(i64::from(args[0].as_date()?.month - 1) / 3 + 1))),
        "MIDNIGHT_SECONDS" => arity(name, args, 1, 1).and_then(|()| {
            let t = args[0].as_time()?;
            Ok(int(i64::from(t.hour) * 3600
                + i64::from(t.minute) * 60
                + i64::from(t.second)))
        }),
        "MONTHS_BETWEEN" => {
            arity(name, args, 2, 2).and_then(|()| months_between(&args[0], &args[1]))
        }
        _ => arity(name, args, 2, 2).and_then(|()| {
            let unit = match name {
                "ADD_DAYS" => DurationUnit::Days,
                "ADD_HOURS" => DurationUnit::Hours,
                "ADD_MINUTES" => DurationUnit::Minutes,
                "ADD_SECONDS" => DurationUnit::Seconds,
                _ => DurationUnit::Years,
            };
            // A date beside hours, minutes or seconds is a timestamp.
            let v = match (&args[0], unit) {
                (
                    Value::Date(d),
                    DurationUnit::Hours | DurationUnit::Minutes | DurationUnit::Seconds,
                ) => Value::Timestamp(Timestamp::midnight(*d)),
                (v, _) => v.clone(),
            };
            super::super::datetime::labeled(&v, &args[1], false, unit)
        }),
    })
}

fn precision(v: &Value) -> Result<u8> {
    let p = v.as_i64()?;
    u8::try_from(p)
        .ok()
        .filter(|p| *p <= 12)
        .ok_or_else(|| SqlError::Overflow {
            what: format!("timestamp precision {p} is not 0 to 12"),
        })
}

fn bad_date(what: &str) -> SqlError {
    SqlError::Conversion {
        what: format!("'{what}' is not a date, time or timestamp"),
    }
}

fn stamp14(s: &str) -> Result<Value> {
    let n = |r: std::ops::Range<usize>| s[r].parse::<u16>().map_err(|_| bad_date(s));
    let date = Date::new(
        n(0..4)?,
        u8::try_from(n(4..6)?).unwrap_or(0),
        u8::try_from(n(6..8)?).unwrap_or(0),
    )
    .ok_or_else(|| bad_date(s))?;
    let time = Time::new(
        u8::try_from(n(8..10)?).unwrap_or(99),
        u8::try_from(n(10..12)?).unwrap_or(99),
        u8::try_from(n(12..14)?).unwrap_or(99),
    )
    .ok_or_else(|| bad_date(s))?;
    Ok(Value::Timestamp(Timestamp::new(date, time, 0)))
}

fn julian7(s: &str) -> Result<Value> {
    let year: u16 = s[..4].parse().map_err(|_| bad_date(s))?;
    let day: i64 = s[4..].parse().map_err(|_| bad_date(s))?;
    let last = if db::record::datetime::is_leap(year) {
        366
    } else {
        365
    };
    if !(1..=last).contains(&day) {
        return Err(bad_date(s));
    }
    let jan1 = Date::new(year, 1, 1).ok_or_else(|| bad_date(s))?;
    Ok(Value::Date(
        date_from_days(days_of(jan1) + day - 1).ok_or_else(|| bad_date(s))?,
    ))
}

/// The ISO year and week of a date.
pub(crate) fn iso_week(d: Date) -> (i64, i64) {
    // ISO weekday: Monday 1 to Sunday 7.
    let wd = |d: Date| (day_of_week(d) + 5) % 7 + 1;
    let thursday = days_of(d) - wd(d) + 4;
    let td = date_from_days(thursday).unwrap_or(d);
    let jan1 = Date::new(td.year, 1, 1).expect("valid");
    (i64::from(td.year), (thursday - days_of(jan1)) / 7 + 1)
}

fn extract(part: &str, v: &Value) -> Result<Value> {
    let part = part.trim().to_ascii_uppercase();
    let date = || v.as_date();
    let ts = || -> Result<Timestamp> {
        match v {
            Value::Time(t) => Ok(Timestamp::new(Date::new(1, 1, 1).expect("valid"), *t, 0)),
            v => v.as_timestamp(),
        }
    };
    Ok(match part.as_str() {
        "YEAR" | "YEARS" => int(i64::from(date()?.year)),
        "MONTH" | "MONTHS" => int(i64::from(date()?.month)),
        "DAY" | "DAYS" => int(i64::from(date()?.day)),
        "HOUR" | "HOURS" => int(i64::from(ts()?.time.hour)),
        "MINUTE" | "MINUTES" => int(i64::from(ts()?.time.minute)),
        "SECOND" | "SECONDS" => {
            let t = ts()?;
            let p = if matches!(v, Value::Timestamp(_)) {
                t.precision.max(6)
            } else {
                6
            };
            let unit = 10u64.pow(12 - u32::from(p));
            let n =
                i128::from(t.time.second) * i128::from(PICOS / unit) + i128::from(t.picos / unit);
            Value::Num(Decimal::new(n, p).expect("fits"))
        }
        "MILLISECOND" | "MILLISECONDS" => {
            let t = ts()?;
            int(i64::from(t.time.second) * 1000
                + i64::try_from(t.picos / 1_000_000_000).expect("small"))
        }
        "MICROSECOND" | "MICROSECONDS" => {
            let t = ts()?;
            int(i64::from(t.time.second) * 1_000_000 + i64::from(t.micros()))
        }
        "EPOCH" => {
            let t = ts()?;
            let days = days_of(t.date) - days_of(Date::new(1970, 1, 1).expect("valid"));
            int(days * 86_400
                + i64::from(t.time.hour) * 3600
                + i64::from(t.time.minute) * 60
                + i64::from(t.time.second))
        }
        "DOW" => int(day_of_week(date()?)),
        "DOY" => int(i64::from(date()?.day_of_year())),
        "WEEK" => int(iso_week(date()?).1),
        "QUARTER" => int(i64::from(date()?.month - 1) / 3 + 1),
        "DECADE" => int(i64::from(date()?.year) / 10),
        "CENTURY" | "CENTURIES" => int((i64::from(date()?.year) + 99) / 100),
        "MILLENNIUM" | "MILLENNIUMS" => int((i64::from(date()?.year) + 999) / 1000),
        other => {
            return Err(SqlError::Unsupported {
                what: format!("EXTRACT of {other}"),
            });
        }
    })
}

/// IBM's `TIMESTAMPDIFF`: the duration text `yyyymmddhhmmss.ffffff` (or
/// a number of that form) in the interval asked for, a month taken as
/// 30 days and a year as 365.
fn timestampdiff(code: i64, dur: &Value) -> Result<i64> {
    let d = match dur {
        Value::Str(s) => Decimal::parse(s.trim()).map_err(|_| SqlError::Conversion {
            what: format!("'{s}' is not a timestamp duration"),
        })?,
        v => v.as_decimal()?,
    };
    let neg = d.is_negative();
    let d = d.abs();
    let whole = d
        .rescale(0, Rounding::Truncate)
        .map_err(overflow)?
        .to_i128()
        .unwrap_or(0);
    let micros = d
        .sub_exact(&Decimal::new(whole, 0).expect("fits"))
        .map_err(overflow)?;
    let micros = micros
        .mul_exact(&Decimal::new(1_000_000, 0).expect("fits"))
        .map_err(overflow)?
        .rescale(0, Rounding::Truncate)
        .map_err(overflow)?
        .to_i128()
        .unwrap_or(0);
    let sec = whole % 100;
    let min = whole / 100 % 100;
    let hour = whole / 10_000 % 100;
    let day = whole / 1_000_000 % 100;
    let month = whole / 100_000_000 % 100;
    let year = whole / 10_000_000_000;
    let days = year * 365 + month * 30 + day;
    let total = (((days * 24 + hour) * 60 + min) * 60 + sec) * 1_000_000 + micros;
    let per: i128 = match code {
        1 => 1,
        2 => 1_000_000,
        4 => 60_000_000,
        8 => 3_600_000_000,
        16 => 86_400_000_000,
        32 => 7 * 86_400_000_000,
        64 => 30 * 86_400_000_000,
        128 => 90 * 86_400_000_000,
        256 => 365 * 86_400_000_000,
        _ => {
            return Err(SqlError::Overflow {
                what: format!("TIMESTAMPDIFF interval {code} is not 1, 2, 4, ..., 256"),
            });
        }
    };
    let n = i64::try_from(total / per).map_err(|_| SqlError::Overflow {
        what: "TIMESTAMPDIFF result too large".into(),
    })?;
    Ok(if neg { -n } else { n })
}

/// `MONTHS_BETWEEN(a, b)`: whole months when the days are the same or
/// both the last of their months, else the days over 31 added.
fn months_between(a: &Value, b: &Value) -> Result<Value> {
    let (x, y) = (a.as_timestamp()?, b.as_timestamp()?);
    let months = (i64::from(x.date.year) - i64::from(y.date.year)) * 12 + i64::from(x.date.month)
        - i64::from(y.date.month);
    let last = |d: Date| d.day == days_in_month(d.year, d.month);
    if x.date.day == y.date.day || (last(x.date) && last(y.date)) {
        return Ok(Value::Num(
            Decimal::new(i128::from(months), 0)
                .expect("fits")
                .rescale(15, Rounding::Truncate)
                .map_err(overflow)?,
        ));
    }
    // The difference in days and time over 31-day months.
    let secs = |t: &Timestamp| {
        i128::from(t.date.day) * 86_400
            + i128::from(t.time.hour) * 3600
            + i128::from(t.time.minute) * 60
            + i128::from(t.time.second)
    };
    let diff = secs(&x) - secs(&y);
    let m = Decimal::new(i128::from(months), 0).expect("fits");
    let frac = Decimal::new(diff, 0)
        .expect("fits")
        .div_to_scale(&Decimal::new(31 * 86_400, 0).expect("fits"), 15)
        .map_err(overflow)?;
    Ok(Value::Num(m.add_exact(&frac).map_err(overflow)?))
}

/// `ROUND_TIMESTAMP` and `TRUNC_TIMESTAMP` to the unit a format names.
#[allow(clippy::too_many_lines)]
fn round_trunc(t: &Timestamp, fmt: &str, round: bool) -> Result<Timestamp> {
    let d = t.date;
    let midnight = |d: Date| Timestamp::midnight(d);
    let day_secs =
        i64::from(t.time.hour) * 3600 + i64::from(t.time.minute) * 60 + i64::from(t.time.second);
    let shift = |d: Date, n: i64| {
        date_from_days(days_of(d) + n).ok_or_else(|| overflow("the date is out of range"))
    };
    let out = match fmt {
        "CC" | "SCC" => {
            let start = (i64::from(d.year) - 1) / 100 * 100 + 1;
            let y = if round && i64::from(d.year) - start >= 50 {
                start + 100
            } else {
                start
            };
            midnight(
                Date::new(u16::try_from(y).map_err(overflow)?, 1, 1)
                    .ok_or_else(|| overflow("year"))?,
            )
        }
        "SYYYY" | "YYYY" | "YEAR" | "SYEAR" | "YYY" | "YY" | "Y" => {
            let y = if round && d.month >= 7 {
                d.year + 1
            } else {
                d.year
            };
            midnight(Date::new(y, 1, 1).ok_or_else(|| overflow("year"))?)
        }
        "IYYY" | "IYY" | "IY" | "I" => {
            let (iy, _) = iso_week(d);
            let start = |y: i64| -> Result<Date> {
                let jan4 = Date::new(u16::try_from(y).map_err(overflow)?, 1, 4)
                    .ok_or_else(|| overflow("year"))?;
                shift(jan4, -((day_of_week(jan4) + 5) % 7))
            };
            let s = start(iy)?;
            let next = start(iy + 1)?;
            let mid = (days_of(next) - days_of(s)) / 2;
            midnight(if round && days_of(d) - days_of(s) >= mid {
                next
            } else {
                s
            })
        }
        "Q" => {
            let first = (d.month - 1) / 3 * 3 + 1;
            let up = round && (d.month > first + 1 || (d.month == first + 1 && d.day >= 16));
            let m = add_months(
                Date::new(d.year, first, 1).expect("valid"),
                if up { 3 } else { 0 },
            )
            .ok_or_else(|| overflow("date"))?;
            midnight(m)
        }
        "MONTH" | "MON" | "MM" | "RM" => {
            let up = round && d.day >= 16;
            midnight(
                add_months(Date::new(d.year, d.month, 1).expect("valid"), i64::from(up))
                    .ok_or_else(|| overflow("date"))?,
            )
        }
        "WW" | "IW" | "W" | "DAY" | "DY" | "D" => {
            // The week's first day: Jan 1's weekday, Monday, the 1st's
            // weekday, or Sunday.
            let anchor = match fmt {
                "WW" => day_of_week(Date::new(d.year, 1, 1).expect("valid")),
                "IW" => 2,
                "W" => day_of_week(Date::new(d.year, d.month, 1).expect("valid")),
                _ => 1,
            };
            let back = (day_of_week(d) - anchor).rem_euclid(7);
            let start = shift(d, -back)?;
            let elapsed = back * 86_400 + day_secs;
            midnight(if round && elapsed >= 7 * 43_200 {
                shift(start, 7)?
            } else {
                start
            })
        }
        "DDD" | "DD" | "J" => {
            let up = round && day_secs >= 43_200;
            midnight(shift(d, i64::from(up))?)
        }
        "HH" | "HH12" | "HH24" => {
            let base = i64::from(t.time.hour) * 3600;
            let up = round && i64::from(t.time.minute) >= 30;
            let s = base + if up { 3600 } else { 0 };
            from_secs(d, s)?
        }
        "MI" => {
            let base = i64::from(t.time.hour) * 3600 + i64::from(t.time.minute) * 60;
            let up = round && t.time.second >= 30;
            from_secs(d, base + if up { 60 } else { 0 })?
        }
        other => {
            return Err(SqlError::Conversion {
                what: format!(
                    "'{other}' is not a format of {}",
                    if round {
                        "ROUND_TIMESTAMP"
                    } else {
                        "TRUNC_TIMESTAMP"
                    }
                ),
            });
        }
    };
    Ok(Timestamp {
        precision: t.precision,
        ..out
    })
}

fn from_secs(d: Date, s: i64) -> Result<Timestamp> {
    let d = date_from_days(days_of(d) + s.div_euclid(86_400)).ok_or_else(|| overflow("date"))?;
    let s = s.rem_euclid(86_400);
    let time = Time::new(
        u8::try_from(s / 3600).expect("hour"),
        u8::try_from(s % 3600 / 60).expect("minute"),
        u8::try_from(s % 60).expect("second"),
    )
    .expect("valid");
    Ok(Timestamp::new(d, time, 0))
}

/// The result type of `name` when this family has it.
pub(crate) fn result_type(
    name: &str,
    given: &[Option<SqlType>],
    exprs: &[crate::ast::Expr],
) -> Option<SqlType> {
    let args = &super::super::restype::rules::known(given)[..];
    let null = args.iter().any(|a| a.nullable);
    let first = args.first().cloned().unwrap_or_else(SqlType::unknown);
    let lit = |i: usize| match exprs.get(i) {
        Some(crate::ast::Expr::Literal(crate::ast::Literal::Num(n))) => n.parse::<u8>().ok(),
        _ => None,
    };
    Some(match name {
        "TIMESTAMP_FORMAT" | "TO_DATE" | "TO_TIMESTAMP" => {
            SqlType::timestamp(lit(2).unwrap_or(6), null)
        }
        "VARCHAR_FORMAT" | "TO_CHAR" => {
            SqlType::varchar(if first.is_numeric() { 254 } else { 255 }, null)
        }
        "EXTRACT" => {
            let part = match exprs.first() {
                Some(crate::ast::Expr::Column(c)) => c.column.value.clone(),
                _ => String::new(),
            };
            match part.as_str() {
                "SECOND" | "SECONDS" => {
                    let p = if first.kind() == Kind::Timestamp {
                        first.scale().max(6)
                    } else {
                        6
                    };
                    SqlType::decimal(2 + u32::from(p), p, null)
                }
                "EPOCH" => SqlType::bigint(null),
                _ => SqlType::int(null),
            }
        }
        "TIMESTAMPDIFF" | "WEEK_ISO" | "QUARTER" | "MIDNIGHT_SECONDS" => SqlType::int(null),
        "TIMESTAMPDIFF_BIG" => SqlType::bigint(null),
        "TIMESTAMP_ISO" => SqlType::timestamp(
            if first.kind() == Kind::Timestamp {
                first.scale()
            } else {
                6
            },
            null,
        ),
        "ROUND_TIMESTAMP" | "TRUNC_TIMESTAMP" => SqlType::timestamp(
            if first.kind() == Kind::Date {
                0
            } else if first.kind() == Kind::Timestamp {
                first.scale()
            } else {
                6
            },
            null,
        ),
        "MONTHS_BETWEEN" => SqlType::decimal(31, 15, null),
        "ADD_DAYS" | "ADD_YEARS" => first.nullable(null),
        "ADD_HOURS" | "ADD_MINUTES" | "ADD_SECONDS" => {
            if first.kind() == Kind::Timestamp {
                first.nullable(null)
            } else {
                SqlType::timestamp(6, null)
            }
        }
        _ => return None,
    })
}

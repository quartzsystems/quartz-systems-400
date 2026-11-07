// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Scalar functions. The first set is here; later sets are families in
//! their own modules, each listed in [`FAMILIES`] and asked first.

pub(crate) mod datetime;
mod ddmath;
mod decfloat;
mod format;
pub(crate) mod math;
pub(crate) mod misc;
mod regexp;
pub(crate) mod services;
pub(crate) mod string;

use std::fmt::Write as _;

use db::record::datetime::{Date, Time, days_in_month, is_leap};
use qdecimal::{Decimal, Rounding, Shape};

mod integrity;

use super::SqlContext;
use super::eval::{add_months, date_from_days, days_of};
use crate::error::{Result, SqlError};
use crate::value::{Timestamp, Value};

pub(super) fn arity(name: &str, args: &[Value], min: usize, max: usize) -> Result<()> {
    if args.len() < min || args.len() > max {
        return Err(SqlError::Unsupported {
            what: format!("{name} with {} arguments", args.len()),
        });
    }
    Ok(())
}

pub(super) fn overflow(e: impl std::fmt::Display) -> SqlError {
    SqlError::Overflow {
        what: e.to_string(),
    }
}

pub(super) fn num(d: Decimal) -> Value {
    Value::Num(d)
}

pub(super) fn int(n: i64) -> Value {
    Value::int(n)
}

pub(super) fn text(s: impl Into<String>) -> Value {
    Value::Str(s.into())
}

/// A decimal's integer digits as text, zero filled (`DIGITS`).
pub(super) fn digits(d: &Decimal) -> String {
    let mag: Vec<u8> = d.digits_le().iter().rev().copied().collect();
    let mut s: String = mag.iter().map(|x| char::from(b'0' + x)).collect();
    if s.is_empty() {
        s.push('0');
    }
    s
}

pub(super) fn f64_of(v: &Value) -> Result<f64> {
    v.as_decimal()?
        .to_string()
        .parse::<f64>()
        .map_err(|e| SqlError::Conversion {
            what: e.to_string(),
        })
}

pub(super) fn from_f64(x: f64) -> Result<Value> {
    if !x.is_finite() {
        return Err(SqlError::Overflow {
            what: "the result is not finite".into(),
        });
    }
    let s = format!("{x:.15}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    Decimal::parse(if s.is_empty() || s == "-" { "0" } else { s })
        .map(Value::Num)
        .map_err(overflow)
}

const DAYNAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHNAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Day of the week, 1 Sunday to 7 Saturday.
pub(super) fn day_of_week(d: Date) -> i64 {
    // 0001-01-01 was a Monday.
    (days_of(d) % 7 + 7) % 7 + 1
}

/// A family of scalar functions in its own module.
pub(crate) struct Family {
    /// Calls `name` when the family has it, `None` when it does not. The
    /// family handles null arguments itself.
    pub call: fn(&str, &[Value], &SqlContext) -> Option<Result<Value>>,
}

/// The families, asked in order before the first set.
const FAMILIES: &[Family] = &[
    Family {
        call: super::array::call,
    },
    Family {
        call: decfloat::call,
    },
    Family {
        call: super::json::plain,
    },
    Family {
        call: integrity::call,
    },
    Family { call: string::call },
    Family {
        call: datetime::call,
    },
    Family { call: math::call },
    Family { call: misc::call },
    Family {
        call: services::call,
    },
];

/// Call a scalar function; `args` are already evaluated.
#[allow(clippy::too_many_lines)]
pub fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    for f in FAMILIES {
        if let Some(r) = (f.call)(name, args, ctx) {
            return r;
        }
    }
    // Null in gives null out, except where a function is about nulls.
    if !matches!(
        name,
        "COALESCE" | "VALUE" | "IFNULL" | "NULLIF" | "NOW" | "RRN"
    ) && args.iter().any(Value::is_null)
    {
        return Ok(Value::Null);
    }
    let a = |i: usize| -> &Value { &args[i] };
    Ok(match name {
        "COALESCE" | "VALUE" | "IFNULL" => {
            arity(name, args, 1, 64)?;
            args.iter()
                .find(|v| !v.is_null())
                .cloned()
                .unwrap_or(Value::Null)
        }
        "NULLIF" => {
            arity(name, args, 2, 2)?;
            if a(0).compare(a(1))? == Some(std::cmp::Ordering::Equal) {
                Value::Null
            } else {
                a(0).clone()
            }
        }
        "CHAR" | "VARCHAR" => {
            arity(name, args, 1, 2)?;
            let s = match a(0) {
                Value::Date(d) => match args.get(1).map(Value::as_text).as_deref() {
                    Some("USA") => d.render(dds::DateFmt::Usa, None),
                    Some("EUR") => d.render(dds::DateFmt::Eur, None),
                    Some("JIS") => d.render(dds::DateFmt::Jis, None),
                    _ => d.render(dds::DateFmt::Iso, None),
                },
                Value::Time(t) => match args.get(1).map(Value::as_text).as_deref() {
                    Some("USA") => t.render(dds::TimeFmt::Usa, None),
                    Some("JIS") => t.render(dds::TimeFmt::Jis, None),
                    _ => t.render(dds::TimeFmt::Iso, None),
                },
                v => {
                    let s = v.as_text();
                    match args.get(1) {
                        Some(n) if matches!(v, Value::Str(_)) => {
                            let n = usize::try_from(n.as_i64()?).unwrap_or(0);
                            let mut out: String = s.chars().take(n).collect();
                            if name == "CHAR" {
                                while out.chars().count() < n {
                                    out.push(' ');
                                }
                            }
                            out
                        }
                        _ => s,
                    }
                }
            };
            text(s)
        }
        "DIGITS" => {
            arity(name, args, 1, 1)?;
            text(digits(&a(0).as_decimal()?))
        }
        "SUBSTR" | "SUBSTRING" => {
            arity(name, args, 2, 4)?;
            if let Value::Bin(b) = a(0) {
                let start = usize::try_from(a(1).as_i64()?.max(1) - 1).expect("positive");
                if start > b.len() {
                    return Err(SqlError::Overflow {
                        what: "SUBSTR start past the end".into(),
                    });
                }
                let len = match args.get(2) {
                    Some(l) => usize::try_from(l.as_i64()?).unwrap_or(0),
                    None => b.len() - start,
                };
                return Ok(Value::Bin(b[start..(start + len).min(b.len())].to_vec()));
            }
            let s: Vec<char> = a(0).as_text().chars().collect();
            let start = a(1).as_i64()?;
            if start < 1 {
                return Err(SqlError::Overflow {
                    what: "SUBSTR start below 1".into(),
                });
            }
            let start = usize::try_from(start - 1).expect("positive");
            let len = match args.get(2) {
                Some(Value::Str(_)) | None => s.len().saturating_sub(start),
                Some(l) => usize::try_from(l.as_i64()?).unwrap_or(0),
            };
            if start > s.len() {
                return Err(SqlError::Overflow {
                    what: "SUBSTR start past the end".into(),
                });
            }
            let end = (start + len).min(s.len());
            text(s[start..end].iter().collect::<String>())
        }
        "LEFT" => {
            arity(name, args, 2, 2)?;
            let n = usize::try_from(a(1).as_i64()?).unwrap_or(0);
            text(a(0).as_text().chars().take(n).collect::<String>())
        }
        "RIGHT" => {
            arity(name, args, 2, 2)?;
            let n = usize::try_from(a(1).as_i64()?).unwrap_or(0);
            let s: Vec<char> = a(0).as_text().chars().collect();
            text(s[s.len().saturating_sub(n)..].iter().collect::<String>())
        }
        "LENGTH" | "LEN" | "CHARACTER_LENGTH" | "CHAR_LENGTH" => {
            arity(name, args, 1, 1)?;
            int(match a(0) {
                Value::Str(s) => i64::try_from(s.chars().count()).expect("fits"),
                Value::Bin(b) => i64::try_from(b.len()).expect("fits"),
                Value::Num(d) => i64::from(d.digits_needed().max(1)),
                Value::DecFloat(d) => i64::try_from(d.format().byte_len()).expect("small"),
                Value::Double(_) => 8,
                Value::Bool(_) => 1,
                Value::Real(_) | Value::Date(_) => 4,
                Value::Time(_) => 3,
                Value::Timestamp(_) => 10,
                Value::Array(_) => return Err(crate::exec::array::misplaced()),
                Value::Null => 0,
            })
        }
        "TRIM" | "LTRIM" | "RTRIM" | "STRIP" => {
            arity(name, args, 1, 3)?;
            // TRIM(BOTH 'x' FROM s) arrives as ('BOTH', 'x', s).
            let (mode, pad, s) = match args {
                [s] => (name, ' ', s.as_text()),
                [Value::Str(m), c, s]
                    if matches!(
                        m.as_str(),
                        "BOTH" | "LEADING" | "TRAILING" | "B" | "L" | "T"
                    ) =>
                {
                    (
                        match m.as_str() {
                            "LEADING" | "L" => "LTRIM",
                            "TRAILING" | "T" => "RTRIM",
                            _ => "TRIM",
                        },
                        c.as_text().chars().next().unwrap_or(' '),
                        s.as_text(),
                    )
                }
                [s, c] => (name, c.as_text().chars().next().unwrap_or(' '), s.as_text()),
                _ => {
                    return Err(SqlError::Unsupported {
                        what: format!("{name} arguments"),
                    });
                }
            };
            text(match mode {
                "LTRIM" => s.trim_start_matches(pad).to_string(),
                "RTRIM" => s.trim_end_matches(pad).to_string(),
                _ => s.trim_matches(pad).to_string(),
            })
        }
        "UPPER" | "UCASE" => {
            arity(name, args, 1, 1)?;
            text(a(0).as_text().to_uppercase())
        }
        "LOWER" | "LCASE" => {
            arity(name, args, 1, 1)?;
            text(a(0).as_text().to_lowercase())
        }
        "CONCAT" => {
            arity(name, args, 2, 2)?;
            text(format!("{}{}", a(0).as_text(), a(1).as_text()))
        }
        "LOCATE" | "POSITION" | "POSSTR" => {
            arity(name, args, 2, 3)?;
            // LOCATE(needle, hay [, start]); POSITION(needle IN hay); POSSTR(hay, needle).
            let (needle, hay) = if name == "POSSTR" {
                (a(1), a(0))
            } else {
                (a(0), a(1))
            };
            let hay: Vec<char> = hay.as_text().chars().collect();
            let needle: Vec<char> = needle.as_text().chars().collect();
            let start = match args.get(2) {
                Some(s) => usize::try_from(s.as_i64()?.max(1) - 1).expect("fits"),
                None => 0,
            };
            let found = (start..=hay.len().saturating_sub(needle.len()))
                .find(|&i| hay[i..].starts_with(&needle))
                .filter(|_| needle.len() <= hay.len());
            int(found.map_or(0, |i| i64::try_from(i + 1).expect("fits")))
        }
        "REPLACE" => {
            arity(name, args, 2, 3)?;
            let to = args.get(2).map_or(String::new(), Value::as_text);
            text(a(0).as_text().replace(&a(1).as_text(), &to))
        }
        "HEX" => {
            arity(name, args, 1, 1)?;
            let bytes = match a(0) {
                Value::Str(s) => ccsid::to_ebcdic(s).unwrap_or_default(),
                Value::Bin(b) => b.clone(),
                Value::DecFloat(d) => d.encode(),
                v => v.as_text().into_bytes(),
            };
            let mut s = String::with_capacity(bytes.len() * 2);
            for b in bytes {
                let _ = write!(s, "{b:02X}");
            }
            text(s)
        }
        "ABS" | "ABSVAL" => {
            arity(name, args, 1, 1)?;
            num(a(0).as_decimal()?.abs())
        }
        "MOD" => {
            arity(name, args, 2, 2)?;
            let x = a(0).as_decimal()?;
            let y = a(1).as_decimal()?;
            if y.is_zero() {
                return Err(SqlError::DivideByZero);
            }
            let q = x.div_to_scale(&y, 0).map_err(overflow)?;
            num(x
                .sub_exact(&q.mul_exact(&y).map_err(overflow)?)
                .map_err(overflow)?)
        }
        "ROUND" | "TRUNCATE" | "TRUNC" => {
            arity(name, args, 1, 2)?;
            let x = a(0).as_decimal()?;
            let places = match args.get(1) {
                Some(p) => p.as_i64()?,
                None => 0,
            };
            let rounding = if name == "ROUND" {
                Rounding::HalfAdjust
            } else {
                Rounding::Truncate
            };
            if places >= 0 {
                let scale = u8::try_from(places.min(63)).expect("small");
                num(x.rescale(scale, rounding).map_err(overflow)?)
            } else {
                let shift = u32::try_from(-places).expect("positive");
                let factor = Decimal::new(10i128.pow(shift), 0).expect("fits");
                let q = x.div_to_scale(&factor, 0).map_err(overflow)?;
                // Rounding on the dropped digits.
                let rounded = x
                    .div_to_scale(&factor, 1)
                    .map_err(overflow)?
                    .rescale(0, rounding)
                    .map_err(overflow)?;
                let keep = if name == "ROUND" { rounded } else { q };
                num(keep.mul_exact(&factor).map_err(overflow)?)
            }
        }
        "CEILING" | "CEIL" => {
            arity(name, args, 1, 1)?;
            let x = a(0).as_decimal()?;
            let t = x.rescale(0, Rounding::Truncate).map_err(overflow)?;
            if t < x {
                num(t
                    .add_exact(&Decimal::new(1, 0).expect("one"))
                    .map_err(overflow)?)
            } else {
                num(t)
            }
        }
        "FLOOR" => {
            arity(name, args, 1, 1)?;
            let x = a(0).as_decimal()?;
            let t = x.rescale(0, Rounding::Truncate).map_err(overflow)?;
            if t > x {
                num(t
                    .sub_exact(&Decimal::new(1, 0).expect("one"))
                    .map_err(overflow)?)
            } else {
                num(t)
            }
        }
        "POWER" | "POW" => {
            arity(name, args, 2, 2)?;
            from_f64(f64_of(a(0))?.powf(f64_of(a(1))?))?
        }
        "SQRT" => {
            arity(name, args, 1, 1)?;
            from_f64(f64_of(a(0))?.sqrt())?
        }
        "DECIMAL" | "DEC" | "ZONED" => {
            arity(name, args, 1, 3)?;
            let p = args.get(1).map_or(Ok(15), Value::as_i64)?;
            let s = args.get(2).map_or(Ok(0), Value::as_i64)?;
            let shape = Shape::new(u8::try_from(p).unwrap_or(63), u8::try_from(s).unwrap_or(0))
                .map_err(overflow)?;
            num(a(0)
                .as_decimal()?
                .fit(shape, Rounding::Truncate)
                .map_err(overflow)?)
        }
        "INTEGER" | "INT" | "BIGINT" | "SMALLINT" => {
            arity(name, args, 1, 1)?;
            int(a(0).as_i64()?)
        }
        "DOUBLE" | "DOUBLE_PRECISION" | "FLOAT" => {
            arity(name, args, 1, 1)?;
            Value::Double(a(0).as_f64()?)
        }
        "REAL" => {
            arity(name, args, 1, 1)?;
            let t = crate::ast::TypeSpec {
                data_type: crate::ast::DataType::Real,
                ccsid: None,
                bit_data: false,
                udt: None,
            };
            super::eval::cast(a(0), &t)?
        }
        "DATE" => {
            arity(name, args, 1, 1)?;
            match a(0) {
                Value::Num(d) => {
                    let n = d.to_i128().ok_or_else(|| overflow("DATE of a fraction"))?;
                    Value::Date(
                        date_from_days(i64::try_from(n).map_err(overflow)?)
                            .ok_or_else(|| overflow("DATE out of range"))?,
                    )
                }
                v => Value::Date(v.as_date()?),
            }
        }
        "TIME" => {
            arity(name, args, 1, 1)?;
            Value::Time(a(0).as_time()?)
        }
        "TIMESTAMP" => {
            arity(name, args, 1, 2)?;
            match args {
                // TIMESTAMP(x, p): to precision p.
                [v, Value::Num(p)] => {
                    let p = p
                        .to_i128()
                        .and_then(|p| u8::try_from(p).ok())
                        .filter(|p| *p <= 12)
                        .ok_or_else(|| SqlError::Overflow {
                            what: "a timestamp precision of 0 to 12 is needed".into(),
                        })?;
                    Value::Timestamp(v.as_timestamp()?.with_precision(p))
                }
                [d, t] => Value::Timestamp(Timestamp::new(d.as_date()?, t.as_time()?, 0)),
                [v] => Value::Timestamp(v.as_timestamp()?),
                _ => unreachable!(),
            }
        }
        "NOW" => {
            let (secs, picos) = ctx.clock_picos();
            Value::Timestamp(Timestamp::new(
                Date::from_unix(secs),
                Time::from_unix(secs),
                picos,
            ))
        }
        "DECFLOAT" => {
            arity(name, args, 1, 2)?;
            let format = match args.get(1).map(Value::as_i64).transpose()? {
                None | Some(34) => qdecimal::DfFormat::D34,
                Some(16) => qdecimal::DfFormat::D16,
                Some(n) => {
                    return Err(SqlError::Overflow {
                        what: format!("DECFLOAT({n}) is not a type"),
                    });
                }
            };
            Value::DecFloat(
                a(0).as_decfloat(format)?
                    .to_format(format)
                    .map_err(crate::value::decfloat_error)?,
            )
        }
        "NORMALIZE_DECFLOAT" => {
            arity(name, args, 1, 1)?;
            Value::DecFloat(a(0).as_decfloat(a(0).decfloat_format())?.normalized())
        }
        "BINARY" | "VARBINARY" => {
            arity(name, args, 1, 2)?;
            let mut b = a(0).as_bytes()?;
            if let Some(n) = args.get(1) {
                let n = usize::try_from(n.as_i64()?).unwrap_or(0);
                b.truncate(n);
                if name == "BINARY" {
                    b.resize(n, 0);
                }
            }
            Value::Bin(b)
        }
        "BOOLEAN" => {
            arity(name, args, 1, 1)?;
            Value::Bool(a(0).as_bool()?)
        }
        "OCTET_LENGTH" => {
            arity(name, args, 1, 1)?;
            let n = match a(0) {
                Value::Bin(b) => b.len(),
                v => ccsid::to_ebcdic(&v.as_text()).map_or(0, |b| b.len()),
            };
            int(i64::try_from(n).expect("fits"))
        }
        "YEAR" | "MONTH" | "DAY" | "DAYOFMONTH" => {
            arity(name, args, 1, 1)?;
            let d = a(0).as_date()?;
            int(match name {
                "YEAR" => i64::from(d.year),
                "MONTH" => i64::from(d.month),
                _ => i64::from(d.day),
            })
        }
        "HOUR" | "MINUTE" | "SECOND" => {
            arity(name, args, 1, 1)?;
            let t = a(0).as_time()?;
            int(match name {
                "HOUR" => i64::from(t.hour),
                "MINUTE" => i64::from(t.minute),
                _ => i64::from(t.second),
            })
        }
        "MICROSECOND" => {
            arity(name, args, 1, 1)?;
            int(i64::from(a(0).as_timestamp()?.micros()))
        }
        "DAYS" => {
            arity(name, args, 1, 1)?;
            int(days_of(a(0).as_date()?))
        }
        "DAYOFWEEK" => {
            arity(name, args, 1, 1)?;
            int(day_of_week(a(0).as_date()?))
        }
        "DAYOFWEEK_ISO" => {
            arity(name, args, 1, 1)?;
            let w = day_of_week(a(0).as_date()?);
            int(if w == 1 { 7 } else { w - 1 })
        }
        "DAYOFYEAR" => {
            arity(name, args, 1, 1)?;
            int(i64::from(a(0).as_date()?.day_of_year()))
        }
        "WEEK" => {
            arity(name, args, 1, 1)?;
            let d = a(0).as_date()?;
            let jan1 = Date::new(d.year, 1, 1).expect("valid");
            int((i64::from(d.day_of_year()) + day_of_week(jan1) - 2) / 7 + 1)
        }
        "DAYNAME" => {
            arity(name, args, 1, 1)?;
            text(DAYNAMES[usize::try_from(day_of_week(a(0).as_date()?) - 1).expect("0..7")])
        }
        "MONTHNAME" => {
            arity(name, args, 1, 1)?;
            text(MONTHNAMES[usize::from(a(0).as_date()?.month - 1)])
        }
        "JULIAN_DAY" => {
            arity(name, args, 1, 1)?;
            int(days_of(a(0).as_date()?) + 1_721_425)
        }
        "ADD_MONTHS" => {
            arity(name, args, 2, 2)?;
            Value::Date(
                add_months(a(0).as_date()?, a(1).as_i64()?)
                    .ok_or_else(|| overflow("ADD_MONTHS out of range"))?,
            )
        }
        "LAST_DAY" => {
            arity(name, args, 1, 1)?;
            let d = a(0).as_date()?;
            Value::Date(Date::new(d.year, d.month, days_in_month(d.year, d.month)).expect("valid"))
        }
        "NEXT_DAY" => {
            arity(name, args, 2, 2)?;
            let d = a(0).as_date()?;
            let want = a(1).as_text().trim().to_ascii_uppercase();
            let target = DAYNAMES
                .iter()
                .position(|n| {
                    n.to_ascii_uppercase()
                        .starts_with(&want[..want.len().min(3)])
                })
                .ok_or_else(|| SqlError::Conversion {
                    what: format!("{want} is not a day name"),
                })?;
            let today = usize::try_from(day_of_week(d) - 1).expect("0..7");
            let ahead = (target + 7 - today - 1) % 7 + 1;
            Value::Date(
                date_from_days(days_of(d) + i64::try_from(ahead).expect("small"))
                    .ok_or_else(|| overflow("NEXT_DAY out of range"))?,
            )
        }
        "TIMESTAMPDIFF" => {
            arity(name, args, 2, 2)?;
            // IBM: the second argument is a 22-character duration string; we
            // take a numeric duration in seconds too.
            let code = a(0).as_i64()?;
            let secs = a(1).as_decimal()?;
            let factor = match code {
                1 => Decimal::parse("0.000001").expect("ok"),
                2 => Decimal::new(1, 0).expect("ok"),
                4 => Decimal::new(60, 0).expect("ok"),
                8 => Decimal::new(3600, 0).expect("ok"),
                16 => Decimal::new(86_400, 0).expect("ok"),
                32 => Decimal::new(604_800, 0).expect("ok"),
                64 => Decimal::new(2_592_000, 0).expect("ok"),
                128 => Decimal::new(7_776_000, 0).expect("ok"),
                256 => Decimal::new(31_536_000, 0).expect("ok"),
                _ => {
                    return Err(SqlError::Conversion {
                        what: format!("TIMESTAMPDIFF interval {code}"),
                    });
                }
            };
            num(secs.div_to_scale(&factor, 0).map_err(overflow)?)
        }
        "MAX" | "MIN" => {
            arity(name, args, 2, 64)?;
            let want = if name == "MIN" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
            let mut best = a(0).clone();
            for v in &args[1..] {
                if v.compare(&best)? == Some(want) {
                    best = v.clone();
                }
            }
            best
        }
        "LEAP" => {
            arity(name, args, 1, 1)?;
            int(i64::from(is_leap(a(0).as_date()?.year)))
        }
        other => {
            return Err(SqlError::Unsupported {
                what: format!("function {other}"),
            });
        }
    })
}

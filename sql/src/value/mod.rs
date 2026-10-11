// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL values: what expressions produce and rows hold. Text is UTF-8
//! here and CP037 in the record; the field's attributes decide the
//! conversion both ways.

use std::cmp::Ordering;
use std::fmt;

use db::record::datetime::{Date, Time};
use dds::{DataType, DateFmt, FieldDef, TimeFmt};
use qdecimal::{Decimal, Rounding, Shape};

use crate::error::{Result, SqlError};

/// A timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timestamp {
    /// Date.
    pub date: Date,
    /// Time.
    pub time: Time,
    /// Microseconds.
    pub micros: u32,
}

impl Timestamp {
    /// Parse `yyyy-mm-dd hh:mm:ss[.ffffff]` or the `-`/`.` IBM spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.len() < 19 {
            return None;
        }
        let date = Date::parse(&t[..10], DateFmt::Iso)?;
        let time = Time::parse(&t[11..19], TimeFmt::Hms)?;
        let micros = match t.get(20..) {
            Some(f) if !f.is_empty() => {
                let mut s = f.to_string();
                while s.len() < 6 {
                    s.push('0');
                }
                s[..6].parse().ok()?
            }
            _ => 0,
        };
        Some(Self { date, time, micros })
    }

    /// `yyyy-mm-dd-hh.mm.ss.ffffff`, as a `Z` field holds it.
    #[must_use]
    pub fn render_field(&self) -> String {
        format!(
            "{}-{}.{:06}",
            self.date.render(DateFmt::Iso, None),
            self.time.render(TimeFmt::Iso, None),
            self.micros
        )
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}.{:06}",
            self.date.render(DateFmt::Iso, None),
            self.time.render(TimeFmt::Hms, None),
            self.micros
        )
    }
}

/// A value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// The null value.
    Null,
    /// A number.
    Num(Decimal),
    /// A character string.
    Str(String),
    /// A date.
    Date(Date),
    /// A time.
    Time(Time),
    /// A timestamp.
    Timestamp(Timestamp),
}

/// Dates in order.
#[must_use]
pub fn cmp_date(a: &Date, b: &Date) -> Ordering {
    (a.year, a.month, a.day).cmp(&(b.year, b.month, b.day))
}

/// Times in order.
#[must_use]
pub fn cmp_time(a: &Time, b: &Time) -> Ordering {
    (a.hour, a.minute, a.second).cmp(&(b.hour, b.minute, b.second))
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_date(&self.date, &other.date)
            .then_with(|| cmp_time(&self.time, &other.time))
            .then(self.micros.cmp(&other.micros))
    }
}

fn conversion(what: impl fmt::Display) -> SqlError {
    SqlError::Conversion {
        what: what.to_string(),
    }
}

/// A date from text in the ISO, USA, EUR or two-digit forms.
#[must_use]
pub fn parse_date(text: &str) -> Option<Date> {
    [
        DateFmt::Iso,
        DateFmt::Usa,
        DateFmt::Eur,
        DateFmt::Mdy,
        DateFmt::Jul,
    ]
    .iter()
    .find_map(|f| Date::parse(text, *f))
}

/// A time from text in any format.
#[must_use]
pub fn parse_time(text: &str) -> Option<Time> {
    Time::parse(text, TimeFmt::Hms).or_else(|| Time::parse(text, TimeFmt::Usa))
}

impl Value {
    /// Whether null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// An integer.
    #[must_use]
    pub fn int(n: i64) -> Self {
        Self::Num(Decimal::new(i128::from(n), 0).expect("fits"))
    }

    /// The value as a number; text is parsed.
    pub fn as_decimal(&self) -> Result<Decimal> {
        match self {
            Self::Num(d) => Ok(d.clone()),
            Self::Str(s) => {
                Decimal::parse(s.trim()).map_err(|_| conversion(format!("'{s}' is not a number")))
            }
            v => Err(conversion(format!("{} is not a number", v.type_name()))),
        }
    }

    /// The value as an integer.
    pub fn as_i64(&self) -> Result<i64> {
        let d = self.as_decimal()?;
        let t = d.rescale(0, Rounding::Truncate).map_err(conversion)?;
        t.to_i128()
            .and_then(|n| i64::try_from(n).ok())
            .ok_or_else(|| conversion(format!("{d} is too large for an integer")))
    }

    /// The value as text.
    #[must_use]
    pub fn as_text(&self) -> String {
        self.to_string()
    }

    /// The value as a date; text is parsed.
    pub fn as_date(&self) -> Result<Date> {
        match self {
            Self::Date(d) => Ok(*d),
            Self::Timestamp(t) => Ok(t.date),
            Self::Str(s) => parse_date(s).ok_or_else(|| conversion(format!("'{s}' is not a date"))),
            v => Err(conversion(format!("{} is not a date", v.type_name()))),
        }
    }

    /// The value as a time; text is parsed.
    pub fn as_time(&self) -> Result<Time> {
        match self {
            Self::Time(t) => Ok(*t),
            Self::Timestamp(t) => Ok(t.time),
            Self::Str(s) => parse_time(s).ok_or_else(|| conversion(format!("'{s}' is not a time"))),
            v => Err(conversion(format!("{} is not a time", v.type_name()))),
        }
    }

    /// The value as a timestamp; text is parsed, a date gets midnight.
    pub fn as_timestamp(&self) -> Result<Timestamp> {
        match self {
            Self::Timestamp(t) => Ok(*t),
            Self::Date(d) => Ok(Timestamp {
                date: *d,
                time: Time::new(0, 0, 0).expect("midnight"),
                micros: 0,
            }),
            Self::Str(s) => {
                Timestamp::parse(s).ok_or_else(|| conversion(format!("'{s}' is not a timestamp")))
            }
            v => Err(conversion(format!("{} is not a timestamp", v.type_name()))),
        }
    }

    /// The SQL type family, for messages.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "NULL",
            Self::Num(_) => "numeric",
            Self::Str(_) => "character",
            Self::Date(_) => "date",
            Self::Time(_) => "time",
            Self::Timestamp(_) => "timestamp",
        }
    }

    /// Compare two values; `None` when either is null. Text compares in
    /// EBCDIC order with blank padding; text against a date, time or
    /// timestamp is parsed; a number against text is an error.
    pub fn compare(&self, other: &Self) -> Result<Option<Ordering>> {
        Ok(Some(match (self, other) {
            (Self::Null, _) | (_, Self::Null) => return Ok(None),
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            (Self::Str(a), Self::Str(b)) => ccsid::ebcdic_cmp_str(a.trim_end(), b.trim_end()),
            (Self::Date(a), Self::Date(b)) => cmp_date(a, b),
            (Self::Time(a), Self::Time(b)) => cmp_time(a, b),
            (Self::Timestamp(a), Self::Timestamp(b)) => a.cmp(b),
            (Self::Date(a), Self::Str(_)) => cmp_date(a, &other.as_date()?),
            (Self::Str(_), Self::Date(b)) => cmp_date(&self.as_date()?, b),
            (Self::Time(a), Self::Str(_)) => cmp_time(a, &other.as_time()?),
            (Self::Str(_), Self::Time(b)) => cmp_time(&self.as_time()?, b),
            (Self::Timestamp(a), Self::Str(_)) => a.cmp(&other.as_timestamp()?),
            (Self::Str(_), Self::Timestamp(b)) => self.as_timestamp()?.cmp(b),
            (a, b) => {
                return Err(SqlError::TypeMismatch {
                    what: format!("{} with {}", a.type_name(), b.type_name()),
                });
            }
        }))
    }

    /// A canonical text for grouping and `DISTINCT`: equal values give
    /// equal text.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Null => "\u{0}".into(),
            Self::Num(d) => {
                let s = d.to_string();
                if s.contains('.') {
                    let t = s.trim_end_matches('0').trim_end_matches('.');
                    if t == "-0" || t.is_empty() {
                        "0".into()
                    } else {
                        t.to_string()
                    }
                } else {
                    s
                }
            }
            Self::Str(s) => format!("'{}", s.trim_end()),
            other => other.to_string(),
        }
    }

    /// From a field's stored value.
    pub fn from_field(f: &FieldDef, v: &db::record::Value) -> Result<Self> {
        Ok(match v {
            db::record::Value::Null => Self::Null,
            db::record::Value::Num(d) => Self::Num(d.clone()),
            db::record::Value::Char(b) => {
                let s = ccsid::from_ebcdic(b);
                match f.data_type {
                    DataType::Date => {
                        Self::Date(Date::parse(&s, f.date_fmt.unwrap_or_default()).ok_or_else(
                            || conversion(format!("field {} holds '{s}', not a date", f.name)),
                        )?)
                    }
                    DataType::Time => {
                        Self::Time(Time::parse(&s, f.time_fmt.unwrap_or_default()).ok_or_else(
                            || conversion(format!("field {} holds '{s}', not a time", f.name)),
                        )?)
                    }
                    DataType::Timestamp => {
                        Self::Timestamp(Timestamp::parse(&s).ok_or_else(|| {
                            conversion(format!("field {} holds '{s}', not a timestamp", f.name))
                        })?)
                    }
                    _ => Self::Str(s),
                }
            }
        })
    }

    /// Into a field's value: text to CP037, dates and times in the
    /// field's format, numbers fitted to the field (truncated).
    pub fn to_field(&self, f: &FieldDef) -> Result<db::record::Value> {
        if self.is_null() {
            return Ok(db::record::Value::Null);
        }
        let text = |s: &str| -> Result<db::record::Value> {
            ccsid::to_ebcdic(s)
                .map(db::record::Value::Char)
                .map_err(|_| conversion(format!("'{s}' has characters CP037 cannot hold")))
        };
        match f.data_type {
            DataType::Zoned | DataType::Packed | DataType::Binary | DataType::Float => {
                let d = self.as_decimal()?;
                let shape = Shape::new(u8::try_from(f.length).unwrap_or(63), f.decimals)
                    .map_err(conversion)?;
                let fitted = d
                    .fit(shape, Rounding::Truncate)
                    .map_err(|_| SqlError::Overflow {
                        what: format!("{d} does not fit field {}", f.name),
                    })?;
                Ok(db::record::Value::Num(fitted))
            }
            DataType::Date => text(
                &self
                    .as_date()?
                    .render(f.date_fmt.unwrap_or_default(), f.date_sep),
            ),
            DataType::Time => text(
                &self
                    .as_time()?
                    .render(f.time_fmt.unwrap_or_default(), f.time_sep),
            ),
            DataType::Timestamp => text(&self.as_timestamp()?.render_field()),
            _ => {
                let s = self.as_text();
                let max = usize::try_from(f.length).expect("fits");
                if s.chars().count() > max && s.trim_end().chars().count() > max {
                    return Err(SqlError::Overflow {
                        what: format!("'{s}' is longer than field {} ({max})", f.name),
                    });
                }
                text(&s)
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("NULL"),
            Self::Num(d) => write!(f, "{d}"),
            Self::Str(s) => f.write_str(s),
            Self::Date(d) => f.write_str(&d.render(DateFmt::Iso, None)),
            Self::Time(t) => f.write_str(&t.render(TimeFmt::Iso, None)),
            Self::Timestamp(t) => write!(f, "{t}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_compares() {
        let a = Value::Num(Decimal::parse("1.50").unwrap());
        let b = Value::Num(Decimal::parse("1.5").unwrap());
        assert_eq!(a.key(), b.key());
        assert_eq!(Value::Num(Decimal::parse("0.00").unwrap()).key(), "0");
        assert_eq!(
            Value::Str("ab  ".into()).key(),
            Value::Str("ab".into()).key()
        );
        assert_eq!(a.compare(&b).unwrap(), Some(Ordering::Equal));
        assert_eq!(Value::Null.compare(&a).unwrap(), None);
        assert!(Value::Str("x".into()).compare(&a).is_err());
        let d = Value::Date(Date::new(2026, 1, 2).unwrap());
        assert_eq!(
            d.compare(&Value::Str("2026-01-02".into())).unwrap(),
            Some(Ordering::Equal)
        );
        assert_eq!(
            d.compare(&Value::Str("01/03/2026".into())).unwrap(),
            Some(Ordering::Less)
        );
        let ts = Timestamp::parse("2026-01-02-10.20.30.000123").unwrap();
        assert_eq!(ts.to_string(), "2026-01-02 10:20:30.000123");
        assert_eq!(ts.render_field(), "2026-01-02-10.20.30.000123");
        assert_eq!(Timestamp::parse("2026-01-02 10:20:30").unwrap().micros, 0);
    }
}

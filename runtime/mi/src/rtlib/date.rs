// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dates, times and timestamps in storage, and the `dt.*` instructions.

use codegen::Cond;
use codegen::abi::{DATE_FMTS, Env, TIME_FMTS};
use db::record::datetime::{Date, Time, days_in_month};
use dds::{DateFmt, TimeFmt};
use qir::Type;

use super::core::raise_on;
use super::mem::{job, ty, typed};
use super::string::{Text, read_text, write_text};

/// A date, time or timestamp value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Dt {
    /// The date part.
    pub date: Option<Date>,
    /// The time part.
    pub time: Option<Time>,
    /// Microseconds of a timestamp.
    pub micros: u32,
}

const UNIT_YEARS: i64 = 0;
const UNIT_MONTHS: i64 = 1;
const UNIT_DAYS: i64 = 2;
const UNIT_HOURS: i64 = 3;
const UNIT_MINUTES: i64 = 4;
const UNIT_SECONDS: i64 = 5;
const UNIT_MSECONDS: i64 = 6;

/// Days since 1970-01-01 of a civil date.
pub(crate) fn days_from_civil(d: Date) -> i64 {
    let (y, m) = if d.month <= 2 {
        (i64::from(d.year) - 1, i64::from(d.month) + 9)
    } else {
        (i64::from(d.year), i64::from(d.month) - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + i64::from(d.day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a day count since 1970-01-01.
pub(crate) fn civil_from_days(z: i64) -> Option<Date> {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).ok()?;
    let month = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).ok()?;
    let year = yoe + era * 400 + i64::from(month <= 2);
    Date::new(u16::try_from(year).ok()?, month, day)
}

fn narrow(t: &Text) -> String {
    t.units
        .iter()
        .map(|u| ccsid::char_of(u8::try_from(*u).unwrap_or(0x6F)))
        .collect()
}

fn parse_timestamp(s: &str) -> Option<Dt> {
    let s = s.trim_end();
    if s.len() < 19 {
        return None;
    }
    let date = Date::parse(&s[..10], DateFmt::Iso)?;
    let time = Time::parse(&s[11..19], TimeFmt::Iso)?;
    let frac = s.get(20..).unwrap_or("");
    let frac: String = frac.chars().take(6).collect();
    let micros = if frac.is_empty() {
        0
    } else {
        let padded = format!("{frac:0<6}");
        padded.parse::<u32>().ok()?
    };
    Some(Dt {
        date: Some(date),
        time: Some(time),
        micros,
    })
}

fn render_timestamp(d: Dt, p: u8) -> String {
    let date = d.date.unwrap_or(Date {
        year: 1,
        month: 1,
        day: 1,
    });
    let time = d.time.unwrap_or(Time {
        hour: 0,
        minute: 0,
        second: 0,
    });
    let mut s = format!(
        "{}-{}",
        date.render(DateFmt::Iso, None),
        time.render(TimeFmt::Iso, None)
    );
    if p > 0 {
        let frac = format!("{:06}", d.micros);
        let frac: String = frac
            .chars()
            .chain(std::iter::repeat('0'))
            .take(usize::from(p))
            .collect();
        s.push('.');
        s.push_str(&frac);
    }
    s
}

/// Parse text as a value of a date, time or timestamp type.
pub(crate) fn parse_dt(text: &str, t: &Type) -> Option<Dt> {
    match t {
        Type::Date(f) => Some(Dt {
            date: Some(Date::parse(text, *f)?),
            time: None,
            micros: 0,
        }),
        Type::Time(f) => Some(Dt {
            date: None,
            time: Some(Time::parse(text, *f)?),
            micros: 0,
        }),
        Type::Timestamp(_) => parse_timestamp(text),
        _ => None,
    }
}

/// Render a value in a date, time or timestamp type.
pub(crate) fn render_dt(d: Dt, t: &Type) -> String {
    match t {
        Type::Date(f) => d.date.map_or_else(String::new, |x| x.render(*f, None)),
        Type::Time(f) => d.time.map_or_else(String::new, |x| x.render(*f, None)),
        Type::Timestamp(p) => render_timestamp(d, *p),
        _ => String::new(),
    }
}

/// Read a date, time or timestamp operand.
pub(crate) fn read_dt(addr: i64, desc: i64) -> Result<Dt, Cond> {
    let t = ty(desc);
    let text = narrow(&read_text(addr, desc));
    parse_dt(&text, &t).ok_or(Cond::Mch1602)
}

/// Write a date, time or timestamp operand.
pub(crate) fn write_dt(addr: i64, desc: i64, d: Dt) {
    let t = ty(desc);
    let s = render_dt(d, &t);
    let units: Vec<u16> = s
        .bytes()
        .map(|b| u16::from(ccsid::byte_of(char::from(b)).unwrap_or(0x40)))
        .collect();
    write_text(addr, desc, &Text { units, wide: false });
}

fn add_months(d: Date, n: i64) -> Option<Date> {
    let total = i64::from(d.year) * 12 + i64::from(d.month) - 1 + n;
    let year = u16::try_from(total.div_euclid(12)).ok()?;
    let month = u8::try_from(total.rem_euclid(12) + 1).ok()?;
    let day = d.day.min(days_in_month(year, month));
    Date::new(year, month, day)
}

fn secs_of(t: Time) -> i64 {
    i64::from(t.hour) * 3600 + i64::from(t.minute) * 60 + i64::from(t.second)
}

fn time_of(secs: i64) -> Time {
    let s = secs.rem_euclid(86_400);
    Time {
        hour: u8::try_from(s / 3600).expect("< 24"),
        minute: u8::try_from((s % 3600) / 60).expect("< 60"),
        second: u8::try_from(s % 60).expect("< 60"),
    }
}

/// Microseconds since the epoch of a timestamp.
fn micros_of(d: Dt) -> i128 {
    let days = d.date.map_or(0, days_from_civil);
    let secs = d.time.map_or(0, secs_of);
    (i128::from(days) * 86_400 + i128::from(secs)) * 1_000_000 + i128::from(d.micros)
}

fn dt_of_micros(m: i128) -> Option<Dt> {
    let secs = m.div_euclid(1_000_000);
    let micros = u32::try_from(m.rem_euclid(1_000_000)).ok()?;
    let days = i64::try_from(secs.div_euclid(86_400)).ok()?;
    let sod = i64::try_from(secs.rem_euclid(86_400)).ok()?;
    Some(Dt {
        date: Some(civil_from_days(days)?),
        time: Some(time_of(sod)),
        micros,
    })
}

fn unit_secs(unit: i64) -> Option<i64> {
    Some(match unit {
        UNIT_HOURS => 3600,
        UNIT_MINUTES => 60,
        UNIT_SECONDS => 1,
        _ => return None,
    })
}

fn add(d: Dt, t: &Type, n: i64, unit: i64) -> Option<Dt> {
    match t {
        Type::Date(_) => {
            let date = d.date?;
            let date = match unit {
                UNIT_YEARS => add_months(date, n.checked_mul(12)?)?,
                UNIT_MONTHS => add_months(date, n)?,
                UNIT_DAYS => civil_from_days(days_from_civil(date).checked_add(n)?)?,
                _ => return None,
            };
            Some(Dt {
                date: Some(date),
                ..d
            })
        }
        Type::Time(_) => {
            let secs = secs_of(d.time?).checked_add(n.checked_mul(unit_secs(unit)?)?)?;
            Some(Dt {
                time: Some(time_of(secs)),
                ..d
            })
        }
        Type::Timestamp(_) => match unit {
            UNIT_YEARS | UNIT_MONTHS => {
                let date = d.date?;
                let months = if unit == UNIT_YEARS {
                    n.checked_mul(12)?
                } else {
                    n
                };
                Some(Dt {
                    date: Some(add_months(date, months)?),
                    ..d
                })
            }
            _ => {
                let per: i128 = match unit {
                    UNIT_DAYS => 86_400_000_000,
                    UNIT_MSECONDS => 1,
                    u => i128::from(unit_secs(u)?) * 1_000_000,
                };
                dt_of_micros(micros_of(d) + i128::from(n) * per)
            }
        },
        _ => None,
    }
}

fn whole_months(a: Date, b: Date) -> i64 {
    let mut months =
        (i64::from(a.year) - i64::from(b.year)) * 12 + i64::from(a.month) - i64::from(b.month);
    if months > 0 && a.day < b.day {
        months -= 1;
    } else if months < 0 && a.day > b.day {
        months += 1;
    }
    months
}

#[allow(clippy::many_single_char_names)]
fn diff(a: Dt, b: Dt, t: &Type, unit: i64) -> Option<i64> {
    match t {
        Type::Date(_) => {
            let (x, y) = (a.date?, b.date?);
            Some(match unit {
                UNIT_YEARS => whole_months(x, y) / 12,
                UNIT_MONTHS => whole_months(x, y),
                UNIT_DAYS => days_from_civil(x) - days_from_civil(y),
                _ => return None,
            })
        }
        Type::Time(_) => {
            let d = secs_of(a.time?) - secs_of(b.time?);
            Some(d / unit_secs(unit)?)
        }
        Type::Timestamp(_) => {
            let (x, y) = (a.date?, b.date?);
            match unit {
                UNIT_YEARS | UNIT_MONTHS => {
                    let mut months = whole_months(x, y);
                    // Same day of month: the time of day decides the last month.
                    if x.day == y.day {
                        let ta = secs_of(a.time?) * 1_000_000 + i64::from(a.micros);
                        let tb = secs_of(b.time?) * 1_000_000 + i64::from(b.micros);
                        if months > 0 && ta < tb {
                            months -= 1;
                        } else if months < 0 && ta > tb {
                            months += 1;
                        }
                    }
                    Some(if unit == UNIT_YEARS {
                        months / 12
                    } else {
                        months
                    })
                }
                _ => {
                    let per: i128 = match unit {
                        UNIT_DAYS => 86_400_000_000,
                        UNIT_MSECONDS => 1,
                        u => i128::from(unit_secs(u)?) * 1_000_000,
                    };
                    i64::try_from((micros_of(a) - micros_of(b)) / per).ok()
                }
            }
        }
        _ => None,
    }
}

fn extract(d: Dt, unit: i64) -> Option<i32> {
    Some(match unit {
        UNIT_YEARS => i32::from(d.date?.year),
        UNIT_MONTHS => i32::from(d.date?.month),
        UNIT_DAYS => i32::from(d.date?.day),
        UNIT_HOURS => i32::from(d.time?.hour),
        UNIT_MINUTES => i32::from(d.time?.minute),
        UNIT_SECONDS => i32::from(d.time?.second),
        UNIT_MSECONDS => i32::try_from(d.micros).ok()?,
        _ => return None,
    })
}

/// `dt_add(env, dst, dty, src, sty, n, unit)`.
pub extern "sysv64" fn dt_add(
    env: *mut Env,
    dst: i64,
    dty: i64,
    src: i64,
    sty: i64,
    n: i64,
    unit: i64,
) {
    let r = read_dt(src, sty).and_then(|d| add(d, &ty(sty), n, unit).ok_or(Cond::Mch1602));
    match r {
        Ok(d) => write_dt(dst, dty, d),
        Err(c) => raise_on(job(env), c),
    }
}

/// `dt_diff(env, a, aty, b, bty, unit) -> i64`.
pub extern "sysv64" fn dt_diff(
    env: *mut Env,
    a: i64,
    aty: i64,
    b: i64,
    bty: i64,
    unit: i64,
) -> i64 {
    let r = (|| {
        let x = read_dt(a, aty)?;
        let y = read_dt(b, bty)?;
        diff(x, y, &ty(aty), unit).ok_or(Cond::Mch1602)
    })();
    match r {
        Ok(v) => v,
        Err(c) => {
            raise_on(job(env), c);
            0
        }
    }
}

/// `dt_extract(env, src, sty, unit) -> i32`.
pub extern "sysv64" fn dt_extract(env: *mut Env, src: i64, sty: i64, unit: i64) -> i32 {
    match read_dt(src, sty).and_then(|d| extract(d, unit).ok_or(Cond::Mch1602)) {
        Ok(v) => v,
        Err(c) => {
            raise_on(job(env), c);
            0
        }
    }
}

/// `dt_test(env, src, sty, datfmt, timfmt) -> 0|1`: whether the operand
/// holds a valid value; `datfmt`/`timfmt` are format indexes plus one.
pub extern "sysv64" fn dt_test(
    _env: *mut Env,
    src: i64,
    sty: i64,
    datfmt: i64,
    timfmt: i64,
) -> i32 {
    let t = ty(sty);
    let text = if t.is_numeric() {
        match super::decimal::read_num(src, sty) {
            Ok(v) => v.value.abs().to_string().replace('.', ""),
            Err(_) => return 0,
        }
    } else {
        narrow(&read_text(src, sty))
    };
    let ok = if datfmt > 0 {
        DATE_FMTS
            .get(usize::try_from(datfmt - 1).unwrap_or(usize::MAX))
            .is_some_and(|f| Date::parse(&text, *f).is_some())
    } else if timfmt > 0 {
        TIME_FMTS
            .get(usize::try_from(timfmt - 1).unwrap_or(usize::MAX))
            .is_some_and(|f| Time::parse(&text, *f).is_some())
    } else {
        parse_dt(&text, &t).is_some() || parse_timestamp(&text).is_some()
    };
    i32::from(ok)
}

/// The current date and time.
pub(crate) fn now() -> Dt {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Dt {
        date: Some(Date::from_unix(since.as_secs())),
        time: Some(Time::from_unix(since.as_secs())),
        micros: since.subsec_micros(),
    }
}

/// `dt_now(env, dst, dty)`.
pub extern "sysv64" fn dt_now(_env: *mut Env, dst: i64, dty: i64) {
    let (_, t) = typed(dst, dty);
    let _ = t;
    write_dt(dst, dty, now());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days_round_trip() {
        for (y, m, d, days) in [
            (1970, 1, 1, 0),
            (2000, 3, 1, 11_017),
            (1999, 12, 31, 10_956),
            (2026, 10, 20, 20_746),
        ] {
            let date = Date::new(y, m, d).unwrap();
            assert_eq!(days_from_civil(date), days);
            assert_eq!(civil_from_days(days), Some(date));
        }
    }

    #[test]
    fn month_arithmetic_clamps_and_counts() {
        let jan31 = Date::new(2024, 1, 31).unwrap();
        assert_eq!(add_months(jan31, 1), Date::new(2024, 2, 29));
        assert_eq!(add_months(jan31, -2), Date::new(2023, 11, 30));
        assert_eq!(whole_months(Date::new(2024, 3, 30).unwrap(), jan31), 1);
        assert_eq!(whole_months(Date::new(2024, 3, 31).unwrap(), jan31), 2);
        assert_eq!(whole_months(jan31, Date::new(2024, 3, 15).unwrap()), -1);
    }

    #[test]
    fn timestamps_parse_and_render() {
        let t = parse_timestamp("2026-10-20-13.45.07.123456").unwrap();
        assert_eq!(t.micros, 123_456);
        assert_eq!(render_timestamp(t, 6), "2026-10-20-13.45.07.123456");
        assert_eq!(render_timestamp(t, 0), "2026-10-20-13.45.07");
        assert_eq!(render_timestamp(t, 3), "2026-10-20-13.45.07.123");
        let later = add(t, &Type::Timestamp(6), 90, UNIT_MINUTES).unwrap();
        assert_eq!(render_timestamp(later, 0), "2026-10-20-15.15.07");
        assert_eq!(
            diff(later, t, &Type::Timestamp(6), UNIT_SECONDS),
            Some(5400)
        );
    }
}

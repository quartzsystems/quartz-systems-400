// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cvt` between storage classes: text and numbers, text and dates,
//! numbers and dates, narrow and wide text, indicators and text.

use codegen::Cond;
use codegen::abi::{DATE_FMTS, Env, TIME_FMTS};
use codegen::rt::cvt_opts;
use db::record::datetime::Time;
use dds::{DateFmt, TimeFmt};
use qdecimal::{Decimal, Rounding};
use qir::{Type, TypeClass as C};

use super::core::raise_on;
use super::date::{Dt, parse_dt, read_dt, render_dt, write_dt};
use super::decimal::{read_num, write_num};
use super::mem::{job, ty, typed};
use super::string::{Text, read_text, write_text};

fn narrow(t: &Text) -> String {
    t.units
        .iter()
        .map(|u| ccsid::char_of(u8::try_from(*u).unwrap_or(0x6F)))
        .collect()
}

fn text_of(s: &str) -> Text {
    Text {
        units: s
            .chars()
            .map(|c| ccsid::byte_of(c).map_or(0x6F, u16::from))
            .collect(),
        wide: false,
    }
}

/// Parse text as a number the way `%DEC` does: blanks removed, a sign
/// before or after, `.` or `,` as the point.
pub(crate) fn parse_number(s: &str) -> Option<Decimal> {
    let mut t: String = s.chars().filter(|c| *c != ' ').collect();
    if t.is_empty() {
        return Some(Decimal::zero());
    }
    let mut neg = false;
    if let Some(rest) = t.strip_suffix('-') {
        neg = true;
        t = rest.to_string();
    } else if let Some(rest) = t.strip_suffix('+') {
        t = rest.to_string();
    }
    if let Some(rest) = t.strip_prefix('-') {
        neg = !neg;
        t = rest.to_string();
    } else if let Some(rest) = t.strip_prefix('+') {
        t = rest.to_string();
    }
    let t = t.replace(',', ".");
    if t.is_empty() || t == "." {
        return Some(Decimal::zero());
    }
    let v = Decimal::parse(&t).ok()?;
    Some(if neg { v.negated() } else { v })
}

fn date_fmt(opts: i64) -> Option<DateFmt> {
    let i = usize::try_from((opts >> cvt_opts::DATFMT_SHIFT) & 0xFF).ok()?;
    if i == 0 {
        None
    } else {
        DATE_FMTS.get(i - 1).copied()
    }
}

fn time_fmt(opts: i64) -> Option<TimeFmt> {
    let i = usize::try_from((opts >> cvt_opts::TIMFMT_SHIFT) & 0xFF).ok()?;
    if i == 0 {
        None
    } else {
        TIME_FMTS.get(i - 1).copied()
    }
}

/// The text form of a date type under the formats the options name.
fn text_type_for(dt: &Type, opts: i64) -> Type {
    match dt {
        Type::Date(f) => Type::Date(date_fmt(opts).unwrap_or(*f)),
        Type::Time(f) => Type::Time(time_fmt(opts).unwrap_or(*f)),
        other => other.clone(),
    }
}

/// Digits of a date value in a format, as `%DEC(date:*FMT)` gives.
fn digits_of(d: Dt, t: &Type, opts: i64) -> String {
    let shown = text_type_for(t, opts);
    let s = render_dt(d, &shown);
    s.chars().filter(char::is_ascii_digit).collect()
}

/// A date value from digits in a format, as `%DATE(num:*FMT)` reads.
fn from_digits(digits: &str, t: &Type, opts: i64) -> Option<Dt> {
    let shown = text_type_for(t, opts);
    let want = match &shown {
        Type::Date(f) => usize::try_from(f.length()).expect("small") - 2,
        Type::Time(_) => 6,
        Type::Timestamp(p) => 14 + usize::from(*p),
        _ => return None,
    };
    let padded = format!("{digits:0>want$}");
    match &shown {
        Type::Date(f) => {
            let text = insert_separators(&padded, *f);
            parse_dt(&text, &shown)
        }
        Type::Time(_) => Some(Dt {
            date: None,
            time: Some(Time::new(
                padded[..2].parse().ok()?,
                padded[2..4].parse().ok()?,
                padded[4..6].parse().ok()?,
            )?),
            micros: 0,
        }),
        Type::Timestamp(_) => {
            let text = format!(
                "{}-{}-{}-{}.{}.{}.{}",
                &padded[..4],
                &padded[4..6],
                &padded[6..8],
                &padded[8..10],
                &padded[10..12],
                &padded[12..14],
                &padded[14..]
            );
            parse_dt(&text, &shown)
        }
        _ => None,
    }
}

/// Put a format's separators back into bare digits so the parser
/// splits the fields where the format does.
fn insert_separators(digits: &str, f: DateFmt) -> String {
    let groups: &[usize] = match f {
        DateFmt::Iso | DateFmt::Jis => &[4, 2, 2],
        DateFmt::Usa | DateFmt::Eur => &[2, 2, 4],
        DateFmt::Mdy | DateFmt::Dmy | DateFmt::Ymd => &[2, 2, 2],
        DateFmt::Jul => &[2, 3],
    };
    let mut out = String::new();
    let mut at = 0;
    for (i, g) in groups.iter().enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(digits.get(at..at + g).unwrap_or(""));
        at += g;
    }
    out
}

#[allow(clippy::too_many_lines)]
fn convert_inner(dst: i64, dty: i64, src: i64, sty: i64, opts: i64) -> Result<(), Cond> {
    let dt = ty(dty);
    let st = ty(sty);
    let rounding = if opts & cvt_opts::HALFADJ != 0 {
        Rounding::HalfAdjust
    } else {
        Rounding::Truncate
    };
    match (dt.class(), st.class()) {
        // Text to text of the other width.
        (C::Char, C::Ucs2 | C::Graphic) | (C::Ucs2 | C::Graphic, C::Char) => {
            let t = read_text(src, sty);
            write_text(dst, dty, &t);
        }
        // Text to number.
        (C::Int | C::UInt | C::Float | C::Decimal, C::Char) => {
            let s = narrow(&read_text(src, sty));
            let v = parse_number(&s).ok_or(Cond::Mch1202)?;
            write_num(dst, dty, &v, rounding)?;
        }
        // Number to text.
        (C::Char, C::Int | C::UInt | C::Float | C::Decimal) => {
            let v = read_num(src, sty)?.value;
            write_text(dst, dty, &text_of(&v.to_string()));
        }
        // Text to date.
        (C::DateTime, C::Char) => {
            let s = narrow(&read_text(src, sty));
            let shown = text_type_for(&dt, opts);
            let d = parse_dt(s.trim_end(), &shown).ok_or(Cond::Mch1602)?;
            write_dt(dst, dty, d);
        }
        // Date to text.
        (C::Char, C::DateTime) => {
            let d = read_dt(src, sty)?;
            let shown = text_type_for(&st, opts);
            write_text(dst, dty, &text_of(&render_dt(d, &shown)));
        }
        // Number to date.
        (C::DateTime, C::Int | C::UInt | C::Decimal) => {
            let v = read_num(src, sty)?.value;
            let digits = v
                .abs()
                .rescale(0, Rounding::Truncate)
                .map_err(|_| Cond::Mch1602)?
                .to_string();
            let d = from_digits(&digits, &dt, opts).ok_or(Cond::Mch1602)?;
            write_dt(dst, dty, d);
        }
        // Date to number.
        (C::Int | C::UInt | C::Decimal, C::DateTime) => {
            let d = read_dt(src, sty)?;
            let digits = digits_of(d, &st, opts);
            let v = Decimal::parse(&digits).map_err(|_| Cond::Mch1602)?;
            write_num(dst, dty, &v, Rounding::Truncate)?;
        }
        // Between date types.
        (C::DateTime, C::DateTime) => {
            let d = read_dt(src, sty)?;
            let out = match (&dt, &st) {
                (Type::Timestamp(_), Type::Date(_)) => Dt {
                    date: d.date,
                    time: Some(Time {
                        hour: 0,
                        minute: 0,
                        second: 0,
                    }),
                    micros: 0,
                },
                (Type::Date(_), Type::Timestamp(_)) => Dt {
                    date: d.date,
                    time: None,
                    micros: 0,
                },
                (Type::Time(_), Type::Timestamp(_)) => Dt {
                    date: None,
                    time: d.time,
                    micros: 0,
                },
                (Type::Date(_), Type::Date(_))
                | (Type::Time(_), Type::Time(_))
                | (Type::Timestamp(_), Type::Timestamp(_)) => d,
                _ => return Err(Cond::Mch1602),
            };
            write_dt(dst, dty, out);
        }
        // Indicator and text.
        (C::Ind, C::Char) => {
            let s = narrow(&read_text(src, sty));
            let (b, _) = typed(dst, dty);
            b[0] = if s.trim_end() == "1" { 0xF1 } else { 0xF0 };
        }
        (C::Char, C::Ind) => {
            let (b, _) = typed(src, sty);
            let on = b[0] == 0xF1;
            write_text(dst, dty, &text_of(if on { "1" } else { "0" }));
        }
        _ => return Err(Cond::Mch1202),
    }
    Ok(())
}

/// `convert(env, dst, dty, src, sty, opts)`.
pub extern "sysv64" fn convert(env: *mut Env, dst: i64, dty: i64, src: i64, sty: i64, opts: i64) {
    if let Err(c) = convert_inner(dst, dty, src, sty, opts) {
        raise_on(job(env), c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_parse_like_dec() {
        assert_eq!(parse_number(" 12.50 ").unwrap().to_string(), "12.50");
        assert_eq!(parse_number("12.5-").unwrap().to_string(), "-12.5");
        assert_eq!(parse_number("-1,25").unwrap().to_string(), "-1.25");
        assert_eq!(parse_number("   ").unwrap(), Decimal::zero());
        assert!(parse_number("1x").is_none());
    }

    #[test]
    fn date_digits_round_trip() {
        let d = Dt {
            date: db::record::datetime::Date::new(2026, 3, 1),
            time: None,
            micros: 0,
        };
        let iso = Type::Date(DateFmt::Iso);
        assert_eq!(digits_of(d, &iso, 0), "20260301");
        let mdy_opts =
            (i64::from(codegen::abi::date_fmt_index(DateFmt::Mdy)) + 1) << cvt_opts::DATFMT_SHIFT;
        assert_eq!(digits_of(d, &iso, mdy_opts), "030126");
        assert_eq!(from_digits("30126", &iso, mdy_opts).unwrap().date, d.date);
        assert_eq!(from_digits("20260301", &iso, 0).unwrap().date, d.date);
    }
}

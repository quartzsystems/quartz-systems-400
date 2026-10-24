// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG dates, times and timestamps. A value lives in storage as its
//! external form (`03/05/26`, `14.30.05`, `2026-03-05-14.30.05.123456`)
//! and every operation goes through here: a value is parsed from its
//! storage, character or numeric form under a format word, worked on as
//! days, seconds and picoseconds, and written back in the target's form.
//!
//! The format word: the format code in bits 0-7, the separator (ASCII,
//! 0 for none) in bits 8-15, a timestamp's fractional digits in bits
//! 16-23 (`docs/RPG.md`, dates).

use crate::value::{Desc, Kind, bytes, read_num};
use qdecimal::{Decimal, Rounding};

/// Date format codes.
pub mod code {
    /// `*ISO`.
    pub const ISO: u8 = 1;
    /// `*USA`.
    pub const USA: u8 = 2;
    /// `*EUR`.
    pub const EUR: u8 = 3;
    /// `*JIS`.
    pub const JIS: u8 = 4;
    /// `*MDY`.
    pub const MDY: u8 = 5;
    /// `*DMY`.
    pub const DMY: u8 = 6;
    /// `*YMD`.
    pub const YMD: u8 = 7;
    /// `*JUL`.
    pub const JUL: u8 = 8;
    /// `*CYMD`.
    pub const CYMD: u8 = 9;
    /// `*CMDY`.
    pub const CMDY: u8 = 10;
    /// `*CDMY`.
    pub const CDMY: u8 = 11;
    /// `*LONGJUL`.
    pub const LONGJUL: u8 = 12;
    /// Time `*HMS`.
    pub const HMS: u8 = 32;
    /// Time `*ISO`.
    pub const TISO: u8 = 33;
    /// Time `*USA`.
    pub const TUSA: u8 = 34;
    /// Time `*EUR`.
    pub const TEUR: u8 = 35;
    /// Time `*JIS`.
    pub const TJIS: u8 = 36;
    /// Timestamp `*ISO`.
    pub const STAMP: u8 = 64;
}

/// Duration units, as the compiler numbers them.
pub mod unit {
    /// `*YEARS`.
    pub const YEARS: i32 = 0;
    /// `*MONTHS`.
    pub const MONTHS: i32 = 1;
    /// `*DAYS`.
    pub const DAYS: i32 = 2;
    /// `*HOURS`.
    pub const HOURS: i32 = 3;
    /// `*MINUTES`.
    pub const MINUTES: i32 = 4;
    /// `*SECONDS`.
    pub const SECONDS: i32 = 5;
    /// `*MSECONDS`.
    pub const MSECONDS: i32 = 6;
}

/// Why an operation failed, as the RPG status it maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DtErr {
    /// 00112: the value is not a valid date, time or timestamp.
    Invalid,
    /// 00113: arithmetic left the range of dates.
    Overflow,
    /// 00114: the value does not fit the target's format.
    Mapping,
}

impl DtErr {
    /// The message identifier the runtime signals.
    #[must_use]
    pub fn msgid(self) -> &'static str {
        match self {
            Self::Invalid => "RNX0112",
            Self::Overflow => "RNX0113",
            Self::Mapping => "RNX0114",
        }
    }
}

/// A decoded format word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fmt {
    /// The format code.
    pub code: u8,
    /// The separator; `None` for none.
    pub sep: Option<u8>,
    /// Fractional digits of a timestamp.
    pub frac: u8,
}

impl Fmt {
    /// From the word.
    #[must_use]
    pub fn of(word: i64) -> Self {
        let w = word.cast_unsigned();
        let sep = u8::try_from((w >> 8) & 0xFF).unwrap_or(0);
        Self {
            code: u8::try_from(w & 0xFF).unwrap_or(0),
            sep: (sep != 0).then_some(sep),
            frac: u8::try_from((w >> 16) & 0xFF).unwrap_or(0),
        }
    }

    /// Whether it is a time format.
    #[must_use]
    pub fn is_time(&self) -> bool {
        (code::HMS..=code::TJIS).contains(&self.code)
    }

    /// Whether it is the timestamp format.
    #[must_use]
    pub fn is_stamp(&self) -> bool {
        self.code == code::STAMP
    }

    /// Years a date format can hold.
    #[must_use]
    pub fn years(&self) -> (i64, i64) {
        match self.code {
            code::MDY | code::DMY | code::YMD | code::JUL => (1940, 2039),
            code::CYMD | code::CMDY | code::CDMY => (1900, 2899),
            _ => (1, 9999),
        }
    }
}

/// What a value holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A date.
    Date,
    /// A time.
    Time,
    /// A timestamp.
    Stamp,
}

/// A date, time or timestamp: days since 0001-01-01, seconds of the
/// day, picoseconds of the second.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dt {
    /// Days since 0001-01-01.
    pub days: i64,
    /// Seconds since midnight.
    pub secs: i64,
    /// Picoseconds.
    pub pico: i64,
}

const PICO: i64 = 1_000_000_000_000;
const DAY_SECS: i64 = 86_400;
/// Days from 0001-01-01 to 9999-12-31.
const MAX_DAYS: i64 = 3_652_058;

/// Days since 0001-01-01 of a civil date.
#[must_use]
pub fn days_of(y: i64, m: i64, d: i64) -> i64 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 306
}

/// The civil date of a day count since 0001-01-01.
#[must_use]
pub fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 306;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn leap(y: i64) -> bool {
    y.rem_euclid(4) == 0 && (y.rem_euclid(100) != 0 || y.rem_euclid(400) == 0)
}

/// Days in a month.
#[must_use]
pub fn month_days(y: i64, m: i64) -> i64 {
    match m {
        2 if leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The days of a valid date.
fn ymd(y: i64, m: i64, d: i64) -> Option<i64> {
    ((1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= month_days(y, m)).then(|| days_of(y, m, d))
}

/// One field of a layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum F {
    Y4,
    Y2,
    C1,
    M,
    D,
    Ddd,
}

fn date_fields(code: u8) -> &'static [F] {
    match code {
        code::USA => &[F::M, F::D, F::Y4],
        code::EUR => &[F::D, F::M, F::Y4],
        code::MDY => &[F::M, F::D, F::Y2],
        code::DMY => &[F::D, F::M, F::Y2],
        code::YMD => &[F::Y2, F::M, F::D],
        code::JUL => &[F::Y2, F::Ddd],
        code::CYMD => &[F::C1, F::Y2, F::M, F::D],
        code::CMDY => &[F::C1, F::M, F::D, F::Y2],
        code::CDMY => &[F::C1, F::D, F::M, F::Y2],
        code::LONGJUL => &[F::Y4, F::Ddd],
        _ => &[F::Y4, F::M, F::D],
    }
}

fn width(f: F) -> usize {
    match f {
        F::Y4 => 4,
        F::C1 => 1,
        F::Ddd => 3,
        _ => 2,
    }
}

/// The separator a century format puts after its century digit: none.
fn sep_after(code: u8, i: usize) -> bool {
    !(matches!(code, code::CYMD | code::CMDY | code::CDMY) && i == 0)
}

/// The characters of a date in a format.
#[must_use]
pub fn date_text(days: i64, f: Fmt) -> String {
    let (y, m, d) = civil(days);
    let fields = date_fields(f.code);
    let mut s = String::new();
    for (i, fl) in fields.iter().enumerate() {
        if i > 0
            && sep_after(f.code, i - 1)
            && let Some(c) = f.sep
        {
            s.push(char::from(c));
        }
        let part = match fl {
            F::Y4 => format!("{y:04}"),
            F::Y2 => format!("{:02}", y % 100),
            F::C1 => format!("{}", (y - 1900) / 100),
            F::M => format!("{m:02}"),
            F::D => format!("{d:02}"),
            F::Ddd => format!("{:03}", days - days_of(y, 1, 1) + 1),
        };
        s.push_str(&part);
    }
    s
}

fn digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// A date from its characters in a format.
#[must_use]
pub fn parse_date(s: &str, f: Fmt) -> Option<i64> {
    let b = s.as_bytes();
    let fields = date_fields(f.code);
    let mut at = 0;
    let (mut y, mut c, mut m, mut d, mut ddd) = (None, None, 1, 1, None);
    let mut two = None;
    for (i, fl) in fields.iter().enumerate() {
        // The separator must be the format's own.
        if i > 0 && sep_after(f.code, i - 1) && f.sep.is_some() {
            if b.get(at).copied() != f.sep {
                return None;
            }
            at += 1;
        }
        let w = width(*fl);
        let v = digits(s.get(at..at + w)?)?;
        at += w;
        match fl {
            F::Y4 => y = Some(v),
            F::Y2 => two = Some(v),
            F::C1 => c = Some(v),
            F::M => m = v,
            F::D => d = v,
            F::Ddd => ddd = Some(v),
        }
    }
    if at != b.len() {
        return None;
    }
    let year = match (y, two, c) {
        (Some(y), _, _) => y,
        (None, Some(t), Some(c)) => 1900 + c * 100 + t,
        (None, Some(t), None) => {
            if t >= 40 {
                1900 + t
            } else {
                2000 + t
            }
        }
        _ => return None,
    };
    match ddd {
        Some(n) => {
            let len = if leap(year) { 366 } else { 365 };
            ((1..=9999).contains(&year) && (1..=len).contains(&n)).then(|| days_of(year, 1, 1) + n - 1)
        }
        None => ymd(year, m, d),
    }
}

/// The characters of a time in a format.
#[must_use]
pub fn time_text(secs: i64, f: Fmt) -> String {
    let (h, mi, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if f.code == code::TUSA {
        let (h12, ap) = match h {
            0 | 24 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        return match f.sep {
            Some(c) => format!("{h12:02}{}{mi:02} {ap}", char::from(c)),
            None => format!("{h12:02}{mi:02}{ap}"),
        };
    }
    match f.sep {
        Some(c) => {
            let c = char::from(c);
            format!("{h:02}{c}{mi:02}{c}{s:02}")
        }
        None => format!("{h:02}{mi:02}{s:02}"),
    }
}

/// A time from its characters in a format.
#[must_use]
pub fn parse_time(s: &str, f: Fmt) -> Option<i64> {
    let b = s.as_bytes();
    let field = |at: usize| digits(s.get(at..at + 2)?);
    let check_sep = |at: usize| b.get(at).copied() == f.sep;
    if f.code == code::TUSA {
        let (h, mi, rest) = if f.sep.is_some() {
            if !check_sep(2) || b.get(5) != Some(&b' ') {
                return None;
            }
            (field(0)?, field(3)?, s.get(6..)?)
        } else {
            (field(0)?, field(2)?, s.get(4..)?)
        };
        if h > 12 || mi > 59 {
            return None;
        }
        let h = match rest {
            "AM" => h % 12,
            "PM" => h % 12 + 12,
            _ => return None,
        };
        return Some(h * 3600 + mi * 60);
    }
    let (h, mi, sec, len) = if f.sep.is_some() {
        if !check_sep(2) || !check_sep(5) {
            return None;
        }
        (field(0)?, field(3)?, field(6)?, 8)
    } else {
        (field(0)?, field(2)?, field(4)?, 6)
    };
    if b.len() != len {
        return None;
    }
    if h == 24 && mi == 0 && sec == 0 {
        return Some(DAY_SECS);
    }
    (h < 24 && mi < 60 && sec < 60).then_some(h * 3600 + mi * 60 + sec)
}

/// The characters of a timestamp with `frac` fractional digits.
#[must_use]
pub fn stamp_text(v: Dt, frac: u8, sep: bool) -> String {
    let iso = Fmt {
        code: code::ISO,
        sep: sep.then_some(b'-'),
        frac: 0,
    };
    let tf = Fmt {
        code: code::TISO,
        sep: sep.then_some(b'.'),
        frac: 0,
    };
    let mut s = date_text(v.days, iso);
    if sep {
        s.push('-');
    }
    s.push_str(&time_text(v.secs, tf));
    if frac > 0 {
        if sep {
            s.push('.');
        }
        let f = format!("{:012}", v.pico);
        s.push_str(&f[..usize::from(frac.min(12))]);
    }
    s
}

/// A timestamp from its characters: `yyyy-mm-dd-hh.mm.ss[.f…]` or the
/// digits alone.
#[must_use]
pub fn parse_stamp(s: &str, sep: bool) -> Option<Dt> {
    let (d, t, f) = if sep {
        if s.len() < 19 || s.as_bytes()[10] != b'-' {
            return None;
        }
        let f = match s.get(19..)? {
            "" => "",
            r => r.strip_prefix('.')?,
        };
        (&s[..10], s.get(11..19)?, f)
    } else {
        if s.len() < 14 {
            return None;
        }
        (s.get(..8)?, s.get(8..14)?, s.get(14..)?)
    };
    let df = Fmt {
        code: code::ISO,
        sep: sep.then_some(b'-'),
        frac: 0,
    };
    let tf = Fmt {
        code: code::TISO,
        sep: sep.then_some(b'.'),
        frac: 0,
    };
    let days = parse_date(d, df)?;
    let secs = parse_time(t, tf)?;
    if f.len() > 12 || !f.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pico: i64 = if f.is_empty() { 0 } else { format!("{f:0<12}").parse().ok()? };
    // 24.00.00 ends a day only with no fraction.
    if secs == DAY_SECS && pico != 0 {
        return None;
    }
    Some(Dt { days, secs, pico })
}

/// The class a format word describes.
#[must_use]
pub fn class_of(f: Fmt) -> Class {
    if f.is_stamp() {
        Class::Stamp
    } else if f.is_time() {
        Class::Time
    } else {
        Class::Date
    }
}

/// A value from its characters under a format.
#[must_use]
pub fn parse(s: &str, f: Fmt) -> Option<Dt> {
    match class_of(f) {
        Class::Date => {
            let days = parse_date(s, f)?;
            let (lo, hi) = f.years();
            let y = civil(days).0;
            ((lo..=hi).contains(&y)).then_some(Dt { days, secs: 0, pico: 0 })
        }
        Class::Time => Some(Dt {
            days: 0,
            secs: parse_time(s, f)?,
            pico: 0,
        }),
        Class::Stamp => parse_stamp(s, f.sep.is_some()),
    }
}

/// A value's characters under a format; `Mapping` when the year does
/// not fit it.
pub fn render(v: Dt, f: Fmt) -> Result<String, DtErr> {
    Ok(match class_of(f) {
        Class::Date => {
            let (lo, hi) = f.years();
            let y = civil(v.days).0;
            if !(lo..=hi).contains(&y) {
                return Err(DtErr::Mapping);
            }
            date_text(v.days, f)
        }
        Class::Time => time_text(v.secs, f),
        Class::Stamp => stamp_text(v, f.frac, f.sep.is_some()),
    })
}

/// The digits of a format's numeric form.
fn num_digits(f: Fmt) -> usize {
    match class_of(f) {
        Class::Date => date_fields(f.code).iter().map(|x| width(*x)).sum(),
        Class::Time => 6,
        Class::Stamp => 14 + usize::from(f.frac),
    }
}

/// A format without its separators.
#[must_use]
pub fn bare(f: Fmt) -> Fmt {
    Fmt { sep: None, ..f }
}

/// EBCDIC bytes as text, `?` for what has no character.
fn text_of(b: &[u8]) -> String {
    b.iter().map(|x| ccsid::char_of(*x)).collect()
}

/// Read an operand as a value under a format word: date storage in its
/// own format, characters in `f`, numbers as `f`'s digits.
pub fn read(addr: u64, d: &Desc, f: Fmt) -> Result<Dt, DtErr> {
    if d.numeric() {
        let v = read_num(addr, d).map_err(|_| DtErr::Invalid)?;
        let v = v.rescale(0, Rounding::Truncate).map_err(|_| DtErr::Invalid)?;
        if v.is_negative() {
            return Err(DtErr::Invalid);
        }
        let mut s = v.to_string();
        let want = num_digits(f);
        if s.len() > want {
            return Err(DtErr::Invalid);
        }
        s = format!("{s:0>want$}");
        let f = bare(f);
        let f = if f.is_stamp() { Fmt { frac: u8::try_from(want - 14).unwrap_or(0), ..f } } else { f };
        return parse(&s, f).ok_or(DtErr::Invalid);
    }
    let b = bytes(addr, d.size());
    let text = match d.kind {
        Kind::VarChar => text_of(d.var_data(b)),
        _ => text_of(b),
    };
    let text = if matches!(d.kind, Kind::Char | Kind::VarChar) {
        text.trim_end().to_string()
    } else {
        text
    };
    parse(&text, f).ok_or(DtErr::Invalid)
}

/// Write a value into an operand: date storage and characters in `f`,
/// numbers as `f`'s digits.
pub fn write(addr: u64, d: &Desc, f: Fmt, v: Dt) -> Result<(), DtErr> {
    if d.numeric() {
        let s = render(v, bare(f))?;
        let n = Decimal::parse(&s).map_err(|_| DtErr::Invalid)?;
        return write_num(addr, d, &n).map_err(|()| DtErr::Mapping);
    }
    let s = render(v, f)?;
    let e: Vec<u8> = s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect();
    if d.kind == Kind::VarChar {
        let max = usize::try_from(d.len).unwrap_or(0);
        crate::value::write_var(addr, d.prefix(), &e[..e.len().min(max)]);
    } else {
        let out = bytes(addr, d.size());
        out.fill(0x40);
        let n = e.len().min(out.len());
        out[..n].copy_from_slice(&e[..n]);
    }
    Ok(())
}

fn write_num(addr: u64, d: &Desc, v: &Decimal) -> Result<(), ()> {
    let digits = u8::try_from(d.len.clamp(1, 63)).unwrap_or(63);
    let enc = match d.kind {
        Kind::Packed => qdecimal::Packed::new(digits, d.dec).and_then(|p| p.encode(v, Rounding::Truncate)),
        Kind::Zoned => qdecimal::Zoned::new(digits, d.dec).and_then(|z| z.encode(v, Rounding::Truncate)),
        Kind::Bin => qdecimal::Binary::new(digits, d.dec).and_then(|b| b.encode(v, Rounding::Truncate)),
        Kind::Int | Kind::Uns => {
            qdecimal::Integer::new(u8::try_from(d.size()).unwrap_or(8), d.kind == Kind::Int).and_then(|i| i.encode(v))
        }
        _ => return Err(()),
    }
    .map_err(|_| ())?;
    bytes(addr, enc.len()).copy_from_slice(&enc);
    Ok(())
}

/// A value converted between classes: a date becomes midnight of its
/// day, a timestamp gives its date or its time.
#[must_use]
pub fn recast(v: Dt, from: Class, to: Class) -> Dt {
    match (from, to) {
        (Class::Stamp | Class::Time, Class::Date) => Dt { secs: 0, pico: 0, ..v },
        (Class::Stamp, Class::Time) => Dt { days: 0, pico: 0, ..v },
        _ => v,
    }
}

/// Add months, the day kept or cut to the end of the month.
fn add_months(days: i64, n: i64) -> Option<i64> {
    let (y, m, d) = civil(days);
    let total = y * 12 + m - 1 + n;
    let (y2, m2) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    if !(1..=9999).contains(&y2) {
        return None;
    }
    Some(days_of(y2, m2, d.min(month_days(y2, m2))))
}

/// `v + n` units of a value of a class.
pub fn add(v: Dt, class: Class, n: i64, u: i32) -> Result<Dt, DtErr> {
    let over = DtErr::Overflow;
    let out = match (class, u) {
        (Class::Date | Class::Stamp, unit::YEARS) => Dt {
            days: add_months(v.days, n.checked_mul(12).ok_or(over)?).ok_or(over)?,
            ..v
        },
        (Class::Date | Class::Stamp, unit::MONTHS) => Dt {
            days: add_months(v.days, n).ok_or(over)?,
            ..v
        },
        (Class::Date, unit::DAYS) => Dt {
            days: v.days.checked_add(n).ok_or(over)?,
            ..v
        },
        (Class::Time, _) => {
            let per = match u {
                unit::HOURS => 3600,
                unit::MINUTES => 60,
                unit::SECONDS => 1,
                _ => return Err(DtErr::Invalid),
            };
            let secs = i128::from(v.secs) + i128::from(n) * per;
            Dt {
                secs: i64::try_from(secs.rem_euclid(i128::from(DAY_SECS))).unwrap_or(0),
                ..v
            }
        }
        (Class::Stamp, _) => {
            let per: i128 = match u {
                unit::DAYS => i128::from(DAY_SECS) * i128::from(PICO),
                unit::HOURS => 3600 * i128::from(PICO),
                unit::MINUTES => 60 * i128::from(PICO),
                unit::SECONDS => i128::from(PICO),
                unit::MSECONDS => 1_000_000,
                _ => return Err(DtErr::Invalid),
            };
            from_pico(to_pico(v) + i128::from(n) * per).ok_or(over)?
        }
        _ => return Err(DtErr::Invalid),
    };
    if !(0..=MAX_DAYS).contains(&out.days) {
        return Err(over);
    }
    Ok(out)
}

fn to_pico(v: Dt) -> i128 {
    (i128::from(v.days) * i128::from(DAY_SECS) + i128::from(v.secs)) * i128::from(PICO) + i128::from(v.pico)
}

fn from_pico(p: i128) -> Option<Dt> {
    let per_day = i128::from(DAY_SECS) * i128::from(PICO);
    let days = i64::try_from(p.div_euclid(per_day)).ok()?;
    let rest = p.rem_euclid(per_day);
    Some(Dt {
        days,
        secs: i64::try_from(rest / i128::from(PICO)).ok()?,
        pico: i64::try_from(rest % i128::from(PICO)).ok()?,
    })
}

/// Whole months from `b` to `a`, counted as RPG counts them.
fn months_between(a: Dt, b: Dt) -> i64 {
    let (ya, ma, da) = civil(a.days);
    let (yb, mb, db) = civil(b.days);
    let mut n = (ya - yb) * 12 + ma - mb;
    // The rest of the value below the month: day, then time of day.
    let ra = (da, a.secs, a.pico);
    let rb = (db, b.secs, b.pico);
    if n > 0 && ra < rb {
        n -= 1;
    } else if n < 0 && ra > rb {
        n += 1;
    }
    n
}

/// `a - b` in whole units.
pub fn diff(a: Dt, b: Dt, u: i32) -> Result<i64, DtErr> {
    Ok(match u {
        unit::YEARS => months_between(a, b) / 12,
        unit::MONTHS => months_between(a, b),
        _ => {
            let per: i128 = match u {
                unit::DAYS => i128::from(DAY_SECS) * i128::from(PICO),
                unit::HOURS => 3600 * i128::from(PICO),
                unit::MINUTES => 60 * i128::from(PICO),
                unit::SECONDS => i128::from(PICO),
                unit::MSECONDS => 1_000_000,
                _ => return Err(DtErr::Invalid),
            };
            i64::try_from((to_pico(a) - to_pico(b)) / per).map_err(|_| DtErr::Overflow)?
        }
    })
}

/// `%SUBDT`: one part of a value.
pub fn part(v: Dt, u: i32) -> Result<i64, DtErr> {
    let (y, m, d) = civil(v.days);
    Ok(match u {
        unit::YEARS => y,
        unit::MONTHS => m,
        unit::DAYS => d,
        unit::HOURS => v.secs / 3600,
        unit::MINUTES => v.secs / 60 % 60,
        unit::SECONDS => v.secs % 60,
        unit::MSECONDS => v.pico / 1_000_000,
        _ => return Err(DtErr::Invalid),
    })
}

/// The current date and time (UTC).
#[must_use]
pub fn now() -> Dt {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = i64::try_from(since.as_secs()).unwrap_or(0);
    Dt {
        days: days_of(1970, 1, 1) + secs.div_euclid(DAY_SECS),
        secs: secs.rem_euclid(DAY_SECS),
        pico: i64::from(since.subsec_micros()) * 1_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(c: u8, sep: Option<u8>) -> Fmt {
        Fmt { code: c, sep, frac: 0 }
    }

    #[test]
    fn calendar_round_trips() {
        assert_eq!(days_of(1, 1, 1), 0);
        assert_eq!(civil(0), (1, 1, 1));
        assert_eq!(days_of(9999, 12, 31), MAX_DAYS);
        for d in [0, 1, 59, 365, 730_000, MAX_DAYS] {
            let (y, m, dd) = civil(d);
            assert_eq!(days_of(y, m, dd), d);
        }
    }

    #[test]
    fn formats_render_and_parse() {
        let d = days_of(2026, 3, 5);
        assert_eq!(date_text(d, f(code::MDY, Some(b'/'))), "03/05/26");
        assert_eq!(date_text(d, f(code::CYMD, Some(b'/'))), "126/03/05");
        assert_eq!(date_text(d, f(code::JUL, None)), "26064");
        assert_eq!(date_text(d, f(code::LONGJUL, Some(b'/'))), "2026/064");
        assert_eq!(parse_date("126/03/05", f(code::CYMD, Some(b'/'))), Some(d));
        assert_eq!(parse_date("26064", f(code::JUL, None)), Some(d));
        assert_eq!(parse_date("2026-02-30", f(code::ISO, Some(b'-'))), None);
        assert_eq!(time_text(14 * 3600 + 30 * 60 + 5, f(code::TUSA, Some(b':'))), "02:30 PM");
        assert_eq!(parse_time("12:15 AM", f(code::TUSA, Some(b':'))), Some(15 * 60));
    }

    #[test]
    fn arithmetic_follows_rpg() {
        let jan31 = Dt { days: days_of(2024, 1, 31), secs: 0, pico: 0 };
        let feb = add(jan31, Class::Date, 1, unit::MONTHS).unwrap();
        assert_eq!(civil(feb.days), (2024, 2, 29));
        let s = parse_stamp("2026-03-05-23.59.59.999999", true).unwrap();
        let t = add(s, Class::Stamp, 1, unit::MSECONDS).unwrap();
        assert_eq!(stamp_text(t, 6, true), "2026-03-06-00.00.00.000000");
        assert_eq!(diff(t, s, unit::MSECONDS), Ok(1));
    }
}

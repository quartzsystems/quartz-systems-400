// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job and system APIs: `QUSRJOBI`, `QUSLJOB`, `QWCRSVAL`, `QWCCVTDT`,
//! `QWCRTVCA`, `QWCRTVTZ`, `QLGCNVCS` (`QSZRTVPR` is `lp`'s).

use mi::Job;
use sysval::{civil_from_days, days_from_civil};

use crate::param::{Args, ErrCode, Out};
use crate::provider::{ApiFail, Provider, SysValKind};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QUSRJOBI", qusrjobi),
    ("QUSLJOB", qusljob),
    ("QWCRSVAL", qwcrsval),
    ("QWCRSSTS", qwcrssts),
    ("QWCCVTDT", qwccvtdt),
    ("QWCRTVCA", qwcrtvca),
    ("QWCRTVTZ", qwcrtvtz),
    ("QLGCNVCS", qlgcnvcs),
    ("QWCRNETA", qwcrneta),
];

/// Civil date and time of Unix seconds: `(year, month, day, hour,
/// minute, second)`.
#[must_use]
pub fn civil(unix: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = i64::try_from(unix / 86_400).unwrap_or(0);
    let (y, m, d) = civil_from_days(days);
    let s = unix % 86_400;
    (
        y,
        m,
        d,
        u32::try_from(s / 3600).unwrap_or(0),
        u32::try_from(s % 3600 / 60).unwrap_or(0),
        u32::try_from(s % 60).unwrap_or(0),
    )
}

/// `CYYMMDDHHMMSS` of Unix seconds.
#[must_use]
pub fn timestamp13(unix: u64) -> String {
    let (y, m, d, hh, mm, ss) = civil(unix);
    let c = i32::from(y >= 2000);
    format!(
        "{c}{:02}{m:02}{d:02}{hh:02}{mm:02}{ss:02}",
        y.rem_euclid(100)
    )
}

/// The job name parameter's target: this job, or none.
fn is_this_job(j: &Job, qual: &[u8]) -> bool {
    let name = ccsid::decode_trimmed(qual.get(..10).unwrap_or(&[]));
    let user = ccsid::decode_trimmed(qual.get(10..20).unwrap_or(&[]));
    let number = ccsid::decode_trimmed(qual.get(20..26).unwrap_or(&[]));
    name == "*"
        || name.is_empty()
        || (name == j.ident.name
            && (user.is_empty() || user == "*" || user == j.ident.user)
            && (number.is_empty() || number == "*" || number == j.ident.number))
}

fn job_head(j: &Job, out: &mut Out) {
    out.char(8, 10, &j.ident.name);
    out.char(18, 10, &j.ident.user);
    out.char(28, 6, &j.ident.number);
    out.bytes(34, &crate::net::internal_job_id(j));
    out.char(50, 10, "*ACTIVE");
    out.ch(60, 'B');
    out.ch(61, ' ');
    out.blank(62, 2);
}

/// `QUSRJOBI(receiver, len, format, job, internal id, errc)`:
/// `JOBI0100` and `JOBI0700`.
fn qusrjobi(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(2, 8);
    let qual = a.bytes(3, 26);
    if !is_this_job(j, &qual) {
        errc.fail(
            j,
            &ApiFail::id("CPF3C53")
                .text(&ccsid::decode_trimmed(&qual[..10]), 10)
                .text(&ccsid::decode_trimmed(&qual[10..20]), 10)
                .text(&ccsid::decode_trimmed(&qual[20..26]), 6),
        );
        return;
    }
    let mut out = Out::new();
    job_head(j, &mut out);
    match format.as_str() {
        "JOBI0100" => {
            out.bin4(64, 50);
            out.bin4(68, 2000);
            out.bin4(72, 30);
            out.char(76, 10, "*NO");
        }
        "JOBI0700" => {
            let sys: Vec<&String> = j
                .ident
                .libl
                .iter()
                .filter(|l| l.as_str() == "QSYS")
                .collect();
            let usr: Vec<&String> = j
                .ident
                .libl
                .iter()
                .filter(|l| l.as_str() != "QSYS" && Some(l.as_str()) != j.ident.curlib.as_deref())
                .collect();
            out.bin4(62, i32::try_from(sys.len()).unwrap_or(0));
            out.bin4(66, 0);
            out.bin4(70, i32::from(j.ident.curlib.is_some()));
            out.bin4(74, i32::try_from(usr.len()).unwrap_or(0));
            let mut at = 78;
            for l in sys.into_iter().chain(j.ident.curlib.iter()).chain(usr) {
                out.char(at, 11, l);
                at += 11;
            }
        }
        _ => {
            errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
            return;
        }
    }
    let _ = p;
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QUSLJOB(user space, format, job, status, errc)`: `JOBL0100`, the
/// one job.
fn qusljob(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(1, 8);
    if format != "JOBL0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let qual = a.bytes(2, 26);
    let status = a.char(3, 10);
    let mut entries = Vec::new();
    let name = ccsid::decode_trimmed(&qual[..10]);
    let wanted = name == "*ALL" || name == "*CURRENT" || is_this_job(j, &qual);
    if wanted && (status == "*ALL" || status == "*ACTIVE" || status.is_empty()) {
        let mut e = Out::new();
        e.char(0, 10, &j.ident.name);
        e.char(10, 10, &j.ident.user);
        e.char(20, 6, &j.ident.number);
        e.blank(26, 16);
        e.char(42, 10, "*ACTIVE");
        e.ch(52, 'B');
        e.ch(53, ' ');
        entries.push(e.buf);
    }
    let input = [
        a.bytes(0, 20),
        crate::param::fixed(&format, 8),
        qual,
        crate::param::fixed(&status, 10),
    ]
    .concat();
    match crate::obj::write_list(j, &a, 0, &format, "QUSLJOB", &input, &[], &entries, 54) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// The receiver image of named values: a count, offsets, then records
/// of name, type, status, length and data.
fn values_image(
    names: &[String],
    mut get: impl FnMut(&str) -> Option<crate::provider::SysVal>,
) -> Out {
    let mut out = Out::new();
    out.bin4(0, i32::try_from(names.len()).unwrap_or(0));
    let mut at = 4 + 4 * names.len();
    for (i, n) in names.iter().enumerate() {
        out.bin4(4 + 4 * i, i32::try_from(at).unwrap_or(0));
        out.char(at, 10, n);
        match get(n) {
            Some(v) => {
                out.ch(at + 10, if v.kind == SysValKind::Char { 'C' } else { 'B' });
                out.ch(at + 11, ' ');
                out.bin4(at + 12, i32::try_from(v.data.len()).unwrap_or(0));
                out.bytes(at + 16, &v.data);
                at += 16 + v.data.len();
            }
            None => {
                out.ch(at + 10, 'C');
                out.ch(at + 11, 'E');
                out.bin4(at + 12, 0);
                at += 16;
            }
        }
    }
    out
}

/// `QWCRSVAL(receiver, len, count, names, errc)`.
fn qwcrsval(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let n = usize::try_from(a.bin4(2)).unwrap_or(0);
    let names: Vec<String> = a
        .bytes(3, 10 * n)
        .chunks(10)
        .map(ccsid::decode_trimmed)
        .collect();
    if let Some(bad) = names.iter().find(|n| p.sysval(n).is_none()) {
        errc.fail(j, &ApiFail::id("CPF1853").text(bad, 10));
        return;
    }
    let out = values_image(&names, |n| p.sysval(n));
    let room = usize::try_from(a.bin4(1)).unwrap_or(0);
    out.deliver_raw(&a, 0, room);
    errc.ok();
}

/// `QWCRSSTS(receiver, len, format, reset, errc)`.
fn qwcrssts(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    let reset = a.char(3, 10).trim() == "*YES";
    match p.system_status(&format, reset) {
        Err(f) => errc.fail(j, &f),
        Ok(mut buf) => {
            let room = usize::try_from(a.bin4(1)).unwrap_or(0);
            let n = room.min(buf.len());
            if n >= 8 {
                let given = u32::try_from(n).unwrap_or(0).to_be_bytes();
                buf[4..8].copy_from_slice(&given);
            }
            a.put_bytes(0, &buf[..n]);
            errc.ok();
        }
    }
}

/// `QWCRNETA(receiver, len, count, names, errc)`.
fn qwcrneta(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let n = usize::try_from(a.bin4(2)).unwrap_or(0);
    let names: Vec<String> = a
        .bytes(3, 10 * n)
        .chunks(10)
        .map(ccsid::decode_trimmed)
        .collect();
    if let Some(bad) = names.iter().find(|n| p.netattr(n).is_none()) {
        errc.fail(j, &ApiFail::id("CPF1848").text(bad, 10));
        return;
    }
    let out = values_image(&names, |n| p.netattr(n));
    let room = usize::try_from(a.bin4(1)).unwrap_or(0);
    out.deliver_raw(&a, 0, room);
    errc.ok();
}

/// A broken-down time for the date conversions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Civil {
    /// Full year.
    pub year: i64,
    /// Month.
    pub month: u32,
    /// Day.
    pub day: u32,
    /// Hour.
    pub hour: u32,
    /// Minute.
    pub min: u32,
    /// Second.
    pub sec: u32,
    /// Milliseconds.
    pub ms: u32,
}

impl Civil {
    /// Of Unix seconds.
    #[must_use]
    pub fn of_unix(unix: u64) -> Self {
        let (year, month, day, hour, min, sec) = civil(unix);
        Self {
            year,
            month,
            day,
            hour,
            min,
            sec,
            ms: 0,
        }
    }

    /// Unix seconds.
    #[must_use]
    pub fn unix(self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.min) * 60
            + i64::from(self.sec)
    }

    /// Day of the year, 1-based.
    #[must_use]
    pub fn yday(self) -> u32 {
        let jan1 = days_from_civil(self.year, 1, 1);
        u32::try_from(days_from_civil(self.year, self.month, self.day) - jan1 + 1).unwrap_or(1)
    }

    /// The first day of a year plus a day number.
    #[must_use]
    pub fn of_yday(year: i64, yday: u32) -> Self {
        let days = days_from_civil(year, 1, 1) + i64::from(yday) - 1;
        let (y, m, d) = civil_from_days(days);
        Self {
            year: y,
            month: m,
            day: d,
            ..Self::default()
        }
    }
}

/// The MI time-of-day stamp (`*DTS`): microseconds since 1928-08-23
/// 12:03:06.314752 UTC, shifted 12 bits.
const DTS_EPOCH_MICROS: i128 = -1_306_209_413_685_248;

/// `*DTS` of a time.
#[must_use]
pub fn to_dts(c: Civil) -> [u8; 8] {
    let micros = i128::from(c.unix()) * 1_000_000 + i128::from(c.ms) * 1000 - DTS_EPOCH_MICROS;
    let v = u64::try_from((micros << 12) & 0xFFFF_FFFF_FFFF_FFFF).unwrap_or(0);
    v.to_be_bytes()
}

/// The time of a `*DTS`.
#[must_use]
pub fn of_dts(b: &[u8]) -> Civil {
    let v = u64::from_be_bytes(b.get(..8).and_then(|x| x.try_into().ok()).unwrap_or([0; 8]));
    let micros = i128::from(v >> 12) + DTS_EPOCH_MICROS;
    let secs = micros.div_euclid(1_000_000);
    let mut c = Civil::of_unix(u64::try_from(secs).unwrap_or(0));
    c.ms = u32::try_from(micros.rem_euclid(1_000_000) / 1000).unwrap_or(0);
    c
}

fn two(s: &str, at: usize) -> u32 {
    s.get(at..at + 2).and_then(|x| x.parse().ok()).unwrap_or(0)
}

fn year_cyy(s: &str, at: usize) -> i64 {
    let c: i64 = s.get(at..at + 1).and_then(|x| x.parse().ok()).unwrap_or(0);
    1900 + c * 100 + i64::from(two(s, at + 1))
}

/// Parse a `QWCCVTDT` character format.
#[must_use]
pub fn parse_format(fmt: &str, s: &str) -> Option<Civil> {
    let s = s.trim_end();
    let mut c = Civil::default();
    let rest = match fmt {
        "*YYMD" => {
            c.year = s.get(0..4)?.parse().ok()?;
            c.month = two(s, 4);
            c.day = two(s, 6);
            8
        }
        "*YMD" => {
            c.year = year_cyy(s, 0);
            c.month = two(s, 3);
            c.day = two(s, 5);
            7
        }
        "*MDYY" => {
            c.month = two(s, 0);
            c.day = two(s, 2);
            c.year = s.get(4..8)?.parse().ok()?;
            8
        }
        "*MDY" => {
            let cen: i64 = s.get(0..1)?.parse().ok()?;
            c.month = two(s, 1);
            c.day = two(s, 3);
            c.year = 1900 + cen * 100 + i64::from(two(s, 5));
            7
        }
        "*DMYY" => {
            c.day = two(s, 0);
            c.month = two(s, 2);
            c.year = s.get(4..8)?.parse().ok()?;
            8
        }
        "*DMY" => {
            let cen: i64 = s.get(0..1)?.parse().ok()?;
            c.day = two(s, 1);
            c.month = two(s, 3);
            c.year = 1900 + cen * 100 + i64::from(two(s, 5));
            7
        }
        "*JUL" => {
            let y = year_cyy(s, 0);
            let yd: u32 = s.get(3..6)?.parse().ok()?;
            c = Civil::of_yday(y, yd);
            6
        }
        "*LONGJUL" => {
            let y: i64 = s.get(0..4)?.parse().ok()?;
            let yd: u32 = s.get(4..7)?.parse().ok()?;
            c = Civil::of_yday(y, yd);
            7
        }
        _ => return None,
    };
    if s.len() >= rest + 6 {
        c.hour = two(s, rest);
        c.min = two(s, rest + 2);
        c.sec = two(s, rest + 4);
    }
    if s.len() >= rest + 9 {
        c.ms = s
            .get(rest + 6..rest + 9)
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
    }
    (1..=12).contains(&c.month).then_some(c)
}

/// Render a `QWCCVTDT` character format.
#[must_use]
pub fn render_format(fmt: &str, c: Civil) -> Option<String> {
    let yy = c.year.rem_euclid(100);
    let cen = (c.year - 1900).div_euclid(100);
    let time = format!("{:02}{:02}{:02}{:03}", c.hour, c.min, c.sec, c.ms);
    Some(match fmt {
        "*YYMD" => format!("{:04}{:02}{:02}{time}", c.year, c.month, c.day),
        "*YMD" => format!("{cen}{yy:02}{:02}{:02}{time}", c.month, c.day),
        "*MDYY" => format!("{:02}{:02}{:04}{time}", c.month, c.day, c.year),
        "*MDY" => format!("{cen}{:02}{:02}{yy:02}{time}", c.month, c.day),
        "*DMYY" => format!("{:02}{:02}{:04}{time}", c.day, c.month, c.year),
        "*DMY" => format!("{cen}{:02}{:02}{yy:02}{time}", c.day, c.month),
        "*JUL" => format!("{cen}{yy:02}{:03}{time}", c.yday()),
        "*LONGJUL" => format!("{:04}{:03}{time}", c.year, c.yday()),
        _ => return None,
    })
}

/// `QWCCVTDT(input format, input, output format, output, errc)`.
fn qwccvtdt(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let in_fmt = a.char(0, 10);
    let out_fmt = a.char(2, 10);
    let now = p.now();
    let input = match in_fmt.as_str() {
        "*CURRENT" | "*SYSVAL" | "*JOB" => {
            let local = i64::try_from(now.unix).unwrap_or(0) + i64::from(now.offset_min) * 60;
            Some(Civil::of_unix(u64::try_from(local).unwrap_or(0)))
        }
        "*DTS" => Some(of_dts(&a.bytes(1, 8))),
        f => {
            let n = match f {
                "*YYMD" | "*MDYY" | "*DMYY" => 17,
                "*LONGJUL" => 16,
                "*JUL" => 15,
                _ => 16,
            };
            parse_format(f, &a.char(1, n))
        }
    };
    let Some(c) = input else {
        errc.fail(j, &ApiFail::id("CPF1846").text(&in_fmt, 10));
        return;
    };
    match out_fmt.as_str() {
        "*DTS" => a.put_bytes(3, &to_dts(c)),
        f => match render_format(f, c) {
            Some(s) => a.put_char(3, s.len(), &s),
            None => {
                errc.fail(j, &ApiFail::id("CPF1846").text(&out_fmt, 10));
                return;
            }
        },
    }
    errc.ok();
}

/// `QWCRTVCA(receiver, len, format, errc)`: `RTVC0100`, the current
/// attributes: job name, user, number and the current library.
fn qwcrtvca(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let format = a.char(2, 8);
    if format != "RTVC0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let mut out = Out::new();
    out.char(8, 10, &j.ident.name);
    out.char(18, 10, &j.ident.user);
    out.char(28, 6, &j.ident.number);
    out.char(34, 10, j.ident.curlib.as_deref().unwrap_or("*NONE"));
    out.char(44, 10, &j.ident.user);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QWCRTVTZ(receiver, len, format, time zone, errc)`: `RTMZ0100`.
fn qwcrtvtz(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "RTMZ0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let now = p.now();
    let wanted = a.char(3, 10);
    let zone = if wanted == "*CURRENT" || wanted.is_empty() {
        now.zone.clone()
    } else {
        wanted
    };
    let Some(z) = sysval::tz::find(&zone) else {
        errc.fail(j, &ApiFail::id("CPF1844").text(&zone, 10));
        return;
    };
    let mut out = Out::new();
    out.bin4(8, now.offset_min);
    out.ch(12, if now.dst { '1' } else { '0' });
    out.char(13, 10, z.name);
    out.char(23, 10, &z.name[2..].chars().skip(4).collect::<String>());
    out.char(33, 50, z.text);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QLGCNVCS(request, input, output, length, errc)`: convert case.
/// The request block: type (1), CCSID, direction (0 upper, 1 lower).
fn qlgcnvcs(_j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let req = a.bytes(0, 22);
    let to_lower = crate::param::bin4_at(&req, 8) == 1;
    let n = usize::try_from(a.bin4(3)).unwrap_or(0);
    let input = a.bytes(1, n);
    let out: Vec<u8> = input
        .iter()
        .map(|b| {
            let c = ccsid::char_of(*b);
            let c2 = if to_lower {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                c.to_uppercase().next().unwrap_or(c)
            };
            ccsid::byte_of(c2).unwrap_or(*b)
        })
        .collect();
    a.put_bytes(2, &out);
    errc.ok();
}

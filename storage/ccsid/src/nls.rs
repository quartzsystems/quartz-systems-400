// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! National language versions: the feature code of each language
//! library (`QSYS2924`), its language and country identifiers and the
//! CCSID a job with that language and CCSID 65535 works in (its
//! default CCSID).

/// One national language version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language {
    /// The feature code, `2924`; the library is `QSYS` followed by it.
    pub feature: u16,
    /// `LANGID`.
    pub langid: &'static str,
    /// `CNTRYID`.
    pub cntryid: &'static str,
    /// The CCSID of its text.
    pub ccsid: u16,
}

impl Language {
    /// Its library, `QSYS2931`.
    #[must_use]
    pub fn library(&self) -> String {
        format!("QSYS{}", self.feature)
    }
}

/// The versions QS/400 knows, the primary (English) first.
pub const LANGUAGES: &[Language] = &[
    Language {
        feature: 2924,
        langid: "ENU",
        cntryid: "US",
        ccsid: 37,
    },
    Language {
        feature: 2928,
        langid: "FRA",
        cntryid: "FR",
        ccsid: 297,
    },
    Language {
        feature: 2929,
        langid: "DEU",
        cntryid: "DE",
        ccsid: 273,
    },
    Language {
        feature: 2931,
        langid: "ESP",
        cntryid: "ES",
        ccsid: 284,
    },
    Language {
        feature: 2932,
        langid: "ITA",
        cntryid: "IT",
        ccsid: 280,
    },
];

/// The primary language's feature code.
pub const PRIMARY: u16 = 2924;

/// The version of a feature code.
#[must_use]
pub fn by_feature(feature: u16) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.feature == feature)
}

/// The version of a language identifier.
#[must_use]
pub fn by_langid(langid: &str) -> Option<&'static Language> {
    let l = langid.trim();
    LANGUAGES.iter().find(|x| x.langid.eq_ignore_ascii_case(l))
}

/// The feature code of a language library name (`QSYS2931`).
#[must_use]
pub fn feature_of_library(lib: &str) -> Option<u16> {
    let n = lib.trim().strip_prefix("QSYS")?;
    if n.len() != 4 {
        return None;
    }
    n.parse().ok().filter(|f| (2900..3000).contains(f))
}

/// The default CCSID of a job: its CCSID, or with 65535 its language's.
#[must_use]
pub fn default_ccsid(ccsid: u32, langid: &str) -> u32 {
    if ccsid != u32::from(crate::conv::BINARY) {
        return ccsid;
    }
    by_langid(langid).map_or(37, |l| u32::from(l.ccsid))
}

/// Whether a job may run in `ccsid`: 65535, or a single-byte EBCDIC
/// CCSID with a table.
#[must_use]
pub fn job_ccsid_ok(ccsid: u32) -> bool {
    let Ok(c) = u16::try_from(ccsid) else {
        return false;
    };
    c == crate::conv::BINARY
        || matches!(
            crate::conv::Codec::of(c),
            Ok(Some(crate::conv::Codec {
                form: crate::conv::Form::Sbcs { ebcdic: true, .. },
                ..
            }))
        )
}

/// The order of a date's parts (`QDATFMT`, `DATFMT`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFmt {
    /// `*MDY`.
    #[default]
    Mdy,
    /// `*DMY`.
    Dmy,
    /// `*YMD`.
    Ymd,
    /// `*JUL`: year and day of the year.
    Jul,
}

impl DateFmt {
    /// From `*MDY`, `MDY`...; anything else is `*MDY`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s
            .trim()
            .trim_start_matches('*')
            .to_ascii_uppercase()
            .as_str()
        {
            "DMY" => Self::Dmy,
            "YMD" => Self::Ymd,
            "JUL" => Self::Jul,
            _ => Self::Mdy,
        }
    }
}

/// How a job shows dates, times and decimal numbers: the one place
/// every display and report formats them (`QDATFMT`, `QDATSEP`,
/// `QTIMSEP`, `QDECFMT` and the job's `DATFMT`, `DATSEP`, `TIMSEP`,
/// `DECFMT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    /// The order of a date's parts.
    pub datfmt: DateFmt,
    /// Between a date's parts.
    pub datsep: char,
    /// Between a time's parts.
    pub timsep: char,
    /// `DECFMT`: ' ' (a point), `J` or `I` (a comma).
    pub decfmt: char,
}

impl Default for Formats {
    fn default() -> Self {
        Self {
            datfmt: DateFmt::Mdy,
            datsep: '/',
            timsep: ':',
            decfmt: ' ',
        }
    }
}

impl Formats {
    /// From the values as the attributes and system values hold them; a
    /// missing separator is the default's.
    #[must_use]
    pub fn parse(datfmt: &str, datsep: &str, timsep: &str, decfmt: &str) -> Self {
        let sep = |s: &str, d: char| s.trim_matches('\'').chars().next().unwrap_or(d);
        Self {
            datfmt: DateFmt::parse(datfmt),
            datsep: sep(datsep, '/'),
            timsep: sep(timsep, ':'),
            decfmt: match decfmt.trim() {
                "J" => 'J',
                "I" => 'I',
                _ => ' ',
            },
        }
    }

    /// A date's digits in order, a two- or four-digit year.
    #[must_use]
    pub fn date_digits(&self, y: i64, m: u32, d: u32, year4: bool) -> String {
        let year = if year4 {
            format!("{y:04}")
        } else {
            format!("{:02}", y.rem_euclid(100))
        };
        match self.datfmt {
            DateFmt::Mdy => format!("{m:02}{d:02}{year}"),
            DateFmt::Dmy => format!("{d:02}{m:02}{year}"),
            DateFmt::Ymd => format!("{year}{m:02}{d:02}"),
            DateFmt::Jul => format!("{year}{:03}", day_of_year(y, m, d)),
        }
    }

    /// A date with the separators: `11/04/26`, `04.11.26`, `26/308`.
    #[must_use]
    pub fn date(&self, y: i64, m: u32, d: u32, year4: bool) -> String {
        let digits = self.date_digits(y, m, d, year4);
        let yw = if year4 { 4 } else { 2 };
        let parts: Vec<&str> = match self.datfmt {
            DateFmt::Jul => vec![&digits[..yw], &digits[yw..]],
            DateFmt::Ymd => vec![&digits[..yw], &digits[yw..yw + 2], &digits[yw + 2..]],
            _ => vec![&digits[..2], &digits[2..4], &digits[4..]],
        };
        parts.join(&self.datsep.to_string())
    }

    /// A time of day: `14:02:17`, or `14.02.17` with `TIMSEP('.')`.
    #[must_use]
    pub fn time(&self, h: u32, m: u32, s: u32) -> String {
        let t = self.timsep;
        format!("{h:02}{t}{m:02}{t}{s:02}")
    }

    /// The date and time of Unix seconds `unix` at `offset` seconds east
    /// of UTC.
    #[must_use]
    pub fn stamp(&self, unix: i64, offset: i64) -> (String, String) {
        let local = unix + offset;
        let (y, m, d) = civil(local.div_euclid(86_400));
        let s = u32::try_from(local.rem_euclid(86_400)).unwrap_or(0);
        (
            self.date(y, m, d, false),
            self.time(s / 3600, s / 60 % 60, s % 60),
        )
    }

    /// A `CYYMMDD` date (`1261104`, century digit 0 for 19xx); blank
    /// when it is not one.
    #[must_use]
    pub fn cyymmdd(&self, s: &str) -> String {
        let s = s.trim();
        if s.len() != 7 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return String::new();
        }
        let n = |a: usize, b: usize| s[a..b].parse::<u32>().unwrap_or(0);
        let y = 1900 + 100 * i64::from(n(0, 1)) + i64::from(n(1, 3));
        self.date(y, n(3, 5), n(5, 7), false)
    }

    /// An `HHMMSS` time; blank when it is not one.
    #[must_use]
    pub fn hhmmss(&self, s: &str) -> String {
        let s = s.trim();
        if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return String::new();
        }
        let n = |a: usize, b: usize| s[a..b].parse::<u32>().unwrap_or(0);
        self.time(n(0, 2), n(2, 4), n(4, 6))
    }

    /// A `CYYMMDDHHMMSS` stamp as date and time; blanks when short.
    #[must_use]
    pub fn cyymmddhhmmss(&self, s: &str) -> (String, String) {
        match (s.get(..7), s.get(7..13)) {
            (Some(d), Some(t)) => (self.cyymmdd(d), self.hhmmss(t)),
            _ => (String::new(), String::new()),
        }
    }

    /// The decimal point: a comma for `DECFMT(J)` and `DECFMT(I)`.
    #[must_use]
    pub fn point(&self) -> char {
        if matches!(self.decfmt, 'J' | 'I') {
            ','
        } else {
            '.'
        }
    }

    /// A number written with a point (`12.5`), with the job's point.
    #[must_use]
    pub fn number(&self, text: &str) -> String {
        if self.point() == ',' {
            text.replace('.', ",")
        } else {
            text.to_string()
        }
    }
}

/// Days since 1970-01-01 as year, month, day.
#[must_use]
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The day of the year, from 1.
#[must_use]
pub fn day_of_year(y: i64, m: u32, d: u32) -> u32 {
    const CUM: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let i = usize::try_from(m.clamp(1, 12) - 1).unwrap_or(0);
    CUM[i] + d + u32::from(leap && m > 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        // 2026-10-29 14:02:17 UTC.
        let t = 1_793_282_537;
        let us = Formats::default();
        assert_eq!(us.stamp(t, 0), ("10/29/26".into(), "14:02:17".into()));
        let de = Formats::parse("*DMY", ".", ".", "J");
        assert_eq!(de.stamp(t, 3600), ("29.10.26".into(), "15.02.17".into()));
        assert_eq!(
            Formats::parse("*YMD", "-", ":", "").date(2026, 10, 29, true),
            "2026-10-29"
        );
        assert_eq!(
            Formats::parse("*JUL", "/", ":", "").date(2026, 10, 29, false),
            "26/302"
        );
        assert_eq!(
            Formats::parse("*JUL", "/", ":", "").date_digits(2026, 10, 29, true),
            "2026302"
        );
        assert_eq!(de.number("12.5"), "12,5");
        assert_eq!(us.number("12.5"), "12.5");
        assert_eq!(
            Formats::parse("*MDY", "' '", ":", "").date(2026, 1, 2, false),
            "01 02 26"
        );
        assert_eq!(de.cyymmdd("1261104"), "04.11.26");
        assert_eq!(
            us.cyymmddhhmmss("1261104093007"),
            ("11/04/26".into(), "09:30:07".into())
        );
        assert_eq!(us.cyymmdd("12611"), "");
        assert_eq!(de.hhmmss("093007"), "09.30.07");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(day_of_year(2024, 3, 1), 61);
    }

    #[test]
    fn languages_and_their_ccsids() {
        assert_eq!(by_feature(2931).unwrap().library(), "QSYS2931");
        assert_eq!(by_langid("esp").unwrap().ccsid, 284);
        assert_eq!(feature_of_library("QSYS2931"), Some(2931));
        assert_eq!(feature_of_library("QSYS2"), None);
        assert_eq!(default_ccsid(65535, "ESP"), 284);
        assert_eq!(default_ccsid(65535, "XXX"), 37);
        assert_eq!(default_ccsid(273, "ESP"), 273);
        for c in [37, 273, 284, 500, 1141, 65535] {
            assert!(job_ccsid_ok(c), "{c}");
        }
        for c in [1208, 819, 930, 300, 1234, 70000] {
            assert!(!job_ccsid_ok(c), "{c}");
        }
        for l in LANGUAGES {
            assert!(job_ccsid_ok(u32::from(l.ccsid)));
        }
    }
}

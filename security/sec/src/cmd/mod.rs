// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security commands as library functions. Each takes the keywords
//! the caller parsed (`KEYWORD`, value text) and an [`Env`]: the store
//! position, who runs it, the time and the system values. The service
//! console, the Phase 7 command processor and the APIs share them.

pub mod audit;
pub mod usrprf;

use store::obj::ObjectId;

use crate::auth::principal::Principal;
use crate::{Result, SecError};

/// Where and as whom a command runs.
pub struct Env<'a> {
    /// `QSYS`.
    pub qsys: ObjectId,
    /// Who runs it.
    pub actor: &'a Principal,
    /// Unix seconds.
    pub now: u64,
    /// A system value's text.
    pub sysval: &'a dyn Fn(&str) -> String,
}

impl Env<'_> {
    /// A numeric system value.
    #[must_use]
    pub fn num(&self, name: &str, default: u8) -> u8 {
        (self.sysval)(name).trim().parse().unwrap_or(default)
    }
}

/// Keywords and their values, keywords upper case.
#[derive(Debug, Clone, Default)]
pub struct Keywords(pub Vec<(String, String)>);

impl Keywords {
    /// From `(keyword, value)` pairs.
    #[must_use]
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Self(
            pairs
                .iter()
                .map(|(k, v)| (k.to_ascii_uppercase(), (*v).to_string()))
                .collect(),
        )
    }

    /// The value of `kw`, quotes removed.
    #[must_use]
    pub fn get(&self, kw: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(k, _)| k == kw)
            .map(|(_, v)| unquote(v.trim()))
    }

    /// The value of `kw` upper case, unless it is quoted text.
    #[must_use]
    pub fn upper(&self, kw: &str) -> Option<String> {
        self.0.iter().find(|(k, _)| k == kw).map(|(_, v)| {
            let v = v.trim();
            if v.starts_with('\'') {
                unquote(v)
            } else {
                v.to_ascii_uppercase()
            }
        })
    }

    /// The value of `kw` as a list of upper case words.
    #[must_use]
    pub fn list(&self, kw: &str) -> Option<Vec<String>> {
        self.upper(kw)
            .map(|v| v.split_whitespace().map(str::to_string).collect())
    }

    /// Whether `kw` was given.
    #[must_use]
    pub fn has(&self, kw: &str) -> bool {
        self.0.iter().any(|(k, _)| k == kw)
    }
}

/// `'it''s'` as `it's`.
#[must_use]
pub fn unquote(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        v.to_string()
    }
}

/// `LIB/NAME` as `(NAME, LIB)`; no library is `*LIBL`.
#[must_use]
pub fn qualified(v: &str) -> (String, String) {
    match v.split_once('/') {
        Some((l, n)) => (n.to_ascii_uppercase(), l.to_ascii_uppercase()),
        None => (v.to_ascii_uppercase(), "*LIBL".into()),
    }
}

/// `CPD0084`-style refusal of a value.
pub fn bad_value(kw: &str, v: &str) -> SecError {
    SecError::msg("CPD0084", format!("{v} is not a valid value for {kw}."))
}

/// A number in `lo..=hi` for `kw`.
pub fn number(kw: &str, v: &str, lo: i64, hi: i64) -> Result<i64> {
    v.trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (lo..=hi).contains(n))
        .ok_or_else(|| bad_value(kw, v))
}

/// One of `allowed`, upper case.
pub fn one_of(kw: &str, v: &str, allowed: &[&str]) -> Result<String> {
    let u = v.trim().to_ascii_uppercase();
    if allowed.contains(&u.as_str()) {
        Ok(u)
    } else {
        Err(bad_value(kw, v))
    }
}

/// Days since 1970-01-01 of a civil date.
#[must_use]
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a Unix time, `YYYY-MM-DD`.
#[must_use]
pub fn civil(secs: u64) -> String {
    let z = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `YYYY-MM-DD` as Unix seconds at midnight UTC.
pub fn parse_date(kw: &str, v: &str) -> Result<u64> {
    let parts: Vec<&str> = v.trim().split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return Err(bad_value(kw, v));
    };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i64>(), m.parse::<u32>(), d.parse::<u32>()) else {
        return Err(bad_value(kw, v));
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1970 {
        return Err(bad_value(kw, v));
    }
    u64::try_from(days_from_civil(y, m, d) * 86_400).map_err(|_| bad_value(kw, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        let k = Keywords::new(&[("text", "'It''s mine'"), ("SPCAUT", "*jobctl *savsys")]);
        assert_eq!(k.get("TEXT").unwrap(), "It's mine");
        assert_eq!(k.list("SPCAUT").unwrap(), vec!["*JOBCTL", "*SAVSYS"]);
        assert_eq!(qualified("qgpl/x"), ("X".into(), "QGPL".into()));
        assert_eq!(qualified("x").1, "*LIBL");
        assert_eq!(parse_date("D", "2026-10-21").unwrap(), 1_792_540_800);
        assert_eq!(civil(1_792_540_800), "2026-10-21");
        assert!(parse_date("D", "2026-13-01").is_err());
        assert_eq!(number("SEV", "50", 0, 99).unwrap(), 50);
        assert!(one_of("X", "*maybe", &["*YES", "*NO"]).is_err());
    }
}

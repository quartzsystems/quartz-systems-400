// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Operational Assistant's options: what cleanup keeps
//! ([`clnup`]), what each backup saves ([`bckup`]), the backups that ran
//! ([`hist`]) and the command sequences of `GO SAVE` and `GO RESTORE`
//! options 20 to 23 ([`gosave`]). Each is kept as `key=value` text in a
//! user space in `QUSRSYS` (`docs/BACKUP.md`).

pub mod bckup;
pub mod clnup;
pub mod gosave;
pub mod hist;

use std::collections::BTreeMap;

/// Seconds in a day.
pub const DAY: u64 = 86_400;

/// How long something is kept: a number of days, or `*KEEP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// Kept this many days.
    Days(u16),
    /// Never removed.
    Forever,
}

impl Keep {
    /// Parse `1` to `366` or `*KEEP`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("*KEEP") {
            return Some(Self::Forever);
        }
        let n: u16 = s.parse().ok()?;
        (1..=366).contains(&n).then_some(Self::Days(n))
    }

    /// Its keyword form.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Days(n) => n.to_string(),
            Self::Forever => "*KEEP".into(),
        }
    }

    /// Whether something made at `then` (Unix seconds) is past keeping
    /// at `now`.
    #[must_use]
    pub fn expired(self, then: u64, now: u64) -> bool {
        match self {
            Self::Forever => false,
            Self::Days(n) => now.saturating_sub(then) > u64::from(n) * DAY,
        }
    }
}

/// The items of `items` past keeping, by the age `made` gives each.
pub fn select<T>(items: &[T], keep: Keep, now: u64, made: impl Fn(&T) -> u64) -> Vec<&T> {
    items.iter().filter(|i| keep.expired(made(i), now)).collect()
}

/// `key=value` lines, in key order.
fn encode_kv(kv: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (k, v) in kv {
        out.push_str(k);
        out.push('=');
        out.push_str(&v.replace('\n', " "));
        out.push('\n');
    }
    out
}

/// The `key=value` lines of `text`.
fn decode_kv(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .collect()
}

/// A `*YES`/`*NO` value.
fn yes_no(s: &str) -> Option<bool> {
    match s.trim().to_ascii_uppercase().as_str() {
        "*YES" | "Y" | "YES" => Some(true),
        "*NO" | "N" | "NO" => Some(false),
        _ => None,
    }
}

fn yn(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// Whether `s` is `HH:MM:SS` (or `HHMMSS`), a time of day; its seconds.
#[must_use]
pub fn time_of_day(text: &str) -> Option<u32> {
    let digits: String = text.trim().trim_matches('\'').chars().filter(|c| *c != ':').collect();
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = |i: usize| digits[i..i + 2].parse::<u32>().ok();
    let (h, m, sec) = (n(0)?, n(2)?, n(4)?);
    (h < 24 && m < 60 && sec < 60).then_some(h * 3600 + m * 60 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_by_age() {
        let now = 100 * DAY;
        let k = Keep::Days(7);
        assert!(!k.expired(now - 7 * DAY, now), "seven days old is kept");
        assert!(k.expired(now - 7 * DAY - 1, now));
        assert!(!Keep::Forever.expired(0, now));
        assert_eq!(Keep::parse("*keep"), Some(Keep::Forever));
        assert_eq!(Keep::parse("0"), None);
        assert_eq!(Keep::parse("367"), None);
        let ages = [now, now - 3 * DAY, now - 10 * DAY, now - 40 * DAY];
        let old = select(&ages, Keep::Days(7), now, |t| *t);
        assert_eq!(old, vec![&ages[2], &ages[3]]);
    }

    #[test]
    fn times_of_day() {
        assert_eq!(time_of_day("'22:00:00'"), Some(79_200));
        assert_eq!(time_of_day("063000"), Some(23_400));
        assert_eq!(time_of_day("24:00:00"), None);
        assert_eq!(time_of_day("HH:MM:SS"), None);
    }
}

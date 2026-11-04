// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Collection Services configuration `CFGPFRCOL` keeps: the
//! default interval, library and profile, when collections cycle, how
//! long they are kept and what is made from them. It is stored as text,
//! `KEY=VALUE` entries separated by `;`.

use crate::profile::{self, Category};

/// The intervals `CFGPFRCOL INTERVAL` takes, in seconds.
pub const INTERVALS: [u32; 7] = [15, 30, 60, 300, 900, 1800, 3600];

/// The configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Seconds between samples.
    pub interval: u32,
    /// The library of the management collections.
    pub library: String,
    /// The profile `STRPFRCOL COLPRF(*CFG)` uses.
    pub profile: String,
    /// The categories of `*CUSTOM`.
    pub custom: Vec<Category>,
    /// When a new collection starts each day, `hhmmss`.
    pub cycle_time: String,
    /// Hours between cycles, 1 to 24.
    pub cycle_hours: u8,
    /// How long a collection is kept: units and whether they are days;
    /// `None` keeps it for ever (`*PERM`).
    pub retention: Option<(u16, bool)>,
    /// Database files are made as data arrives.
    pub create_files: bool,
    /// A summary is made when a collection ends.
    pub create_summary: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval: 900,
            library: "QPFRDATA".into(),
            profile: "*STANDARDP".into(),
            custom: profile::categories("*STANDARD", &[]).unwrap_or_default(),
            cycle_time: "000000".into(),
            cycle_hours: 24,
            retention: Some((120, false)),
            create_files: true,
            create_summary: false,
        }
    }
}

/// `INTERVAL` as `CFGPFRCOL` shows it: minutes with two decimals.
#[must_use]
pub fn minutes(secs: u32) -> String {
    let hundredths = u64::from(secs) * 100 / 60;
    let whole = hundredths / 100;
    let frac = hundredths % 100;
    if whole == 0 {
        format!(".{frac:02}")
    } else {
        format!("{whole}.{frac:02}")
    }
}

/// Seconds of an `INTERVAL` in minutes (`.25`, `15.0`); only the values
/// the command allows.
#[must_use]
pub fn interval_secs(text: &str) -> Option<u32> {
    let t = text.trim();
    let v: f64 = if t.starts_with('.') {
        format!("0{t}").parse().ok()?
    } else {
        t.parse().ok()?
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked below
    let secs = (v * 60.0).round() as u32;
    (v > 0.0 && INTERVALS.contains(&secs)).then_some(secs)
}

impl Config {
    /// The stored text.
    #[must_use]
    pub fn encode(&self) -> String {
        let ret = match self.retention {
            None => "*PERM".to_string(),
            Some((n, days)) => format!("{n} {}", if days { "*DAYS" } else { "*HOURS" }),
        };
        let custom: Vec<&str> = self.custom.iter().map(|c| c.name()).collect();
        format!(
            "INTERVAL={};LIB={};PROFILE={};CUSTOM={};CYCTIME={};CYCITV={};RETPERIOD={ret};CRTDBF={};CRTPFRSUM={};",
            self.interval,
            self.library,
            self.profile,
            custom.join(" "),
            self.cycle_time,
            self.cycle_hours,
            yes(self.create_files),
            yes(self.create_summary),
        )
    }

    /// The configuration in `text`; entries missing or unreadable keep
    /// their defaults.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // one per entry part
    pub fn decode(text: &str) -> Self {
        let mut c = Self::default();
        for entry in text.split(';') {
            let Some((k, v)) = entry.split_once('=') else {
                continue;
            };
            let v = v.trim();
            match k.trim() {
                "INTERVAL" => c.interval = v.parse().unwrap_or(c.interval),
                "LIB" if !v.is_empty() => c.library = v.to_string(),
                "PROFILE" if profile::PROFILES.contains(&v) => c.profile = v.to_string(),
                "CUSTOM" => c.custom = v.split_whitespace().filter_map(Category::parse).collect(),
                "CYCTIME" if v.len() == 6 => c.cycle_time = v.to_string(),
                "CYCITV" => c.cycle_hours = v.parse().unwrap_or(c.cycle_hours),
                "RETPERIOD" => {
                    c.retention = if v == "*PERM" {
                        None
                    } else {
                        let mut w = v.split_whitespace();
                        let n = w.next().and_then(|n| n.parse().ok()).unwrap_or(120);
                        Some((n, w.next() == Some("*DAYS")))
                    };
                }
                "CRTDBF" => c.create_files = v == "*YES",
                "CRTPFRSUM" => c.create_summary = v == "*YES",
                _ => {}
            }
        }
        c
    }

    /// The keyword form `CFGPFRCOL`'s prompt shows: the current values.
    #[must_use]
    pub fn keyword_form(&self) -> String {
        let ret = match self.retention {
            None => "*PERM *HOURS".to_string(),
            Some((n, days)) => format!("{n:05} {}", if days { "*DAYS" } else { "*HOURS" }),
        };
        format!(
            "INTERVAL({}) LIB({}) DFTCOLPRF({}) CYCTIME('{}') CYCITV({}) RETPERIOD({ret}) CRTDBF({}) CRTPFRSUM({})",
            minutes(self.interval),
            self.library,
            self.profile,
            self.cycle_time,
            self.cycle_hours,
            yes(self.create_files),
            yes(self.create_summary),
        )
    }

    /// How long a collection is kept, seconds; `None` for ever.
    #[must_use]
    pub fn retention_secs(&self) -> Option<u64> {
        self.retention
            .map(|(n, days)| u64::from(n) * if days { 86_400 } else { 3_600 })
    }

    /// The categories a collection with `colprf` gathers.
    #[must_use]
    pub fn categories(&self, colprf: &str) -> Option<Vec<Category>> {
        let p = if colprf.eq_ignore_ascii_case("*CFG") {
            self.profile.as_str()
        } else {
            colprf
        };
        profile::categories(p, &self.custom)
    }
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_forms() {
        let mut c = Config {
            interval: 15,
            retention: None,
            ..Config::default()
        };
        assert_eq!(Config::decode(&c.encode()), c);
        c.retention = Some((7, true));
        assert_eq!(Config::decode(&c.encode()), c);
        assert_eq!(Config::decode(""), Config::default());
        let d = Config::default().keyword_form();
        assert!(d.contains("INTERVAL(15.00)"), "{d}");
        assert!(d.contains("RETPERIOD(00120 *HOURS)"), "{d}");
        assert_eq!(minutes(15), ".25");
        assert_eq!(minutes(30), ".50");
        assert_eq!(interval_secs(".25"), Some(15));
        assert_eq!(interval_secs("5.0"), Some(300));
        assert_eq!(interval_secs("2"), None);
        assert_eq!(Config::default().retention_secs(), Some(120 * 3600));
        assert_eq!(Config::default().categories("*CFG").unwrap().len(), 7);
    }
}

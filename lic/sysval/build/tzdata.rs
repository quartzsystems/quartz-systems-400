// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Just enough of the IANA tz source format (`zic`'s input) to find the
//! daylight saving rule a zone follows in a given year: `Rule`, `Zone`
//! with its continuation lines, and `Link`.

use std::collections::HashMap;

/// One `Rule` line.
#[derive(Clone, Debug)]
struct RuleLine {
    from: i64,
    to: i64,
    month: u32,
    on: On,
    at: At,
    save: i64,
}

/// The `ON` field.
#[derive(Clone, Copy, Debug)]
enum On {
    /// `lastSun`: weekday 0 is Sunday.
    Last(u32),
    /// `Sun>=8`.
    AtLeast(u32, u32),
    /// `Sun<=25`.
    AtMost(u32, u32),
    /// `15`.
    Day(u32),
}

/// The `AT` field: seconds and what clock it is on.
#[derive(Clone, Copy, Debug)]
struct At {
    secs: i64,
    clock: Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clock {
    Wall,
    Standard,
    Utc,
}

/// One line of a zone: its standard offset, rules and end.
#[derive(Clone, Debug)]
struct ZoneLine {
    stdoff: i64,
    rules: String,
    until_year: Option<i64>,
}

/// The parsed sources.
#[derive(Default)]
pub struct TzData {
    rules: HashMap<String, Vec<RuleLine>>,
    zones: HashMap<String, Vec<ZoneLine>>,
    links: HashMap<String, String>,
}

/// One edge of a daylight saving period as a *TIMZON holds it: month,
/// weekday (0 Sunday), week (1 to 4, 5 the last) and local wall time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub month: u32,
    pub weekday: u32,
    pub week: u32,
    pub secs: i64,
}

/// What a zone does in a year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YearRule {
    /// Standard offset in seconds east of UTC.
    pub stdoff: i64,
    /// Start, end and the shift in seconds; `None` without saving.
    pub dst: Option<(Edge, Edge, i64)>,
}

fn hms(s: &str) -> i64 {
    let (neg, s) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let mut parts = s.split(':').map(|p| p.parse::<i64>().unwrap_or(0));
    let h = parts.next().unwrap_or(0);
    let m = parts.next().unwrap_or(0);
    let sec = parts.next().unwrap_or(0);
    let v = h * 3600 + m * 60 + sec;
    if neg { -v } else { v }
}

fn month(s: &str) -> u32 {
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    M.iter()
        .position(|m| s.starts_with(m))
        .map_or_else(|| panic!("tzdata: month {s}"), |i| u32::try_from(i).expect("month") + 1)
}

fn weekday(s: &str) -> u32 {
    const D: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    D.iter()
        .position(|d| s.starts_with(d))
        .map_or_else(|| panic!("tzdata: weekday {s}"), |i| u32::try_from(i).expect("day"))
}

fn on(s: &str) -> On {
    if let Some(d) = s.strip_prefix("last") {
        On::Last(weekday(d))
    } else if let Some((d, n)) = s.split_once(">=") {
        On::AtLeast(weekday(d), n.parse().expect("day"))
    } else if let Some((d, n)) = s.split_once("<=") {
        On::AtMost(weekday(d), n.parse().expect("day"))
    } else {
        On::Day(s.parse().expect("day"))
    }
}

fn at(s: &str) -> At {
    let (body, clock) = match s.chars().last() {
        Some('u' | 'g' | 'z') => (&s[..s.len() - 1], Clock::Utc),
        Some('s') => (&s[..s.len() - 1], Clock::Standard),
        Some('w') => (&s[..s.len() - 1], Clock::Wall),
        _ => (s, Clock::Wall),
    };
    At { secs: hms(body), clock }
}

fn year(s: &str, from: i64) -> i64 {
    match s {
        "only" => from,
        "max" => i64::MAX,
        y => y.parse().expect("year"),
    }
}

impl TzData {
    /// Parse one source file into the set.
    pub fn add(&mut self, text: &str) {
        let mut zone: Option<String> = None;
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("");
            if line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            let cont = line.starts_with([' ', '\t']);
            if cont {
                if let Some(z) = &zone {
                    let zl = Self::zone_line(&f);
                    let done = zl.until_year.is_none();
                    self.zones.get_mut(z).expect("zone").push(zl);
                    if done {
                        zone = None;
                    }
                }
                continue;
            }
            match f[0] {
                "Rule" => self.rules.entry(f[1].to_string()).or_default().push(RuleLine {
                    from: f[2].parse().expect("from"),
                    to: year(f[3], f[2].parse().expect("from")),
                    month: month(f[5]),
                    on: on(f[6]),
                    at: at(f[7]),
                    save: hms(f[8]),
                }),
                "Zone" => {
                    let zl = Self::zone_line(&f[2..]);
                    let done = zl.until_year.is_none();
                    self.zones.insert(f[1].to_string(), vec![zl]);
                    zone = (!done).then(|| f[1].to_string());
                }
                "Link" => {
                    self.links.insert(f[2].to_string(), f[1].to_string());
                }
                _ => {}
            }
        }
    }

    fn zone_line(f: &[&str]) -> ZoneLine {
        ZoneLine { stdoff: hms(f[0]), rules: f[1].to_string(), until_year: f.get(3).map(|y| y.parse().expect("until")) }
    }

    /// The zone or the zone a link names.
    fn zone(&self, name: &str) -> Option<&Vec<ZoneLine>> {
        let mut n = name;
        for _ in 0..4 {
            if let Some(z) = self.zones.get(n) {
                return Some(z);
            }
            n = self.links.get(n)?;
        }
        None
    }

    /// Whether `name` is a zone or a link to one.
    pub fn knows(&self, name: &str) -> bool {
        self.zone(name).is_some()
    }

    /// The rule `name` follows in `y`: the zone line in force at the
    /// middle of the year and its rules' start and end in that year.
    pub fn rule(&self, name: &str, y: i64) -> Result<YearRule, String> {
        let lines = self.zone(name).ok_or_else(|| format!("{name} is not in tzdata"))?;
        let zl = lines
            .iter()
            .find(|l| l.until_year.is_none_or(|u| u > y))
            .ok_or_else(|| format!("{name} has no line for {y}"))?;
        if zl.rules == "-" {
            return Ok(YearRule { stdoff: zl.stdoff, dst: None });
        }
        if zl.rules.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(format!("{name} keeps fixed saving {} in {y}", zl.rules));
        }
        let rules = self.rules.get(&zl.rules).ok_or_else(|| format!("rule {} missing", zl.rules))?;
        let live: Vec<&RuleLine> = rules.iter().filter(|r| r.from <= y && y <= r.to).collect();
        let start: Vec<&&RuleLine> = live.iter().filter(|r| r.save > 0).collect();
        let end: Vec<&&RuleLine> = live.iter().filter(|r| r.save == 0).collect();
        match (start.as_slice(), end.as_slice()) {
            ([], _) => Ok(YearRule { stdoff: zl.stdoff, dst: None }),
            ([s], [e]) => {
                let save = s.save;
                Ok(YearRule {
                    stdoff: zl.stdoff,
                    dst: Some((edge(s, y, zl.stdoff, 0), edge(e, y, zl.stdoff, save), save)),
                })
            }
            _ => Err(format!("{name} has {} starts and {} ends in {y}", start.len(), end.len())),
        }
    }
}

/// A rule line as a *TIMZON edge in year `y`: `save` is the saving in
/// force before it (local wall time is standard time plus it).
fn edge(r: &RuleLine, y: i64, stdoff: i64, save: i64) -> Edge {
    let secs = match r.at.clock {
        Clock::Wall => r.at.secs,
        Clock::Standard => r.at.secs + save,
        Clock::Utc => r.at.secs + stdoff + save,
    };
    let (weekday, week) = match r.on {
        On::Last(d) => (d, 5),
        On::AtLeast(d, n) if (n - 1) % 7 == 0 => (d, (n - 1) / 7 + 1),
        _ => {
            // A date: its weekday and which of them in the month it is.
            let day = day_of(r, y);
            let wd = u32::try_from((crate_days(y, r.month, day) + 4).rem_euclid(7)).expect("weekday");
            (wd, (day - 1) / 7 + 1)
        }
    };
    Edge { month: r.month, weekday, week, secs }
}

/// The day of the month a rule falls on in `y`.
fn day_of(r: &RuleLine, y: i64) -> u32 {
    let wd = |d: u32| u32::try_from((crate_days(y, r.month, d) + 4).rem_euclid(7)).expect("weekday");
    match r.on {
        On::Day(d) => d,
        On::AtLeast(w, n) => (n..n + 7).find(|&d| wd(d) == w).expect("day"),
        On::AtMost(w, n) => (n - 6..=n).rev().find(|&d| wd(d) == w).expect("day"),
        On::Last(w) => {
            let last = (28..=31).rev().find(|&d| crate_days(y, r.month, d) < crate_days(y, r.month + 1, 1) || r.month == 12).unwrap_or(28);
            (last - 6..=last).rev().find(|&d| wd(d) == w).expect("day")
        }
    }
}

/// Days since 1970-01-01 (proleptic Gregorian); month 13 is January of
/// the next year.
fn crate_days(y: i64, m: u32, d: u32) -> i64 {
    let (y, m) = if m > 12 { (y + 1, m - 12) } else { (y, m) };
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

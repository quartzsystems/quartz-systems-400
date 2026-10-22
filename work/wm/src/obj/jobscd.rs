// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*JOBSCD`: the job schedule, `QUSRSYS/QDFTJOBSCD`. Each entry is a
//! command to submit once, weekly or monthly at a time of day; the
//! system job `QSYSSCD` submits each when it falls due. Entries name
//! local dates and times of day, `QTIMZON`'s; `next` is Unix seconds.

use std::fmt::Write as _;

use ipc::{wire_enum, wire_struct};

use crate::cmd::Params;
use crate::rec::{Layout, Rec, field, text_bytes, text_of};
use crate::{Error, Result};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "JobSchedule",
    size: Some(32),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Reserved", 10, 2),
        field("NextEntry", 12, 4),
        field("Count", 16, 4),
        field("Reserved2", 20, 12),
    ],
};

/// One entry's fixed part.
pub const ENTRY_LAYOUT: Layout = Layout {
    name: "JobScheduleEntry",
    size: Some(256),
    fields: &[
        field("Number", 0, 4),
        field("Frequency", 4, 1),
        field("RcyAcn", 5, 1),
        field("Held", 6, 1),
        field("Save", 7, 1),
        field("Days", 8, 1),
        field("RelDays", 9, 1),
        field("DateKind", 10, 1),
        field("Reserved", 11, 1),
        field("Date", 12, 4),
        field("Time", 16, 4),
        field("Next", 20, 8),
        field("LastSubmit", 28, 8),
        field("Job", 36, 10),
        field("JobdName", 46, 10),
        field("JobdLib", 56, 10),
        field("JobqName", 66, 10),
        field("JobqLib", 76, 10),
        field("User", 86, 10),
        field("MsgqName", 96, 10),
        field("MsgqLib", 106, 10),
        field("LastJob", 116, 28),
        field("Text", 144, 50),
        field("Omits", 194, 2),
        field("CmdLen", 196, 4),
        field("Reserved2", 200, 56),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMJSCD";
const VERSION: u64 = 1;

/// The schedule's name and library.
pub const SCHEDULE: (&str, &str) = ("QUSRSYS", "QDFTJOBSCD");

/// `FRQ`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Frequency {
    /// `*ONCE`.
    #[default]
    Once,
    /// `*WEEKLY`.
    Weekly,
    /// `*MONTHLY`.
    Monthly,
}

/// What the entry's date is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateKind {
    /// A civil date, `YYYYMMDD` in `date`.
    #[default]
    Date,
    /// `*CURRENT`: the day it was added.
    Current,
    /// `*MONTHSTR`: the first of each month.
    MonthStart,
    /// `*MONTHEND`: the last of each month.
    MonthEnd,
    /// `*NONE`: the days in `days` decide.
    None,
}

/// `RCYACN`: what a missed entry does after an IPL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RcyAcn {
    /// `*SBMRLS`: submit it.
    #[default]
    SbmRls,
    /// `*SBMHLD`: submit it held.
    SbmHld,
    /// `*NOSBM`: skip it.
    NoSbm,
}

/// An entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScdEntry {
    /// The entry number, unique in the schedule.
    pub number: u32,
    /// `JOB`.
    pub job: String,
    /// `CMD`.
    pub cmd: String,
    /// `FRQ`.
    pub frequency: Frequency,
    /// What `date` is.
    pub date_kind: DateKind,
    /// `SCDDATE` as `YYYYMMDD` when `date_kind` is a date.
    pub date: u32,
    /// `SCDDAY`: bit 0 Monday to bit 6 Sunday; 0 is `*NONE`.
    pub days: u8,
    /// `RELDAYMON`: bit 0 the first to bit 4 the fifth, 5 the last;
    /// 0 for none.
    pub reldays: u8,
    /// `SCDTIME`, seconds after midnight.
    pub time: u32,
    /// `RCYACN`.
    pub rcyacn: RcyAcn,
    /// Held (`HLDJOBSCDE`).
    pub held: bool,
    /// `SAVE(*YES)`: a `*ONCE` entry stays after it runs.
    pub save: bool,
    /// `JOBD`, `lib/name`.
    pub jobd: String,
    /// `JOBQ`, `lib/name`; empty for `*JOBD`.
    pub jobq: String,
    /// `USER`; empty for `*JOBD`.
    pub user: String,
    /// `MSGQ`, `lib/name`; empty for `*USRPRF`.
    pub msgq: String,
    /// `TEXT`.
    pub text: String,
    /// `OMITDATE` as `YYYYMMDD`.
    pub omit: Vec<u32>,
    /// When it is next due, Unix seconds; 0 when it will not run again.
    pub next: u64,
    /// When it was last submitted.
    pub last_submit: u64,
    /// The job it last submitted, `number/user/name`.
    pub last_job: String,
}

wire_enum!(Frequency {
    0 => Once,
    1 => Weekly,
    2 => Monthly,
});

wire_enum!(DateKind {
    0 => Date,
    1 => Current,
    2 => MonthStart,
    3 => MonthEnd,
    4 => None,
});

wire_enum!(RcyAcn {
    0 => SbmRls,
    1 => SbmHld,
    2 => NoSbm,
});

wire_struct!(ScdEntry {
    number,
    job,
    cmd,
    frequency,
    date_kind,
    date,
    days,
    reldays,
    time,
    rcyacn,
    held,
    save,
    jobd,
    jobq,
    user,
    msgq,
    text,
    omit,
    next,
    last_submit,
    last_job
});

/// The UTC offset in seconds in effect at a Unix time: the zone
/// `QTIMZON` names. `&|_| 0` is UTC.
pub type UtcOffset<'a> = &'a dyn Fn(u64) -> i32;

/// `t` moved by `by` seconds, never below zero.
fn shift(t: u64, by: i32) -> u64 {
    u64::try_from(i64::try_from(t).unwrap_or(0) + i64::from(by)).unwrap_or(0)
}

/// The Unix time of a local time, through the offset in effect then.
fn to_utc(local: u64, off: UtcOffset<'_>) -> u64 {
    let guess = shift(local, -off(local));
    shift(local, -off(guess))
}

/// Days since 1970-01-01 of a civil date.
#[must_use]
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a day number.
#[must_use]
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

/// `YYYYMMDD` of a day number.
fn ymd(days: i64) -> u32 {
    let (y, m, d) = civil_from_days(days);
    u32::try_from(y * 10_000 + i64::from(m) * 100 + i64::from(d)).unwrap_or(0)
}

/// The day number of `YYYYMMDD`.
fn day_of(ymd: u32) -> i64 {
    days_from_civil(i64::from(ymd / 10_000), ymd / 100 % 100, ymd % 100)
}

/// 0 Monday to 6 Sunday.
fn weekday(days: i64) -> u8 {
    u8::try_from((days + 3).rem_euclid(7)).unwrap_or(0)
}

/// Days in a month.
fn month_len(y: i64, m: u32) -> u32 {
    let next = if m == 12 {
        days_from_civil(y + 1, 1, 1)
    } else {
        days_from_civil(y, m + 1, 1)
    };
    u32::try_from(next - days_from_civil(y, m, 1)).unwrap_or(30)
}

impl ScdEntry {
    /// Whether the entry runs on day `days`.
    fn runs_on(&self, days: i64) -> bool {
        if self.omit.contains(&ymd(days)) {
            return false;
        }
        let (y, m, d) = civil_from_days(days);
        let wd = weekday(days);
        let on_day = self.days & (1 << wd) != 0;
        match self.frequency {
            Frequency::Once => match self.date_kind {
                DateKind::Date => ymd(days) >= self.date,
                _ => true,
            },
            Frequency::Weekly => match self.date_kind {
                DateKind::Date => ymd(days) >= self.date && weekday(day_of(self.date)) == wd,
                _ => on_day,
            },
            Frequency::Monthly => match self.date_kind {
                DateKind::MonthStart => d == 1,
                DateKind::MonthEnd => d == month_len(y, m),
                DateKind::Date => ymd(days) >= self.date && d == self.date % 100,
                DateKind::Current | DateKind::None => {
                    if !on_day {
                        return false;
                    }
                    if self.reldays == 0 {
                        return true;
                    }
                    let nth = (d - 1) / 7;
                    let last = d + 7 > month_len(y, m);
                    (self.reldays & (1 << nth) != 0) || (last && self.reldays & (1 << 5) != 0)
                }
            },
        }
    }

    /// The first time it is due after `after` (Unix seconds) with the
    /// entry's dates and time of day read as UTC; `None` when it will
    /// not run again.
    #[must_use]
    pub fn next_after(&self, after: u64) -> Option<u64> {
        self.next_after_in(after, &|_| 0)
    }

    /// `next_after` with the entry's dates and time of day read in the
    /// zone `off` describes.
    #[must_use]
    pub fn next_after_in(&self, after: u64, off: UtcOffset<'_>) -> Option<u64> {
        let local_after = shift(after, off(after));
        let start = i64::try_from(local_after / 86_400).unwrap_or(0);
        let first = match (self.frequency, self.date_kind) {
            (_, DateKind::Date) => start.max(day_of(self.date)),
            _ => start,
        };
        // A once entry only ever looks at its date (or, for *CURRENT,
        // the day it is added and the day after when its time passed).
        for day in first..first + 400 {
            if !self.runs_on(day) {
                continue;
            }
            let at = to_utc(u64::try_from(day).ok()? * 86_400 + u64::from(self.time), off);
            if at > after {
                return Some(at);
            }
            if self.frequency == Frequency::Once && self.date_kind == DateKind::Date {
                return None;
            }
        }
        None
    }

    /// `WRKJOBSCDE`'s status: `HLD`, `SCD`, or `SAV` for a once entry
    /// kept after it ran.
    #[must_use]
    pub fn status(&self) -> &'static str {
        if self.held {
            "HLD"
        } else if self.next == 0 {
            "SAV"
        } else {
            "SCD"
        }
    }
}

/// The schedule.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobScd {
    /// The next entry number.
    pub next_entry: u32,
    /// Entries by number.
    pub entries: Vec<ScdEntry>,
}

fn frq_code(f: Frequency) -> u64 {
    match f {
        Frequency::Once => 0,
        Frequency::Weekly => 1,
        Frequency::Monthly => 2,
    }
}

fn kind_code(k: DateKind) -> u64 {
    match k {
        DateKind::Date => 0,
        DateKind::Current => 1,
        DateKind::MonthStart => 2,
        DateKind::MonthEnd => 3,
        DateKind::None => 4,
    }
}

fn split_q(q: &str) -> (&str, &str) {
    q.split_once('/').unwrap_or(("", q))
}

fn join_q(lib: &str, name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        format!("{lib}/{name}")
    }
}

impl JobScd {
    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("NextEntry", u64::from(self.next_entry));
        r.set_uint("Count", self.entries.len() as u64);
        let mut out = r.buf;
        for e in &self.entries {
            let mut x = Rec::new(&ENTRY_LAYOUT);
            x.set_uint("Number", u64::from(e.number));
            x.set_uint("Frequency", frq_code(e.frequency));
            x.set_uint(
                "RcyAcn",
                match e.rcyacn {
                    RcyAcn::SbmRls => 0,
                    RcyAcn::SbmHld => 1,
                    RcyAcn::NoSbm => 2,
                },
            );
            x.set_flag("Held", e.held);
            x.set_flag("Save", e.save);
            x.set_uint("Days", u64::from(e.days));
            x.set_uint("RelDays", u64::from(e.reldays));
            x.set_uint("DateKind", kind_code(e.date_kind));
            x.set_uint("Date", u64::from(e.date));
            x.set_uint("Time", u64::from(e.time));
            x.set_uint("Next", e.next);
            x.set_uint("LastSubmit", e.last_submit);
            x.set_text("Job", &e.job);
            let (jl, jn) = split_q(&e.jobd);
            x.set_text("JobdName", jn);
            x.set_text("JobdLib", jl);
            let (ql, qn) = split_q(&e.jobq);
            x.set_text("JobqName", qn);
            x.set_text("JobqLib", ql);
            x.set_text("User", &e.user);
            let (ml, mn) = split_q(&e.msgq);
            x.set_text("MsgqName", mn);
            x.set_text("MsgqLib", ml);
            x.set_text("LastJob", &e.last_job);
            x.set_text("Text", &e.text);
            x.set_uint("Omits", e.omit.len() as u64);
            let cmd = text_bytes(&e.cmd, e.cmd.chars().count());
            x.set_uint("CmdLen", cmd.len() as u64);
            out.extend(x.buf);
            for d in &e.omit {
                out.extend_from_slice(&d.to_le_bytes());
            }
            out.extend(cmd);
        }
        out
    }

    /// Read a payload back.
    pub fn decode(payload: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, payload)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a job schedule".into()));
        }
        let mut scd = Self {
            next_entry: u32::try_from(r.uint("NextEntry")).unwrap_or(1),
            entries: Vec::new(),
        };
        let mut rest = r.tail();
        let size = ENTRY_LAYOUT.size.unwrap_or(0);
        let bad = || Error::Corrupt("job schedule entry truncated".into());
        for _ in 0..r.uint("Count") {
            let head = rest.get(..size).ok_or_else(bad)?;
            let x = Rec::read(&ENTRY_LAYOUT, head)?;
            rest = &rest[size..];
            let omits = usize::try_from(x.uint("Omits")).unwrap_or(0);
            let mut omit = Vec::new();
            for _ in 0..omits {
                let d = rest.get(..4).ok_or_else(bad)?;
                omit.push(u32::from_le_bytes(d.try_into().expect("four")));
                rest = &rest[4..];
            }
            let n = usize::try_from(x.uint("CmdLen")).unwrap_or(usize::MAX);
            let cmd = text_of(rest.get(..n).ok_or_else(bad)?);
            rest = &rest[n..];
            let small = |f: &str| u8::try_from(x.uint(f)).unwrap_or(0);
            scd.entries.push(ScdEntry {
                number: u32::try_from(x.uint("Number")).unwrap_or(0),
                job: x.text("Job"),
                cmd,
                frequency: match small("Frequency") {
                    1 => Frequency::Weekly,
                    2 => Frequency::Monthly,
                    _ => Frequency::Once,
                },
                date_kind: match small("DateKind") {
                    1 => DateKind::Current,
                    2 => DateKind::MonthStart,
                    3 => DateKind::MonthEnd,
                    4 => DateKind::None,
                    _ => DateKind::Date,
                },
                date: u32::try_from(x.uint("Date")).unwrap_or(0),
                days: small("Days"),
                reldays: small("RelDays"),
                time: u32::try_from(x.uint("Time")).unwrap_or(0),
                rcyacn: match small("RcyAcn") {
                    1 => RcyAcn::SbmHld,
                    2 => RcyAcn::NoSbm,
                    _ => RcyAcn::SbmRls,
                },
                held: x.flag("Held"),
                save: x.flag("Save"),
                jobd: join_q(&x.text("JobdLib"), &x.text("JobdName")),
                jobq: join_q(&x.text("JobqLib"), &x.text("JobqName")),
                user: x.text("User"),
                msgq: join_q(&x.text("MsgqLib"), &x.text("MsgqName")),
                text: x.text("Text"),
                omit,
                next: x.uint("Next"),
                last_submit: x.uint("LastSubmit"),
                last_job: x.text("LastJob"),
            });
        }
        Ok(scd)
    }

    /// An entry by job name and number (`ENTRYNBR(*ONLY)` when 0).
    pub fn find_mut(&mut self, job: &str, number: u32) -> Result<&mut ScdEntry> {
        let mut hits = self
            .entries
            .iter_mut()
            .filter(|e| e.job == job && (number == 0 || e.number == number));
        let first = hits.next().ok_or_else(|| {
            Error::value("CPF1628", format!("job schedule entry {job} not found"))
        })?;
        if number == 0 && hits.next().is_some() {
            return Err(Error::value(
                "CPF1630",
                format!("more than one entry is named {job}; give ENTRYNBR"),
            ));
        }
        Ok(first)
    }

    /// `WRKJOBSCDE`'s list, the next submit times in UTC.
    #[must_use]
    pub fn display(&self) -> String {
        self.display_in(&|_| 0)
    }

    /// `WRKJOBSCDE`'s list, the next submit times in the zone `off`
    /// describes.
    #[must_use]
    pub fn display_in(&self, off: UtcOffset<'_>) -> String {
        let mut out = String::from(
            "Job         Entry   Status  Schedule Date  Time      Frequency  Next Submit\n",
        );
        for e in &self.entries {
            let date = match e.date_kind {
                DateKind::Date => format!("{}", e.date),
                DateKind::Current => "*CURRENT".into(),
                DateKind::MonthStart => "*MONTHSTR".into(),
                DateKind::MonthEnd => "*MONTHEND".into(),
                DateKind::None => "*NONE".into(),
            };
            let next = if e.next == 0 {
                "*NONE".to_string()
            } else {
                let local = shift(e.next, off(e.next));
                let d = i64::try_from(local / 86_400).unwrap_or(0);
                let t = local % 86_400;
                format!(
                    "{} {:02}:{:02}:{:02}",
                    ymd(d),
                    t / 3600,
                    t % 3600 / 60,
                    t % 60
                )
            };
            let _ = writeln!(
                out,
                "{:<10}  {:06}  {:<6}  {:<13}  {:02}:{:02}:{:02}  {:<9}  {next}",
                e.job,
                e.number,
                e.status(),
                date,
                e.time / 3600,
                e.time % 3600 / 60,
                e.time % 60,
                match e.frequency {
                    Frequency::Once => "*ONCE",
                    Frequency::Weekly => "*WEEKLY",
                    Frequency::Monthly => "*MONTHLY",
                }
            );
            if !e.last_job.is_empty() {
                let _ = writeln!(out, "            last submitted as {}", e.last_job);
            }
        }
        out
    }
}

const DAYS: [&str; 7] = ["*MON", "*TUE", "*WED", "*THU", "*FRI", "*SAT", "*SUN"];

/// `SCDDAY`'s bits.
fn scd_days(list: &str) -> Option<u8> {
    let mut days = 0;
    for word in list.split_whitespace() {
        match word {
            "*NONE" => days = 0,
            "*ALL" => days = 0x7F,
            day => days |= 1 << DAYS.iter().position(|x| *x == day)?,
        }
    }
    Some(days)
}

/// `RELDAYMON`'s bits.
fn rel_days(list: &str) -> Option<u8> {
    let mut bits = 0;
    for word in list.split_whitespace() {
        if word == "*LAST" {
            bits |= 1 << 5;
        } else {
            let nth: u8 = word.parse().ok().filter(|i| (1..=5).contains(i))?;
            bits |= 1 << (nth - 1);
        }
    }
    Some(bits)
}

/// `SCDTIME`, `hhmmss` or `hh:mm:ss`, as seconds after midnight.
fn scd_time(text: &str) -> Option<u32> {
    let digits = text.replace(':', "");
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pair = |at: usize| digits[at..at + 2].parse::<u32>().ok();
    let (hour, min, sec) = (pair(0)?, pair(2)?, pair(4)?);
    (hour <= 23 && min <= 59 && sec <= 59).then_some(hour * 3600 + min * 60 + sec)
}

/// `ADDJOBSCDE` (over the defaults) or `CHGJOBSCDE` (over the entry):
/// the keywords of `p`, dates and times read as UTC. `now` is for
/// `*CURRENT`.
pub fn apply(e: ScdEntry, p: &Params, now: u64) -> Result<ScdEntry> {
    apply_in(e, p, now, &|_| 0)
}

/// `apply` with dates and times read in the zone `off` describes.
pub fn apply_in(mut e: ScdEntry, p: &Params, now: u64, off: UtcOffset<'_>) -> Result<ScdEntry> {
    let bad = |k: &str, v: &str| Error::value("CPD0084", format!("{k}({v}) is not valid"));
    for (k, raw) in &p.kw {
        let v = raw.trim();
        let up = v.to_ascii_uppercase();
        match k.as_str() {
            "JOB" | "ENTRYNBR" => {}
            "CMD" => e.cmd = v.trim_matches('\'').replace("''", "'"),
            "FRQ" => {
                e.frequency = match up.as_str() {
                    "*ONCE" => Frequency::Once,
                    "*WEEKLY" => Frequency::Weekly,
                    "*MONTHLY" => Frequency::Monthly,
                    _ => return Err(bad(k, v)),
                };
            }
            "SCDDATE" => match up.as_str() {
                "*CURRENT" => {
                    e.date_kind = DateKind::Current;
                    e.date = ymd(i64::try_from(shift(now, off(now)) / 86_400).unwrap_or(0));
                }
                "*MONTHSTR" => e.date_kind = DateKind::MonthStart,
                "*MONTHEND" => e.date_kind = DateKind::MonthEnd,
                "*NONE" => e.date_kind = DateKind::None,
                d if d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()) => {
                    e.date_kind = DateKind::Date;
                    e.date = d.parse().map_err(|_| bad(k, v))?;
                }
                _ => return Err(bad(k, v)),
            },
            "SCDDAY" => e.days = scd_days(&up).ok_or_else(|| bad(k, v))?,
            "RELDAYMON" => e.reldays = rel_days(&up).ok_or_else(|| bad(k, v))?,
            "SCDTIME" => e.time = scd_time(&up).ok_or_else(|| bad(k, v))?,
            "RCYACN" => {
                e.rcyacn = match up.as_str() {
                    "*SBMRLS" => RcyAcn::SbmRls,
                    "*SBMHLD" => RcyAcn::SbmHld,
                    "*NOSBM" => RcyAcn::NoSbm,
                    _ => return Err(bad(k, v)),
                };
            }
            "SAVE" => e.save = up == "*YES",
            "JOBD" => e.jobd = up,
            "JOBQ" => e.jobq = if up == "*JOBD" { String::new() } else { up },
            "USER" => e.user = if up == "*JOBD" { String::new() } else { up },
            "MSGQ" => e.msgq = if up == "*USRPRF" { String::new() } else { up },
            "TEXT" => e.text = v.trim_matches('\'').replace("''", "'"),
            "OMITDATE" => {
                e.omit = up
                    .split_whitespace()
                    .filter(|d| *d != "*NONE")
                    .map(|d| d.parse().map_err(|_| bad(k, v)))
                    .collect::<Result<_>>()?;
            }
            _ => {
                return Err(Error::value(
                    "CPD0043",
                    format!("keyword {k} not valid for this command"),
                ));
            }
        }
    }
    if e.cmd.is_empty() {
        return Err(Error::value("CPD0071", "CMD is needed"));
    }
    if e.frequency == Frequency::Weekly && e.days == 0 && e.date_kind != DateKind::Date {
        return Err(Error::value(
            "CPF1633",
            "a weekly entry needs SCDDAY or a date",
        ));
    }
    e.next = e.next_after_in(now, off).unwrap_or(0);
    Ok(e)
}

/// A new entry's defaults.
#[must_use]
pub fn defaults(job: &str, number: u32) -> ScdEntry {
    ScdEntry {
        number,
        job: job.to_string(),
        date_kind: DateKind::Current,
        jobd: "QGPL/QDFTJOBD".into(),
        rcyacn: RcyAcn::SbmRls,
        ..ScdEntry::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-21 00:00:00 UTC, a Wednesday.
    const DAY: u64 = 1_792_540_800;

    fn entry(line: &str, now: u64) -> ScdEntry {
        apply(defaults("T", 1), &Params::parse(line), now).unwrap()
    }

    #[test]
    fn calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(weekday(i64::try_from(DAY / 86_400).unwrap()), 2);
        assert_eq!(month_len(2024, 2), 29);
        assert_eq!(month_len(2026, 2), 28);
    }

    #[test]
    fn once_today_or_tomorrow() {
        let now = DAY + 10 * 3600;
        let e = entry("x T CMD('wrkactjob') SCDTIME(120000)", now);
        assert_eq!(e.next, DAY + 12 * 3600);
        let late = entry("x T CMD('wrkactjob') SCDTIME(080000)", now);
        assert_eq!(late.next, DAY + 86_400 + 8 * 3600);
        let fixed = entry("x T CMD('a') SCDDATE(20261020) SCDTIME(080000)", now);
        assert_eq!(fixed.next, 0, "a past date never runs");
    }

    #[test]
    fn local_time() {
        // Two hours east: 12:00 local is 10:00 UTC.
        let east = |_: u64| 7200;
        let now = DAY + 3600;
        let e = apply_in(
            defaults("T", 1),
            &Params::parse("x T CMD('a') SCDTIME(120000)"),
            now,
            &east,
        )
        .unwrap();
        assert_eq!(e.next, DAY + 10 * 3600);
        // 01:00 local is already past at 03:00 local: tomorrow.
        let late = apply_in(
            defaults("T", 1),
            &Params::parse("x T CMD('a') SCDTIME(010000)"),
            now,
            &east,
        )
        .unwrap();
        assert_eq!(late.next, DAY + 86_400 - 3600);
        // *CURRENT just before local midnight names the local day.
        let eve = DAY + 86_400 - 3600;
        let c = apply_in(
            defaults("T", 1),
            &Params::parse("x T CMD('a') SCDDATE(*CURRENT) SCDTIME(233000)"),
            eve,
            &east,
        )
        .unwrap();
        assert_eq!(c.date, 20_261_022);
    }

    #[test]
    fn weekly_monthly() {
        let now = DAY + 3600;
        let w = entry(
            "x T CMD('a') FRQ(*WEEKLY) SCDDAY(*MON *FRI) SCDTIME(060000)",
            now,
        );
        // Friday 2026-10-23.
        assert_eq!(w.next, DAY + 2 * 86_400 + 6 * 3600);
        let start = entry(
            "x T CMD('a') FRQ(*MONTHLY) SCDDATE(*MONTHSTR) SCDTIME(000100)",
            now,
        );
        assert_eq!(
            start.next,
            u64::try_from(days_from_civil(2026, 11, 1)).unwrap() * 86_400 + 60
        );
        let end = entry(
            "x T CMD('a') FRQ(*MONTHLY) SCDDATE(*MONTHEND) SCDTIME(230000)",
            now,
        );
        assert_eq!(
            end.next,
            u64::try_from(days_from_civil(2026, 10, 31)).unwrap() * 86_400 + 23 * 3600
        );
        let rel = entry(
            "x T CMD('a') FRQ(*MONTHLY) SCDDATE(*NONE) SCDDAY(*TUE) RELDAYMON(1) SCDTIME(090000)",
            now,
        );
        assert_eq!(
            rel.next,
            u64::try_from(days_from_civil(2026, 11, 3)).unwrap() * 86_400 + 9 * 3600
        );
        let omitted = entry(
            "x T CMD('a') FRQ(*WEEKLY) SCDDAY(*FRI) SCDTIME(060000) OMITDATE(20261023)",
            now,
        );
        assert_eq!(omitted.next, DAY + 9 * 86_400 + 6 * 3600);
    }

    #[test]
    fn round_trip() {
        let mut s = JobScd {
            next_entry: 3,
            entries: vec![
                entry("x T CMD('wrkactjob') SCDTIME(120000) TEXT('a')", DAY),
                entry(
                    "x T CMD('b') FRQ(*WEEKLY) SCDDAY(*ALL) SCDTIME(010203) OMITDATE(20261225)",
                    DAY,
                ),
            ],
        };
        s.entries[1].number = 2;
        s.entries[1].last_job = "000123/QSYS/T".into();
        assert_eq!(JobScd::decode(&s.encode()).unwrap(), s);
        assert!(s.find_mut("T", 0).is_err(), "two entries named T");
        assert!(s.find_mut("T", 2).is_ok());
        assert!(s.display().contains("*WEEKLY"));
    }
}

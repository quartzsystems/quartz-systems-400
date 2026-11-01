// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System values, disk status, hardware resources, time management.
//! Every read is what `WRKSYSVAL`, `WRKDSKSTS`, `WRKHDWRSC` and
//! `DSPSYSVAL` show the user; every change is a CL command run as the
//! user through [`Config::run`].

use qsweb::msg::Message;

/// What a system value accepts, as its catalog says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Allowed {
    /// Any text that fits.
    #[default]
    Any,
    /// One of these.
    Values(Vec<String>),
    /// A number in `min..=max`, or one of the special values.
    Range(i64, i64, Vec<String>),
    /// Up to `max` of these, separated by blanks.
    List(Vec<String>, usize),
}

impl Allowed {
    /// The grey hint beside the field (`*NOMAX or 1-25`).
    pub fn hint(&self) -> String {
        match self {
            Allowed::Any => String::new(),
            Allowed::Values(v) => v.join(", "),
            Allowed::Range(min, max, sp) if sp.is_empty() => format!("{min}-{max}"),
            Allowed::Range(min, max, sp) => format!("{} or {min}-{max}", sp.join(", ")),
            Allowed::List(v, max) => format!("up to {max} of {}", v.join(", ")),
        }
    }

    /// Whether `value` is allowed.
    pub fn accepts(&self, value: &str) -> bool {
        let v = value.trim().to_ascii_uppercase();
        if v.is_empty() {
            return false;
        }
        match self {
            Allowed::Any => true,
            Allowed::Values(vs) => vs.iter().any(|a| a.eq_ignore_ascii_case(&v)),
            Allowed::Range(min, max, sp) => {
                sp.iter().any(|a| a.eq_ignore_ascii_case(&v))
                    || v.parse::<i64>().is_ok_and(|n| (*min..=*max).contains(&n))
            }
            Allowed::List(vs, max) => {
                let words: Vec<&str> = v.split_whitespace().collect();
                words.len() <= *max
                    && words
                        .iter()
                        .all(|w| vs.iter().any(|a| a.eq_ignore_ascii_case(w)))
            }
        }
    }
}

/// One system value, as `WRKSYSVAL` and `DSPSYSVAL` show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SysVal {
    /// `QMAXSIGN`.
    pub name: String,
    /// The category's key ([`CATEGORIES`]).
    pub category: String,
    /// The value as `DSPSYSVAL` shows it.
    pub value: String,
    /// The value a new system has.
    pub shipped: String,
    /// The description.
    pub text: String,
    /// What it accepts.
    pub allowed: Allowed,
    /// A change takes effect at the next IPL.
    pub ipl: bool,
    /// When it was last changed (seconds since 1970) and by whom.
    pub changed: Option<(u64, String)>,
}

/// The categories `WRKSYSVAL` groups the values in: key, title.
pub const CATEGORIES: &[(&str, &str)] = &[
    ("security", "Security"),
    ("datetime", "Date and time"),
    ("jobs", "Jobs"),
    ("storage", "Storage"),
    ("msgs", "Messages and logging"),
    ("devices", "Devices"),
    ("libl", "Library list"),
    ("perf", "Performance"),
    ("power", "Power"),
    ("save", "Save and restore"),
    ("sysctl", "System control"),
];

/// An auxiliary storage pool (`WRKDSKSTS`, `WRKASPCFG`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asp {
    /// The ASP number.
    pub number: u32,
    /// `System`, `User (basic)`, `Independent (IASP INVIASP)`.
    pub kind: String,
    /// Capacity, bytes.
    pub capacity: u64,
    /// Used, bytes.
    pub used: u64,
    /// Used, tenths of a percent.
    pub used_pct: u32,
    /// `Mirrored`, `None`, ...
    pub protection: String,
    /// Whether it has overflowed into the system ASP; `None` for an
    /// independent ASP, which cannot.
    pub overflow: Option<bool>,
    /// The storage threshold, percent.
    pub threshold: u32,
}

/// A disk unit (`WRKDSKSTS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskUnit {
    /// The unit number.
    pub unit: u32,
    /// `DD001`.
    pub resource: String,
    /// `6B22`.
    pub kind: String,
    /// `050`.
    pub model: String,
    /// Capacity, bytes.
    pub capacity: u64,
    /// Used, tenths of a percent.
    pub used_pct: u32,
    /// `Mirrored (A)`, `None`, `Non-configured`.
    pub protection: String,
    /// `Active`, `Suspended`, `Failed`, `Non-configured`.
    pub status: String,
    /// The ASP, `None` when not configured.
    pub asp: Option<u32>,
    /// The serial number.
    pub serial: String,
}

/// `WRKDSKSTS`'s data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disks {
    /// The ASPs.
    pub asps: Vec<Asp>,
    /// The units, configured and not.
    pub units: Vec<DiskUnit>,
    /// Seconds since the user's I/O counters were reset.
    pub elapsed: u32,
}

/// A hardware resource (`WRKHDWRSC`), in tree order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resource {
    /// `CMN01`.
    pub name: String,
    /// The depth in the tree, 0 for the system unit.
    pub depth: u32,
    /// `5706`, `9Q04-M2`.
    pub type_model: String,
    /// The description.
    pub text: String,
    /// `Operational`, `Not operational`, `Not connected`.
    pub status: String,
    /// The serial number.
    pub serial: String,
    /// `U07`, `Frame 1`.
    pub location: String,
    /// `Communications`, `Storage`, `Processor`, `Workstation`; blank
    /// for the system unit.
    pub category: String,
    /// The configuration descriptions over it: name, `*LIN`, `*CTL`,
    /// `*DEV` (what `VRYCFG` varies).
    pub cfg: Vec<(String, String)>,
}

/// Time management: the date and time system values and the NTP
/// client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Time {
    /// `QTIMZON`.
    pub zone: String,
    /// The time zone descriptions offered: name, description.
    pub zones: Vec<(String, String)>,
    /// The offset from UTC now (`-04:00`).
    pub offset: String,
    /// Daylight saving time is in effect.
    pub dst: bool,
    /// `QDATFMT`.
    pub date_format: String,
    /// `QDATSEP`.
    pub date_sep: String,
    /// `QTIMSEP`.
    pub time_sep: String,
    /// `QTIMADJ`.
    pub adjust: String,
    /// The `QTIMADJ` values offered: value, description.
    pub adjusters: Vec<(String, String)>,
    /// The NTP client's servers (`CHGNTPA RMTSYS`).
    pub ntp_servers: Vec<String>,
    /// The last synchronization: when, the offset applied in
    /// microseconds.
    pub last_sync: Option<(u64, i64)>,
    /// The `*NTP` server is active.
    pub ntp_active: bool,
}

/// What the system pages need. `user` is the signed-on user: the live
/// system reads and acts as that user.
pub trait Config: Send + Sync {
    /// `WRKSYSVAL`: every system value, in the system's order.
    fn sysvals(&self, user: &str) -> Vec<SysVal>;
    /// `WRKDSKSTS`; `reset` restarts the user's I/O counters
    /// (`RESET(*YES)`).
    fn disks(&self, user: &str, reset: bool) -> Disks;
    /// `WRKHDWRSC`: the resources in tree order.
    fn resources(&self, user: &str) -> Vec<Resource>;
    /// The date and time values and the NTP client's state.
    fn time(&self, user: &str) -> Time;
    /// Run CL command `command` as `user`: its completion message, or
    /// the message it failed with.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}

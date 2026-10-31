// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The cleanup options (`CHGCLNUP`), kept in `QUSRSYS/QEZCLNOPT`: whether
//! cleanup may run, when it starts each day, how many days each kind of
//! item is kept, the job queue and priority of the cleanup job.

use std::collections::BTreeMap;

use super::{Keep, decode_kv, encode_kv, time_of_day, yes_no, yn};

/// The user space that holds the options.
pub const OBJECT: (&str, &str) = ("QUSRSYS", "QEZCLNOPT");
/// The job schedule entry `STRCLNUP OPTION(*SCHED)` adds.
pub const SCHED_ENTRY: &str = "QEZCLNUP";
/// The command the entry and `OPTION(*IMMED)` run.
pub const RUN_CMD: &str = "STRCLNUP OPTION(*IMMED)";

/// The options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleanup {
    /// `ALWCLNUP`.
    pub allow: bool,
    /// `STRTIME`, `HH:MM:SS`, or `*SCDPWROFF`.
    pub strtime: String,
    /// `USRMSG`: messages in user message queues.
    pub usrmsg: Keep,
    /// `SYSMSG`: messages in `QSYSOPR` and workstation queues.
    pub sysmsg: Keep,
    /// `CRITSYSMSG`: critical messages in `QSYSOPR`.
    pub critsysmsg: Keep,
    /// `SYSPRT`: job logs and system output on `QEZJOBLOG`, `QEZDEBUG`.
    pub sysprt: Keep,
    /// `SYSLOG`: `QHST` files, system journal receivers, problems.
    pub syslog: Keep,
    /// `CALITM`: office calendar items (none here).
    pub calitm: Keep,
    /// `JOBQ`: library, name.
    pub jobq: (String, String),
    /// `JRNRCVSIZ`, KB.
    pub jrnrcvsiz: u32,
    /// `RUNPTY`.
    pub runpty: u8,
    /// `STRCLNUP OPTION(*SCHED)` added the schedule entry.
    pub scheduled: bool,
    /// The last run, Unix seconds; 0 before the first.
    pub last_run: u64,
    /// What the last run removed.
    pub last_result: String,
}

impl Default for Cleanup {
    fn default() -> Self {
        Self {
            allow: true,
            strtime: "22:00:00".into(),
            usrmsg: Keep::Days(7),
            sysmsg: Keep::Days(4),
            critsysmsg: Keep::Forever,
            sysprt: Keep::Days(7),
            syslog: Keep::Days(30),
            calitm: Keep::Days(30),
            jobq: ("*LIBL".into(), "QCTL".into()),
            jrnrcvsiz: 100_000,
            runpty: 55,
            scheduled: false,
            last_run: 0,
            last_result: String::new(),
        }
    }
}

/// A value `CHGCLNUP` refuses: the keyword and the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bad(pub &'static str, pub String);

impl Cleanup {
    /// Decode, defaults for what is missing.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let kv = decode_kv(text);
        let mut c = Self::default();
        let keep = |k: &str, d: Keep| kv.get(k).and_then(|v| Keep::parse(v)).unwrap_or(d);
        c.allow = kv.get("allow").and_then(|v| yes_no(v)).unwrap_or(c.allow);
        if let Some(v) = kv.get("strtime") {
            c.strtime.clone_from(v);
        }
        c.usrmsg = keep("usrmsg", c.usrmsg);
        c.sysmsg = keep("sysmsg", c.sysmsg);
        c.critsysmsg = keep("critsysmsg", c.critsysmsg);
        c.sysprt = keep("sysprt", c.sysprt);
        c.syslog = keep("syslog", c.syslog);
        c.calitm = keep("calitm", c.calitm);
        if let Some((l, n)) = kv.get("jobq").and_then(|v| v.split_once('/')) {
            c.jobq = (l.to_string(), n.to_string());
        }
        c.jrnrcvsiz = kv.get("jrnrcvsiz").and_then(|v| v.parse().ok()).unwrap_or(c.jrnrcvsiz);
        c.runpty = kv.get("runpty").and_then(|v| v.parse().ok()).unwrap_or(c.runpty);
        c.scheduled = kv.get("scheduled").and_then(|v| yes_no(v)).unwrap_or(false);
        c.last_run = kv.get("last_run").and_then(|v| v.parse().ok()).unwrap_or(0);
        c.last_result = kv.get("last_result").cloned().unwrap_or_default();
        c
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut kv = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            kv.insert(k.to_string(), v);
        };
        put("allow", yn(self.allow).into());
        put("strtime", self.strtime.clone());
        put("usrmsg", self.usrmsg.text());
        put("sysmsg", self.sysmsg.text());
        put("critsysmsg", self.critsysmsg.text());
        put("sysprt", self.sysprt.text());
        put("syslog", self.syslog.text());
        put("calitm", self.calitm.text());
        put("jobq", format!("{}/{}", self.jobq.0, self.jobq.1));
        put("jrnrcvsiz", self.jrnrcvsiz.to_string());
        put("runpty", self.runpty.to_string());
        put("scheduled", yn(self.scheduled).into());
        put("last_run", self.last_run.to_string());
        put("last_result", self.last_result.clone());
        encode_kv(&kv)
    }

    /// `CHGCLNUP`: each keyword `get` gives a value for, other than
    /// `*SAME`, changes its option. Nothing changes when one is bad.
    pub fn apply(&mut self, get: impl Fn(&str) -> Option<String>) -> Result<(), Bad> {
        let mut c = self.clone();
        let val = |k: &str| {
            get(k)
                .map(|v| v.trim().trim_matches('\'').trim().to_string())
                .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("*SAME"))
        };
        if let Some(v) = val("ALWCLNUP") {
            c.allow = yes_no(&v).ok_or(Bad("ALWCLNUP", v))?;
        }
        if let Some(v) = val("STRTIME") {
            let up = v.to_ascii_uppercase();
            if up == "*SCDPWROFF" {
                c.strtime = up;
            } else {
                let s = time_of_day(&v).ok_or(Bad("STRTIME", v))?;
                c.strtime = format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
            }
        }
        for (k, slot) in [
            ("USRMSG", &mut c.usrmsg),
            ("SYSMSG", &mut c.sysmsg),
            ("CRITSYSMSG", &mut c.critsysmsg),
            ("SYSPRT", &mut c.sysprt),
            ("SYSLOG", &mut c.syslog),
            ("CALITM", &mut c.calitm),
        ] {
            if let Some(v) = val(k) {
                *slot = Keep::parse(&v).ok_or(Bad(k, v))?;
            }
        }
        if let Some(v) = val("JOBQ") {
            let v = v.to_ascii_uppercase();
            let (l, n) = v.split_once('/').map_or(("*LIBL", v.as_str()), |(l, n)| (l, n));
            if qsname::Name::parse(n).is_err() {
                return Err(Bad("JOBQ", v.clone()));
            }
            c.jobq = (l.to_string(), n.to_string());
        }
        if let Some(v) = val("JRNRCVSIZ") {
            let n: u32 = v.parse().map_err(|_| Bad("JRNRCVSIZ", v.clone()))?;
            if !(1000..=1_000_000_000).contains(&n) {
                return Err(Bad("JRNRCVSIZ", v));
            }
            c.jrnrcvsiz = n;
        }
        if let Some(v) = val("RUNPTY") {
            let n: u8 = v.parse().map_err(|_| Bad("RUNPTY", v.clone()))?;
            if !(1..=99).contains(&n) {
                return Err(Bad("RUNPTY", v));
            }
            c.runpty = n;
        }
        *self = c;
        Ok(())
    }

    /// The keyword form of the options, for the prompt.
    #[must_use]
    pub fn keyword_form(&self) -> String {
        format!(
            "ALWCLNUP({}) STRTIME({}) USRMSG({}) SYSMSG({}) CRITSYSMSG({}) SYSPRT({}) \
             SYSLOG({}) JOBQ({}/{}) JRNRCVSIZ({}) CALITM({}) RUNPTY({})",
            yn(self.allow),
            if self.strtime.starts_with('*') {
                self.strtime.clone()
            } else {
                format!("'{}'", self.strtime)
            },
            self.usrmsg.text(),
            self.sysmsg.text(),
            self.critsysmsg.text(),
            self.sysprt.text(),
            self.syslog.text(),
            self.jobq.0,
            self.jobq.1,
            self.jrnrcvsiz,
            self.calitm.text(),
            self.runpty,
        )
    }

    /// The schedule entry's time, `HHMMSS`; `None` for `*SCDPWROFF`.
    #[must_use]
    pub fn scdtime(&self) -> Option<String> {
        time_of_day(&self.strtime).map(|s| format!("{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60))
    }

    /// The `ADDJOBSCDE` of the daily cleanup.
    #[must_use]
    pub fn sched_entry(&self) -> Option<String> {
        let t = self.scdtime()?;
        Some(format!(
            "addjobscde JOB({SCHED_ENTRY}) CMD('{RUN_CMD}') FRQ(*WEEKLY) SCDDAY(*ALL) SCDTIME({t}) \
             JOBQ({}/{}) USER(QPGMR) TEXT('Daily cleanup')",
            self.jobq.0, self.jobq.1
        ))
    }
}

/// What one cleanup run removed, by kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Removed {
    /// Spooled job logs and system output.
    pub spooled: usize,
    /// User queue messages.
    pub user_msgs: usize,
    /// System and workstation queue messages.
    pub sys_msgs: usize,
    /// `QHST` files.
    pub hist_files: usize,
    /// Journal receivers.
    pub receivers: usize,
    /// Problems.
    pub problems: usize,
    /// `QRPLOBJ` objects.
    pub replaced: usize,
}

impl Removed {
    /// One line for the history log and the options.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} spooled file(s), {} user message(s), {} system message(s), {} history file(s), \
             {} journal receiver(s), {} problem(s), {} replaced object(s) removed",
            self.spooled,
            self.user_msgs,
            self.sys_msgs,
            self.hist_files,
            self.receivers,
            self.problems,
            self.replaced
        )
    }
}

/// A message queue message and whether cleanup may remove it:
/// inquiries waiting for a reply never go; a critical message (severity
/// 90 or more on `QSYSOPR`) follows `CRITSYSMSG`.
#[must_use]
pub fn message_expired(c: &Cleanup, system_queue: bool, severity: u8, unanswered: bool, sent: u64, now: u64) -> bool {
    if unanswered {
        return false;
    }
    let keep = match (system_queue, severity >= 90) {
        (false, _) => c.usrmsg,
        (true, true) => c.critsysmsg,
        (true, false) => c.sysmsg,
    };
    keep.expired(sent, now)
}

#[cfg(test)]
mod tests {
    use super::super::DAY;
    use super::*;

    fn get<'a>(kv: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| kv.iter().find(|(x, _)| *x == k).map(|(_, v)| (*v).to_string())
    }

    #[test]
    fn change_keeps_what_is_same() {
        let mut c = Cleanup::default();
        c.apply(get(&[("USRMSG", "31"), ("STRTIME", "'063000'"), ("SYSMSG", "*SAME")])).unwrap();
        assert_eq!(c.usrmsg, Keep::Days(31));
        assert_eq!(c.sysmsg, Keep::Days(4));
        assert_eq!(c.strtime, "06:30:00");
        assert_eq!(c.scdtime().as_deref(), Some("063000"));
        let before = c.clone();
        assert_eq!(c.apply(get(&[("USRMSG", "1"), ("SYSLOG", "0")])), Err(Bad("SYSLOG", "0".into())));
        assert_eq!(c, before, "a bad value changes nothing");
    }

    #[test]
    fn options_persist() {
        let mut c = Cleanup::default();
        c.apply(get(&[("ALWCLNUP", "*NO"), ("CRITSYSMSG", "60"), ("JOBQ", "QGPL/NIGHT"), ("RUNPTY", "20")]))
            .unwrap();
        c.last_run = 1_792_540_800;
        c.last_result = "3 spooled file(s) removed".into();
        let back = Cleanup::decode(&c.encode());
        assert_eq!(back, c);
        assert_eq!(Cleanup::decode(""), Cleanup::default());
        assert!(c.keyword_form().contains("JOBQ(QGPL/NIGHT)"));
    }

    #[test]
    fn messages_by_queue_and_severity() {
        let c = Cleanup::default();
        let now = 50 * DAY;
        assert!(message_expired(&c, false, 0, false, now - 8 * DAY, now));
        assert!(!message_expired(&c, false, 0, false, now - 6 * DAY, now));
        assert!(message_expired(&c, true, 10, false, now - 5 * DAY, now));
        assert!(!message_expired(&c, true, 99, false, 0, now), "critical kept");
        assert!(!message_expired(&c, true, 10, true, 0, now), "an inquiry waits");
    }
}

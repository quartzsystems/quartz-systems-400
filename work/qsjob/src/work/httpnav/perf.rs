// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::perf::Perf` as the user: `panel2_data SYSACT`
//! (`WRKSYSACT`), `SYSSTS RESET(*YES)`'s pools (`WRKSYSSTS`) and
//! `DSKSTS RESET(*YES)` (`WRKDSKSTS`). Each display ends the interval
//! and starts the next, as F10 does on the 5250 displays; the service
//! keeps one interval per display, not one per user.

use navigator::env::perf::{DiskActivity, JobActivity, Perf, PoolActivity, SysActivity, UnitIo};
use stg::Client;
use wm::job::wrkactjob_type;

use super::Link;
use super::parse;
use crate::work::web::bridge::Main;

/// Performance.
pub struct LivePerf {
    /// The job's thread.
    pub link: Link,
}

/// `(variable, value)` pairs.
pub type Pairs = Vec<(String, String)>;

/// A variable's value, blank when it is not there.
pub fn val<'a>(row: &'a [(String, String)], key: &str) -> &'a str {
    row.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
}

/// A `panel2_data` display's variables and rows, empty when refused.
pub fn panel(m: &Main, what: &str, args: &[&str]) -> (Pairs, Vec<Pairs>) {
    let (w, a) = (what.to_string(), args.iter().map(ToString::to_string).collect());
    m.ask(|l| l.panel2_data(w, a)).unwrap_or_default()
}

fn num<T: std::str::FromStr + Default>(text: &str) -> T {
    text.trim().parse().unwrap_or_default()
}

/// Megabytes from `12.50`.
fn mb(text: &str) -> u64 {
    parse::size(&format!("{} M", text.trim())) >> 20
}

fn system(m: &Main) -> SysActivity {
    let (vars, rows) = panel(m, "SYSACT", &[]);
    let active = m.ask(Client::active_jobs);
    let kind = |n: u32| {
        active
            .iter()
            .find(|j| j.number == n)
            .and_then(|j| j.job_type().map(|t| wrkactjob_type(t, j.sub_type()).trim().to_string()))
            .unwrap_or_default()
    };
    let jobs: Vec<JobActivity> = rows
        .iter()
        .map(|r| {
            let number = val(r, "NUMBER").trim().to_string();
            JobActivity {
                kind: kind(num(&number)),
                number,
                user: val(r, "USER").trim().to_string(),
                name: val(r, "JOB").trim().to_string(),
                cpu: parse::tenths(val(r, "CPU")),
                io: num::<u64>(val(r, "SYNCIO")) + num::<u64>(val(r, "ASYNCIO")),
                transactions: 0,
            }
        })
        .collect();
    let interactive = jobs.iter().filter(|j| j.kind == "INT").map(|j| j.cpu).sum();
    let (_, pools) = panel(m, "SYSSTS", &["*YES"]);
    let pools = pools
        .iter()
        .map(|p| {
            let act = val(p, "MAXACT").trim();
            PoolActivity {
                number: num(val(p, "POOL")),
                name: val(p, "POOLNAME").trim().to_string(),
                size_mb: mb(val(p, "SIZE")),
                reserved_mb: mb(val(p, "RESV")),
                activity: act.parse().ok(),
                db_faults: parse::tenths(val(p, "DBF")),
                other_faults: parse::tenths(val(p, "NDBF")),
                act_wait: num(val(p, "AW")),
                wait_inel: num(val(p, "WI")),
                act_inel: num(val(p, "AI")),
            }
        })
        .collect();
    SysActivity {
        elapsed: parse::hms(val(&vars, "ELAPSED")),
        cpu: parse::tenths(val(&vars, "OVERALL")),
        cpu_db: 0,
        interactive,
        overhead: 0,
        jobs,
        pools,
    }
}

/// KB a second from requests a second and KB a request.
fn kb_per_s(rqs: &str, per: &str) -> u64 {
    let v = |t: &str| t.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped
    let r = (v(rqs) * v(per)).round().max(0.0) as u64;
    r
}

fn disks(m: &Main) -> DiskActivity {
    let (vars, rows) = panel(m, "DSKSTS", &["*YES"]);
    let names = super::config::disk_names(m);
    let units = rows
        .iter()
        .map(|r| {
            let unit = num(val(r, "UNIT"));
            UnitIo {
                unit,
                resource: super::config::unit_resource(&names, val(r, "SERIAL").trim(), unit),
                asp: 1,
                busy: parse::tenths(val(r, "BUSY")),
                reads: parse::tenths(val(r, "READRQS")),
                writes: parse::tenths(val(r, "WRITERQS")),
                read_kb: kb_per_s(val(r, "READRQS"), val(r, "READK")),
                write_kb: kb_per_s(val(r, "WRITERQS"), val(r, "WRITEK")),
                ..UnitIo::default()
            }
        })
        .collect();
    DiskActivity {
        elapsed: parse::hms(val(&vars, "ELAPSED")),
        units,
    }
}

impl Perf for LivePerf {
    fn system(&self, user: &str) -> SysActivity {
        self.link.read(user, |m| system(m))
    }

    fn disks(&self, user: &str) -> DiskActivity {
        self.link.read(user, |m| disks(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures() {
        let row = vec![("A".to_string(), " 1 ".to_string())];
        assert_eq!(val(&row, "A"), " 1 ");
        assert_eq!(val(&row, "B"), "");
        assert_eq!(mb("12.50"), 12);
        assert_eq!(kb_per_s("2.0", "4.5"), 9);
        assert_eq!(num::<u32>(" 7 "), 7);
    }
}

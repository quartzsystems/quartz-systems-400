// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Performance figures: samples of the machine's CPU, each job's
//! process, each pool's cgroup and each disk unit, and the displays
//! that compare two samples. `WRKACTJOB` and `WRKSYSACT` measure from
//! their previous use, `WRKSYSSTS` and `WRKDSKSTS` from their first use
//! or the last `RESET(*YES)`. `QWCRSSTS` answers from the same figures.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Instant;

use wm::job::wrkactjob_type;
use wm::stats::{self, CpuTimes, DiskStat, Faults, ProcStat, hms, percent};
use wm::table::JobInfo;

use crate::ipl::system::{System, now_unix};

/// One look at the machine.
#[derive(Clone, Debug)]
pub struct Sample {
    at: Instant,
    cpu: CpuTimes,
    procs: BTreeMap<u32, ProcStat>,
    pools: BTreeMap<String, Faults>,
    counters: BTreeMap<String, (u64, u64, u64)>,
    disks: Vec<DiskStat>,
    aux: BTreeMap<u32, u64>,
}

/// The bases the displays measure from, and what the service counts.
#[derive(Debug, Default)]
pub struct Stats {
    actjob: Option<Sample>,
    sysact: Option<Sample>,
    syssts: Option<Sample>,
    dsksts: Option<Sample>,
    /// Storage requests each job made: its auxiliary I/O.
    pub aux_io: BTreeMap<u32, u64>,
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// A pool's cgroup directory.
fn pool_dir(pool: &str) -> String {
    let mut d = String::from("/sys/fs/cgroup/qs400");
    for part in pool.trim_start_matches('*').split('/') {
        d.push('/');
        d.push_str(part);
    }
    d
}

/// The block device behind a unit path: `/dev/vda3` is `vda3`.
fn block_name(dev: &std::path::Path) -> String {
    dev.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn disk_paths(system: &System) -> Vec<String> {
    system
        .storage
        .as_ref()
        .map(|s| {
            s.devices
                .iter()
                .map(|d| format!("/sys/class/block/{}/stat", block_name(d)))
                .collect()
        })
        .unwrap_or_default()
}

/// Look now.
pub fn sample(system: &System) -> Sample {
    let mut procs = BTreeMap::new();
    for j in system.jobs.all() {
        if let Some(p) = read(&format!("/proc/{}/stat", j.pid)).and_then(|t| stats::proc_stat(&t)) {
            procs.insert(j.number, p);
        }
    }
    let mut pools = BTreeMap::new();
    let mut counters = BTreeMap::new();
    for p in system.pools.info() {
        let f = read(&format!("{}/memory.stat", pool_dir(&p.name)))
            .map(|t| stats::memory_stat(&t))
            .unwrap_or_default();
        pools.insert(p.name.clone(), f);
        counters.insert(p.name.clone(), (p.act_wait, p.wait_inel, p.act_inel));
    }
    Sample {
        at: Instant::now(),
        cpu: read("/proc/stat")
            .and_then(|t| stats::cpu_times(&t))
            .unwrap_or_default(),
        procs,
        pools,
        counters,
        disks: disk_paths(system)
            .iter()
            .map(|p| {
                read(p)
                    .and_then(|t| stats::block_stat(&t))
                    .unwrap_or_default()
            })
            .collect(),
        aux: system.stats.aux_io.clone(),
    }
}

/// The interval between two samples.
struct Delta<'a> {
    now: &'a Sample,
    base: &'a Sample,
}

impl Delta<'_> {
    fn secs(&self) -> f64 {
        self.now
            .at
            .duration_since(self.base.at)
            .as_secs_f64()
            .max(0.001)
    }

    fn whole_secs(&self) -> u64 {
        self.now.at.duration_since(self.base.at).as_secs()
    }

    fn ticks(&self) -> u64 {
        self.now.cpu.total.saturating_sub(self.base.cpu.total)
    }

    /// The machine's CPU use, percent.
    fn cpu_used(&self) -> f64 {
        let busy = self
            .ticks()
            .saturating_sub(self.now.cpu.idle.saturating_sub(self.base.cpu.idle));
        percent(busy, self.ticks())
    }

    /// A job's CPU ticks in the interval.
    fn job_ticks(&self, n: u32) -> u64 {
        let now = self.now.procs.get(&n).map_or(0, ProcStat::cpu);
        let then = self.base.procs.get(&n).map_or(0, ProcStat::cpu);
        now.saturating_sub(then)
    }

    fn job_aux(&self, n: u32) -> u64 {
        let now = self.now.aux.get(&n).copied().unwrap_or(0);
        now.saturating_sub(self.base.aux.get(&n).copied().unwrap_or(0))
    }

    /// Faults per second, one decimal: (major, minor).
    #[allow(clippy::cast_precision_loss)] // counters far below 2^52
    fn faults(&self, pool: &str) -> (f64, f64) {
        let n = self.now.pools.get(pool).copied().unwrap_or_default();
        let b = self.base.pools.get(pool).copied().unwrap_or_default();
        let s = self.secs();
        let r = |x: u64| (x as f64 / s * 10.0).round() / 10.0;
        (
            r(n.major.saturating_sub(b.major)),
            r(n.minor.saturating_sub(b.minor)),
        )
    }

    fn counters(&self, pool: &str) -> (u64, u64, u64) {
        let n = self.now.counters.get(pool).copied().unwrap_or_default();
        let b = self.base.counters.get(pool).copied().unwrap_or_default();
        (
            n.0.saturating_sub(b.0),
            n.1.saturating_sub(b.1),
            n.2.saturating_sub(b.2),
        )
    }
}

/// Take a sample for a display: the interval from its base, which then
/// moves to now when `advance`.
fn interval(
    system: &mut System,
    which: fn(&mut Stats) -> &mut Option<Sample>,
    advance: bool,
) -> (Sample, Sample) {
    let now = sample(system);
    let slot = which(&mut system.stats);
    let base = slot.clone().unwrap_or_else(|| now.clone());
    if advance || slot.is_none() {
        *slot = Some(now.clone());
    }
    (now, base)
}

fn header(title: &str) -> String {
    let (date, time) = wm::joblog::stamp_of(now_unix().saturating_mul(1_000_000));
    format!("{:^66}QS400\n{:>58}  {} UTC\n", title, date, &time[..8])
}

fn job_type(j: &JobInfo) -> &'static str {
    j.job_type()
        .map_or("   ", |t| wrkactjob_type(t, j.sub_type()))
}

/// The jobs in `WRKACTJOB` order: subsystems and jobs outside them
/// alphabetically, each subsystem's jobs after it, indented.
fn tree(system: &System) -> Vec<(bool, JobInfo)> {
    let jobs = system.jobs.all();
    let mut top: Vec<&JobInfo> = jobs
        .iter()
        .filter(|j| j.kind == 'M' || j.subsystem.is_empty())
        .collect();
    top.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::new();
    for t in top {
        out.push((false, t.clone()));
        if t.kind == 'M' {
            let mut kids: Vec<&JobInfo> = jobs
                .iter()
                .filter(|j| j.kind != 'M' && j.subsystem == t.name)
                .collect();
            kids.sort_by(|a, b| a.name.cmp(&b.name));
            out.extend(kids.into_iter().map(|k| (true, k.clone())));
        }
    }
    // Jobs whose subsystem is gone still show, at the end.
    for j in &jobs {
        if !out.iter().any(|(_, o)| o.number == j.number) {
            out.push((true, j.clone()));
        }
    }
    out
}

/// `WRKACTJOB [VIEW(1|2|3)] [RESET(*YES)]`: 1 status, 2 elapsed data,
/// 3 job numbers, threads and storage.
pub fn wrkactjob(system: &mut System, view: &str) -> String {
    let (now, base) = interval(system, |s| &mut s.actjob, true);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let rows = tree(system);
    let mut out = header("Work with Active Jobs");
    let _ = writeln!(
        out,
        "CPU %: {:>6.1}     Elapsed time:   {}     Active jobs: {:>5}\n",
        d.cpu_used(),
        hms(d.whole_secs()),
        rows.len()
    );
    let ticks = d.ticks();
    let page_kb = 4u64;
    match view {
        "2" => {
            out.push_str("Opt  Subsystem/Job  Type  Pool  Pty      CPU  Int    Rsp  AuxIO  CPU %\n");
        }
        "3" => out.push_str(
            "Opt  Subsystem/Job  User        Number  Type  Threads  Temp Stg (M)  Status\n",
        ),
        _ => out.push_str("Opt  Subsystem/Job  User        Type  CPU %  Function        Status\n"),
    }
    for (kid, j) in &rows {
        let name = if *kid {
            format!("  {}", j.name)
        } else {
            j.name.clone()
        };
        let pct = percent(d.job_ticks(j.number), ticks);
        let p = now.procs.get(&j.number).copied().unwrap_or_default();
        match view {
            "2" => {
                let pool = system.pools.get(&j.pool).map_or(0, |p| p.number);
                let pty = system.attrs.get(&j.number).map_or(0, |a| a.runpty);
                #[allow(clippy::cast_precision_loss)] // ticks far below 2^52
                let cpu_s = p.cpu() as f64 / 100.0;
                let _ = writeln!(
                    out,
                    "     {name:<14} {:<4}  {pool:>4}  {pty:>3}  {cpu_s:>7.1}  {:>3}  {:>5}  {:>5}  {pct:>5.1}",
                    job_type(j),
                    0,
                    ".0",
                    d.job_aux(j.number)
                );
            }
            "3" => {
                let _ = writeln!(
                    out,
                    "     {name:<14} {:<10}  {:06}  {:<4}  {:>7}  {:>12}  {}",
                    j.user,
                    j.number,
                    job_type(j),
                    p.threads.max(1),
                    p.rss_pages * page_kb / 1024,
                    j.job_status().code()
                );
            }
            _ => {
                let _ = writeln!(
                    out,
                    "     {name:<14} {:<10}  {:<4}  {pct:>5.1}  {:<14}  {}",
                    j.user,
                    job_type(j),
                    j.function,
                    j.job_status().code()
                );
            }
        }
    }
    out
}

/// `WRKSYSACT`: the jobs that used the most CPU since its last use.
pub fn wrksysact(system: &mut System) -> String {
    let (now, base) = interval(system, |s| &mut s.sysact, true);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let ticks = d.ticks();
    let mut jobs = system.jobs.all();
    jobs.sort_by_key(|j| std::cmp::Reverse(d.job_ticks(j.number)));
    let mut out = header("Work with System Activity");
    let _ = writeln!(
        out,
        "Elapsed time:   {}     Overall CPU util: {:>5.1}\n",
        hms(d.whole_secs()),
        d.cpu_used()
    );
    out.push_str("Job or Task  User        Number  Thread    Pty  CPU Util  AuxIO\n");
    for j in jobs.iter().take(20) {
        let pty = system.attrs.get(&j.number).map_or(0, |a| a.runpty);
        let _ = writeln!(
            out,
            "{:<12} {:<10}  {:06}  00000001  {pty:>3}  {:>8.1}  {:>5}",
            j.name,
            j.user,
            j.number,
            percent(d.job_ticks(j.number), ticks),
            d.job_aux(j.number)
        );
    }
    out
}

/// What `WRKSYSSTS` shows of the auxiliary storage: system ASP MB, %
/// used, and the temporary ASP MB.
fn asp_figures(system: &System) -> (u64, f64, u64) {
    let Some(s) = system.storage.as_ref() else {
        return (0, 0.0, 0);
    };
    let a = s.store.asp();
    let total = a.layout().total_pages();
    let used = total.saturating_sub(a.total_free());
    let temp = system.temp.as_ref().map_or(0, |_| 64);
    ((total * 4096) >> 20, percent(used, total), temp)
}

/// `WRKSYSSTS [RESET(*YES)]`.
pub fn wrksyssts(system: &mut System, reset: bool) -> String {
    let (now, base) = interval(system, |s| &mut s.syssts, reset);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let (asp_mb, asp_pct, temp_mb) = asp_figures(system);
    let jobs = system.jobs.all().len();
    let main = read("/proc/meminfo")
        .and_then(|t| stats::mem_total_kb(&t))
        .unwrap_or(0);
    let mut out = header("Work with System Status");
    let _ = writeln!(
        out,
        "% CPU used . . . . . . . : {:>10.1}    Auxiliary storage:",
        d.cpu_used()
    );
    let _ = writeln!(
        out,
        "Elapsed time . . . . . . : {:>10}      System ASP . . . . . . : {asp_mb:>7} M",
        hms(d.whole_secs())
    );
    let _ = writeln!(
        out,
        "Jobs in system . . . . . : {jobs:>10}      % system ASP used  . . : {asp_pct:>7.1}"
    );
    let _ = writeln!(
        out,
        "% perm addresses . . . . : {:>10}      Total  . . . . . . . . : {asp_mb:>7} M",
        ".007"
    );
    let _ = writeln!(
        out,
        "% temp addresses . . . . : {:>10}      Current unprotect used : {temp_mb:>7} M",
        ".012"
    );
    let _ = writeln!(
        out,
        "Main storage size  . . . : {main:>8} K      Maximum unprotect  . . : {temp_mb:>7} M\n"
    );
    out.push_str(
        "System     Pool  Reserved    Max  -----DB-----  ---Non-DB---  Act-  Wait-  Act-\n",
    );
    out.push_str(
        " Pool    Size M    Size M Active  Fault  Pages  Fault  Pages  Wait   Inel  Inel\n",
    );
    for p in system.pools.info() {
        let (db, ndb) = d.faults(&p.name);
        let (aw, wi, ai) = d.counters(&p.name);
        let _ = writeln!(
            out,
            "{:>5}  {:>8.2}  {:>8.2}  {:>5}  {db:>5.1}  {db:>5.1}  {ndb:>5.1}  {ndb:>5.1}  {aw:>4}  {wi:>5}  {ai:>4}",
            p.number,
            mb(p.size_kb),
            mb(p.reserved_kb),
            if p.actlvl == 0 {
                "+++++".to_string()
            } else {
                p.actlvl.to_string()
            },
        );
    }
    out
}

#[allow(clippy::cast_precision_loss)] // sizes far below 2^52
fn mb(kb: u64) -> f64 {
    kb as f64 / 1024.0
}

/// `WRKDSKSTS [RESET(*YES)]`.
#[allow(clippy::cast_precision_loss)] // counters far below 2^52
pub fn wrkdsksts(system: &mut System, reset: bool) -> String {
    let (now, base) = interval(system, |s| &mut s.dsksts, reset);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let secs = d.secs();
    let ms = u64::try_from(d.now.at.duration_since(d.base.at).as_millis()).unwrap_or(u64::MAX);
    let units = system
        .storage
        .as_ref()
        .map(|s| s.store.units())
        .unwrap_or_default();
    let mut out = header("Work with Disk Status");
    let _ = writeln!(out, "Elapsed time:   {}\n", hms(d.whole_secs()));
    out.push_str("              Size    %     I/O   Request   Read  Write   Read   Write    %\n");
    out.push_str("Unit  Type     (M)  Used    Rqs  Size (K)    Rqs    Rqs    (K)     (K)  Busy\n");
    for (i, u) in units.iter().enumerate() {
        let now_d = now.disks.get(i).copied().unwrap_or_default();
        let base_d = base.disks.get(i).copied().unwrap_or_default();
        let x = now_d.since(&base_d);
        let rqs = x.reads + x.writes;
        let per_s = |n: u64| (n as f64 / secs * 10.0).round() / 10.0;
        let req_kb = if rqs == 0 {
            0.0
        } else {
            (x.read_sectors + x.write_sectors) as f64 / 2.0 / rqs as f64
        };
        let _ = writeln!(
            out,
            "{:>4}  VIRT  {:>6}  {:>4.1}  {:>5.1}  {req_kb:>8.1}  {:>5.1}  {:>5.1}  {:>5.1}  {:>6.1}  {:>4}",
            u.unit + 1,
            (u.entry.pages * 4096) >> 20,
            percent(u.used_pages, u.entry.usable_pages.max(1)),
            per_s(rqs),
            per_s(x.reads),
            per_s(x.writes),
            if x.reads == 0 {
                0.0
            } else {
                x.read_sectors as f64 / 2.0 / x.reads as f64
            },
            if x.writes == 0 {
                0.0
            } else {
                x.write_sectors as f64 / 2.0 / x.writes as f64
            },
            percent(x.busy_ms, ms)
        );
    }
    out
}

fn be4(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&u32::try_from(v).unwrap_or(u32::MAX).to_be_bytes());
}

fn text(out: &mut Vec<u8>, s: &str, n: usize) {
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
        .collect();
    b.resize(n, 0x40);
    out.extend(b);
}

/// The `*DTS` stamp and system name every format starts with, after the
/// two lengths.
fn common(out: &mut Vec<u8>) {
    let micros = now_unix().saturating_mul(1_000_000);
    out.extend_from_slice(&micros.to_be_bytes());
    text(out, "QS400", 8);
}

/// `QWCRSSTS`: the receiver of `format` (`SSTS0100` to `SSTS0500`),
/// statistics from the last reset (`*YES` resets them).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // rounded tenths
pub fn qwcrssts(system: &mut System, format: &str, reset: bool) -> Result<Vec<u8>, String> {
    let (now, base) = interval(system, |s| &mut s.syssts, reset);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let jobs = system.jobs.all();
    let mut out = vec![0u8; 8];
    common(&mut out);
    match format {
        "SSTS0100" => {
            let count = |f: &dyn Fn(&JobInfo) -> bool| jobs.iter().filter(|j| f(j)).count() as u64;
            be4(&mut out, count(&|j| j.kind == 'I'));
            for _ in 0..4 {
                be4(&mut out, 0);
            }
            be4(
                &mut out,
                count(&|j| j.kind == 'B' && j.job_status().code() == "MSGW"),
            );
            be4(&mut out, count(&|j| j.kind == 'B'));
            for _ in 0..2 {
                be4(&mut out, 0);
            }
            be4(&mut out, system.queued.len() as u64);
            for _ in 0..4 {
                be4(&mut out, 0);
            }
        }
        "SSTS0200" => {
            let (asp_mb, asp_pct, temp_mb) = asp_figures(system);
            let main = read("/proc/meminfo")
                .and_then(|t| stats::mem_total_kb(&t))
                .unwrap_or(0);
            text(&mut out, &hms(d.whole_secs()).replace(':', ""), 6);
            out.push(if system.sbs.is_empty() { 0xF1 } else { 0xF0 });
            out.push(0);
            be4(&mut out, (d.cpu_used() * 10.0).round() as u64);
            be4(&mut out, jobs.len() as u64);
            be4(&mut out, 70);
            be4(&mut out, 120);
            be4(&mut out, asp_mb);
            be4(&mut out, (asp_pct * 10_000.0).round() as u64 / 100);
            be4(&mut out, asp_mb);
            be4(&mut out, temp_mb);
            be4(&mut out, temp_mb);
            be4(&mut out, 0);
            be4(&mut out, main);
            be4(&mut out, 1);
            be4(&mut out, 1);
            be4(&mut out, 0);
            be4(&mut out, u64::from(now.cpu.cpus) * 100);
            out.extend_from_slice(&[0, 0, 0, 0]);
            be4(&mut out, u64::from(now.cpu.cpus));
            be4(&mut out, jobs.len() as u64);
            let threads: u64 = now.procs.values().map(|p| u64::from(p.threads)).sum();
            be4(&mut out, threads);
            be4(&mut out, 163_520);
        }
        "SSTS0300" | "SSTS0400" | "SSTS0500" => {
            text(&mut out, &hms(d.whole_secs()).replace(':', ""), 6);
            out.extend_from_slice(&[0, 0]);
            let pools = system.pools.info();
            let entry = if format == "SSTS0300" { 84 } else { 104 };
            be4(&mut out, pools.len() as u64);
            be4(&mut out, 44);
            be4(&mut out, entry);
            for p in &pools {
                let (db, ndb) = d.faults(&p.name);
                let (aw, wi, ai) = d.counters(&p.name);
                be4(&mut out, u64::from(p.number));
                be4(&mut out, p.size_kb);
                be4(&mut out, p.reserved_kb);
                be4(&mut out, u64::from(p.actlvl));
                for v in [db, db, ndb, ndb] {
                    be4(&mut out, (v * 10.0).round() as u64);
                }
                be4(&mut out, aw);
                be4(&mut out, wi);
                be4(&mut out, ai);
                text(&mut out, &p.name, 10);
                let sbs = p.name.split_once('/').map_or("", |(s, _)| s);
                text(&mut out, sbs, 10);
                text(&mut out, if sbs.is_empty() { "" } else { "QSYS" }, 10);
                text(&mut out, "*FIXED", 10);
                if format != "SSTS0300" {
                    be4(&mut out, u64::from(p.active));
                    be4(&mut out, u64::from(p.ineligible));
                    be4(&mut out, p.size_kb);
                    be4(&mut out, u64::from(p.actlvl));
                    be4(&mut out, 0);
                }
            }
        }
        f => return Err(format!("CPF3C21 format {f} is not valid")),
    }
    let n = out.len() as u64;
    out[..4].copy_from_slice(&u32::try_from(n).unwrap_or(0).to_be_bytes());
    out[4..8].copy_from_slice(&u32::try_from(n).unwrap_or(0).to_be_bytes());
    Ok(out)
}

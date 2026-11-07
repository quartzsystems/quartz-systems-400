// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Performance figures: samples of the machine's CPU, each job's
//! process, each pool's cgroup and each disk unit, and the displays
//! that compare two samples. `WRKACTJOB` and `WRKSYSACT` measure from
//! their previous use, `WRKSYSSTS` and `WRKDSKSTS` from their first use
//! or the last `RESET(*YES)`. `QWCRSSTS` answers from the same figures.
//! Collection Services (`perfcol.rs`) records intervals between the
//! same samples: every display and the collector look through
//! [`sample`], which shares a look taken in the last half second, so
//! figures shown at once agree.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

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
    io: BTreeMap<u32, JobIo>,
    txns: BTreeMap<u32, Txns>,
    lines: Vec<(String, NetStat)>,
    main_kb: u64,
}

/// A job's storage requests by kind, since it started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JobIo {
    /// Database reads (record reads, SQL).
    pub db_reads: u64,
    /// Database writes, updates and deletes.
    pub db_writes: u64,
    /// Full opens.
    pub opens: u64,
}

/// A job's interactive transactions and waits, since it started: a
/// transaction runs from the input a display sends to the job's next
/// wait at the display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Txns {
    /// Transactions.
    pub count: u64,
    /// Their response time, milliseconds.
    pub ms: u64,
    /// Active to wait transitions.
    pub waits: u64,
    /// Times it found no activity level free.
    pub inel: u64,
}

/// An interface's counters, from `/sys/class/net/<if>/statistics`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetStat {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_packets: u64,
    tx_packets: u64,
    rx_errors: u64,
    tx_errors: u64,
    speed_mbps: u64,
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
    /// When each queued job was taken off its queue.
    pub taken: BTreeMap<u32, Instant>,
    /// Jobs started from queues, and their start latency (taken to
    /// hello): count, total and longest, microseconds.
    pub starts: (u64, u128, u128),
    /// Spooled file data written: requests and bytes.
    pub spooled: (u64, u64),
    /// Each job's requests by kind.
    pub io: BTreeMap<u32, JobIo>,
    /// Each job's transactions.
    pub txns: BTreeMap<u32, Txns>,
    /// Jobs in a transaction: since when.
    pub inflight: BTreeMap<u32, Instant>,
    /// The last look, shared by whoever looks within half a second.
    latest: Option<Sample>,
    /// Collection Services.
    pub perf: super::perfcol::Collector,
    /// Job accounting.
    pub acg: super::acg::Acct,
}

impl Stats {
    /// Count request `req` of job `number` by kind.
    pub fn request(&mut self, number: u32, req: &stg::Request) {
        use mi::io::IoOp;
        let io = self.io.entry(number).or_default();
        match req {
            stg::Request::Open { .. } => io.opens += 1,
            stg::Request::Io { req, .. } => match req.op {
                IoOp::Write | IoOp::Update | IoOp::Delete => io.db_writes += 1,
                IoOp::Unlock | IoOp::Feod => {}
                _ => io.db_reads += 1,
            },
            stg::Request::SqlRun { .. } => io.db_reads += 1,
            _ => {}
        }
    }

    /// Job `number` reports a status: at a display (`dspw`), runnable
    /// or waiting, and whether it got an activity level. Input from the
    /// display starts a transaction; the next wait there ends it.
    #[allow(clippy::fn_params_excessive_bools)] // the states a status report carries
    pub fn status(
        &mut self,
        number: u32,
        was_dspw: bool,
        dspw: bool,
        runnable: bool,
        granted: bool,
    ) {
        let t = self.txns.entry(number).or_default();
        if dspw {
            if let Some(at) = self.inflight.remove(&number) {
                t.count += 1;
                t.ms += u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX);
            }
        } else if was_dspw && runnable {
            self.inflight.insert(number, Instant::now());
        }
        if !runnable {
            t.waits += 1;
        } else if !granted {
            t.inel += 1;
        }
    }

    /// Job `number` ended: its counters go.
    pub fn ended(&mut self, number: u32) {
        self.aux_io.remove(&number);
        self.io.remove(&number);
        self.txns.remove(&number);
        self.inflight.remove(&number);
    }

    /// A queued job said hello: its start latency.
    pub fn started(&mut self, number: u32) {
        if let Some(t) = self.taken.remove(&number) {
            let us = t.elapsed().as_micros();
            self.starts.0 += 1;
            self.starts.1 += us;
            self.starts.2 = self.starts.2.max(us);
        }
    }

    /// `wmstats`: what the service counted since it started.
    #[must_use]
    pub fn report(&self) -> String {
        let (n, total, max) = self.starts;
        let avg = if n == 0 { 0 } else { total / u128::from(n) };
        format!(
            "jobs started from queues: {n}, start latency average {:.1} ms, longest {:.1} ms\nspooled data: {} request(s), {} bytes\n",
            ms(avg),
            ms(max),
            self.spooled.0,
            self.spooled.1
        )
    }
}

#[allow(clippy::cast_precision_loss)] // microseconds far below 2^52
fn ms(us: u128) -> f64 {
    us as f64 / 1000.0
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// A pool's cgroup directory.
pub(crate) fn pool_dir(pool: &str) -> String {
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

/// A look shared by the displays and the collector: the last one when
/// it is less than half a second old.
pub fn sample(system: &mut System) -> Sample {
    sample_after(system, None)
}

/// [`sample`], but newer than `base`: an interval never ends where it
/// starts.
pub fn sample_after(system: &mut System, base: Option<&Sample>) -> Sample {
    if let Some(s) = system
        .stats
        .latest
        .as_ref()
        .filter(|s| s.at.elapsed() < Duration::from_millis(500))
        .filter(|s| base.is_none_or(|b| s.at > b.at))
    {
        return s.clone();
    }
    let s = look(system);
    system.stats.latest = Some(s.clone());
    s
}

/// The Ethernet lines varied on, with their interfaces' counters.
fn net_lines() -> Vec<(String, NetStat)> {
    let names: Vec<(String, String)> = super::cmn::lock()
        .state
        .lines()
        .filter(|l| !l.ifname.is_empty())
        .map(|l| (l.name.clone(), l.ifname.clone()))
        .collect();
    names
        .into_iter()
        .map(|(line, ifname)| {
            let n = |f: &str| {
                read(&format!("/sys/class/net/{ifname}/{f}"))
                    .and_then(|t| t.trim().parse::<i64>().ok())
                    .map_or(0, |v| u64::try_from(v).unwrap_or(0))
            };
            let st = NetStat {
                rx_bytes: n("statistics/rx_bytes"),
                tx_bytes: n("statistics/tx_bytes"),
                rx_packets: n("statistics/rx_packets"),
                tx_packets: n("statistics/tx_packets"),
                rx_errors: n("statistics/rx_errors"),
                tx_errors: n("statistics/tx_errors"),
                speed_mbps: n("speed"),
            };
            (line, st)
        })
        .collect()
}

/// Look now.
fn look(system: &System) -> Sample {
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
        io: system.stats.io.clone(),
        txns: system.stats.txns.clone(),
        lines: net_lines(),
        main_kb: read("/proc/meminfo")
            .and_then(|t| stats::mem_total_kb(&t))
            .unwrap_or(0),
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

    /// A job's requests by kind in the interval.
    fn job_io(&self, n: u32) -> JobIo {
        let now = self.now.io.get(&n).copied().unwrap_or_default();
        let then = self.base.io.get(&n).copied().unwrap_or_default();
        JobIo {
            db_reads: now.db_reads.saturating_sub(then.db_reads),
            db_writes: now.db_writes.saturating_sub(then.db_writes),
            opens: now.opens.saturating_sub(then.opens),
        }
    }

    /// A job's transactions in the interval and their average response
    /// time, tenths of a second.
    fn job_txns(&self, n: u32) -> (u64, u64) {
        let now = self.now.txns.get(&n).copied().unwrap_or_default();
        let then = self.base.txns.get(&n).copied().unwrap_or_default();
        let count = now.count.saturating_sub(then.count);
        let ms = now.ms.saturating_sub(then.ms);
        (count, ms.checked_div(count).unwrap_or(0) / 100)
    }

    /// Tenths of a second a job did not run in the interval.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a short interval in tenths
    fn wait_tenths(&self, n: u32) -> u64 {
        let all = (self.secs() * 10.0).round().max(0.0) as u64;
        all.saturating_sub(self.job_ticks(n) / 10)
    }

    /// Pages a job's resident set grew and shrank by.
    fn job_rss(&self, n: u32) -> (u64, u64) {
        let now = self.now.procs.get(&n).map_or(0, |p| p.rss_pages);
        let then = self.base.procs.get(&n).map_or(now, |p| p.rss_pages);
        (now.saturating_sub(then), then.saturating_sub(now))
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
    let base = which(&mut system.stats).clone();
    let now = sample_after(system, base.as_ref());
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
            out.push_str(
                "Opt  Subsystem/Job  Type  Pool  Pty      CPU  Int    Rsp  AuxIO  CPU %\n",
            );
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

/// A percentage in tenths.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0 to 1000
fn tenths(pct: f64) -> u32 {
    (pct * 10.0).round().max(0.0) as u32
}

/// `WRKACTJOB`'s panel: every view's figures for the jobs in display
/// order, measured from the display's base, which moves to now when
/// `advance` (the panel's start and F10).
pub fn actjob_data(system: &mut System, advance: bool) -> stg::proto::ActJobs {
    let (now, base) = interval(system, |s| &mut s.actjob, advance);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let ticks = d.ticks();
    let jobs = tree(system)
        .into_iter()
        .map(|(under, j)| {
            let p = now.procs.get(&j.number).copied().unwrap_or_default();
            stg::proto::ActJob {
                under,
                cpu: tenths(percent(d.job_ticks(j.number), ticks)),
                // Clock ticks are hundredths of a second.
                cpu_secs: p.cpu() / 10,
                pool: system.pools.get(&j.pool).map_or(0, |p| p.number),
                runpty: system.attrs.get(&j.number).map_or(0, |a| a.runpty),
                aux_io: d.job_aux(j.number),
                threads: p.threads.max(1),
                tmp_mb: p.rss_pages * 4 / 1024,
                txns: d.job_txns(j.number).0,
                rsp: d.job_txns(j.number).1,
                job: j,
            }
        })
        .collect();
    stg::proto::ActJobs {
        cpu: tenths(d.cpu_used()),
        elapsed: d.whole_secs(),
        jobs,
    }
}

/// `DSPJOB`'s figures for one job: CPU milliseconds, threads,
/// temporary storage (M) and auxiliary I/O.
pub fn job_figures(system: &System, number: u32) -> (u64, u32, u64, u64) {
    let pid = system.jobs.get(number).map_or(0, |j| j.pid);
    let p = read(&format!("/proc/{pid}/stat"))
        .and_then(|t| stats::proc_stat(&t))
        .unwrap_or_default();
    let aux = system.stats.aux_io.get(&number).copied().unwrap_or(0);
    (p.cpu() * 10, p.threads.max(1), p.rss_pages * 4 / 1024, aux)
}

/// `WRKSYSACT` is Performance Tools' (`QS00PT1`): an error when the
/// product is not installed.
pub fn performance_tools(system: &mut System) -> Result<(), String> {
    if super::licpgm::installed(system, "QS00PT1", licpgm::OptionNo::BASE) {
        Ok(())
    } else {
        Err(format!(
            "{} product QS00PT1 option *BASE is not installed",
            crate::cpp::lpcmd::NOT_INSTALLED
        ))
    }
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
/// used, and the temporary ASP MB. The IFS root's filesystem counts in
/// the system ASP, where IBM keeps the IFS (`docs/IFS.md`); pages
/// device parity holds count in neither figure (`units.rs`).
fn asp_figures(system: &System) -> (u64, f64, u64) {
    if system.storage.is_none() {
        return (0, 0.0, 0);
    }
    let (total, used) = asp_bytes(system);
    let temp = system.temp.as_ref().map_or(0, |_| 64);
    (total >> 20, exact_percent(used, total), temp)
}

/// The system ASP's capacity and the bytes in use, as `WRKSYSSTS`
/// counts them; what the threshold and `QSTGLOWLMT` are measured on.
pub fn asp_bytes(system: &System) -> (u64, u64) {
    let Some(s) = system.storage.as_ref() else {
        return (0, 0);
    };
    let a = s.store.asp();
    let (ifs_size, ifs_used) = super::ifs::partition_capacity(system).unwrap_or((0, 0));
    let held = super::units::parity_held(system);
    let pages = a.layout().total_pages().saturating_sub(held);
    let total = pages + ifs_size / 4096;
    let used = a
        .layout()
        .total_pages()
        .saturating_sub(a.total_free())
        .saturating_sub(held)
        + ifs_used / 4096;
    (total * 4096, used * 4096)
}

/// `part` of `whole` in percent, unrounded: a fresh system's few
/// hundred MiB of a few TiB is far below a tenth of a percent.
#[allow(clippy::cast_precision_loss)] // pages far below 2^52
fn exact_percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
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
        "Jobs in system . . . . . : {jobs:>10}      % system ASP used  . . : {asp_pct:>7.4}"
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
    let units = super::units::views(system);
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
            "{:>4}  {:<4}  {:>6}  {:>4.1}  {:>5.1}  {req_kb:>8.1}  {:>5.1}  {:>5.1}  {:>5.1}  {:>6.1}  {:>4}",
            u.unit + 1,
            u.type_code.chars().take(4).collect::<String>(),
            (u.pages * 4096) >> 20,
            percent(u.used, u.usable.max(1)),
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

/// One `(variable, value)` pair of a panel's data.
#[allow(clippy::needless_pass_by_value)] // numbers and strings alike
fn pair(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// `WRKSYSSTS` as panel data: the header figures and one entry per
/// pool, measured from the display's base (moved by `reset`).
pub fn syssts_panel(system: &mut System, reset: bool) -> stg::proto::PanelData {
    let (now, base) = interval(system, |s| &mut s.syssts, reset);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let (asp_mb, asp_pct, temp_mb) = asp_figures(system);
    let main = read("/proc/meminfo")
        .and_then(|t| stats::mem_total_kb(&t))
        .unwrap_or(0);
    let gb = |m: u64| format!("{:.1} G", mb(m * 1024) / 1024.0);
    let vars = vec![
        pair("CPUPCT", format!("{:.1}", d.cpu_used())),
        pair("DBCAP", ".0"),
        pair("ELAPSED", hms(d.whole_secs())),
        pair("JOBS", system.jobs.all().len()),
        pair("PERMADR", ".007"),
        pair("TEMPADR", ".012"),
        pair("SYSASP", gb(asp_mb)),
        pair("ASPPCT", format!("{asp_pct:.4}")),
        pair("TOTAUX", gb(asp_mb)),
        pair("CURUNP", format!("{temp_mb} M")),
        pair("MAXUNP", format!("{temp_mb} M")),
        pair("MAINSTG", format!("{:.2}", mb(main))),
    ];
    let rows = system
        .pools
        .info()
        .into_iter()
        .map(|p| {
            let (db, ndb) = d.faults(&p.name);
            let (aw, wi, ai) = d.counters(&p.name);
            vec![
                pair("POOL", p.number),
                pair("POOLNAME", &p.name),
                pair("SIZE", format!("{:.2}", mb(p.size_kb))),
                pair("RESV", format!("{:.2}", mb(p.reserved_kb))),
                pair(
                    "MAXACT",
                    if p.actlvl == 0 {
                        "+++++".to_string()
                    } else {
                        p.actlvl.to_string()
                    },
                ),
                pair("DBF", format!("{db:.1}")),
                pair("DBP", format!("{db:.1}")),
                pair("NDBF", format!("{ndb:.1}")),
                pair("NDBP", format!("{ndb:.1}")),
                pair("AW", aw),
                pair("WI", wi),
                pair("AI", ai),
            ]
        })
        .collect();
    (vars, rows)
}

/// `WRKDSKSTS` as panel data: the elapsed time and one entry per unit.
#[allow(clippy::cast_precision_loss)] // counters far below 2^52
pub fn dsksts_panel(system: &mut System, reset: bool) -> stg::proto::PanelData {
    let (now, base) = interval(system, |s| &mut s.dsksts, reset);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let secs = d.secs();
    let ms = u64::try_from(d.now.at.duration_since(d.base.at).as_millis()).unwrap_or(u64::MAX);
    let units = super::units::views(system);
    let rows = units
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let x = now
                .disks
                .get(i)
                .copied()
                .unwrap_or_default()
                .since(&base.disks.get(i).copied().unwrap_or_default());
            let rqs = x.reads + x.writes;
            let per_s = |n: u64| format!("{:.1}", (n as f64 / secs * 10.0).round() / 10.0);
            let kb = |sectors: u64, n: u64| {
                format!(
                    "{:.1}",
                    if n == 0 {
                        0.0
                    } else {
                        sectors as f64 / 2.0 / n as f64
                    }
                )
            };
            vec![
                pair("UNIT", u.unit + 1),
                pair("TYPE", &u.type_code),
                pair("SIZE", (u.pages * 4096) >> 30),
                pair("USED", format!("{:.1}", percent(u.used, u.usable.max(1)))),
                pair("IORQS", per_s(rqs)),
                pair("RQSSIZE", kb(x.read_sectors + x.write_sectors, rqs)),
                pair("READRQS", per_s(x.reads)),
                pair("WRITERQS", per_s(x.writes)),
                pair("READK", kb(x.read_sectors, x.reads)),
                pair("WRITEK", kb(x.write_sectors, x.writes)),
                pair("BUSY", format!("{:.0}", percent(x.busy_ms, ms))),
                // For Navigator: model, exact size, serial, protection, status.
                pair("MODEL", &u.model),
                pair("SIZEB", u.pages * 4096),
                pair("SERIAL", &u.serial),
                pair("PROT", &u.protection),
                pair("STATUS", &u.status),
            ]
        })
        .collect();
    (vec![pair("ELAPSED", hms(d.whole_secs()))], rows)
}

/// `WRKSYSACT` as panel data: the interval, the machine's CPU and the
/// jobs by CPU used, most first.
pub fn sysact_panel(system: &mut System) -> stg::proto::PanelData {
    let (now, base) = interval(system, |s| &mut s.sysact, true);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let ticks = d.ticks();
    let mut jobs = system.jobs.all();
    jobs.sort_by_key(|j| std::cmp::Reverse(d.job_ticks(j.number)));
    let cpu = format!("{:.1}", d.cpu_used());
    let vars = vec![
        pair("ELAPSED", hms(d.whole_secs())),
        pair("OVERALL", &cpu),
        pair("AVGCPU", &cpu),
        pair("MAXCPU", &cpu),
        pair("MINCPU", &cpu),
        pair(
            "VPROCS",
            std::thread::available_parallelism().map_or(1, std::num::NonZero::get),
        ),
    ];
    let rows = jobs
        .iter()
        .map(|j| {
            let pty = system.attrs.get(&j.number).map_or(0, |a| a.runpty);
            vec![
                pair("JOB", &j.name),
                pair("USER", &j.user),
                pair("NUMBER", format!("{:06}", j.number)),
                pair("THREAD", "00000001"),
                pair("PTY", pty),
                pair(
                    "CPU",
                    format!("{:.1}", percent(d.job_ticks(j.number), ticks)),
                ),
                pair("SYNCIO", 0),
                pair("ASYNCIO", d.job_aux(j.number)),
                pair("CPUDB", ".0"),
                // QPYSYSAC's views.
                pair("CPUT", tenths(percent(d.job_ticks(j.number), ticks))),
                pair("DBR", d.job_io(j.number).db_reads),
                pair("DBW", d.job_io(j.number).db_writes),
                pair(
                    "NDBR",
                    d.job_aux(j.number)
                        .saturating_sub(d.job_io(j.number).db_reads + d.job_io(j.number).db_writes),
                ),
                pair("ALLOC", d.job_rss(j.number).0),
                pair("DEALLOC", d.job_rss(j.number).1),
                pair("WAIT", d.wait_tenths(j.number)),
            ]
        })
        .collect();
    (vars, rows)
}

/// Seconds between two samples, at least one.
#[must_use]
pub fn secs_between(now: &Sample, base: &Sample) -> u32 {
    u32::try_from(now.at.duration_since(base.at).as_secs().max(1)).unwrap_or(u32::MAX)
}

/// The interval from `base` to `now` as Collection Services records
/// it: interval `number`, ending at local time `end`.
#[allow(clippy::too_many_lines)] // one record per category, in order
pub fn interval_data(
    system: &mut System,
    now: &Sample,
    base: &Sample,
    number: u32,
    end: u64,
) -> perf::sample::Interval {
    use perf::sample::{DiskData, Interval, JobData, LineData, PoolData, SysData};
    let d = Delta { now, base };
    let ms = u64::try_from(now.at.duration_since(base.at).as_millis())
        .unwrap_or(u64::MAX)
        .max(1);
    // Clock ticks are hundredths of a second.
    let tick_ms = |t: u64| t * 10;
    let cpus = u64::from(now.cpu.cpus.max(1));
    let busy = d
        .ticks()
        .saturating_sub(now.cpu.idle.saturating_sub(base.cpu.idle));
    let io = |s: &Sample, n: u32| s.io.get(&n).copied().unwrap_or_default();
    let tx = |s: &Sample, n: u32| s.txns.get(&n).copied().unwrap_or_default();
    let mut jobs = Vec::new();
    for j in system.jobs.all() {
        let n = j.number;
        let (i1, i0) = (io(now, n), io(base, n));
        let (t1, t0) = (tx(now, n), tx(base, n));
        let p = now.procs.get(&n).copied().unwrap_or_default();
        let db_reads = i1.db_reads.saturating_sub(i0.db_reads);
        let db_writes = i1.db_writes.saturating_sub(i0.db_writes);
        let opens = i1.opens.saturating_sub(i0.opens);
        jobs.push(JobData {
            kind: j.kind.to_string(),
            sub_type: if j.subtype == ' ' || j.subtype == '\0' {
                String::new()
            } else {
                j.subtype.to_string()
            },
            sbs_lib: if j.subsystem.is_empty() {
                String::new()
            } else {
                "QSYS".into()
            },
            subsystem: j.subsystem.clone(),
            pool: system.pools.get(&j.pool).map_or(2, |p| p.number),
            priority: system.attrs.get(&n).map_or(0, |a| a.runpty),
            cpu_ms: tick_ms(d.job_ticks(n)),
            cpu_total_ms: tick_ms(p.cpu()),
            threads: p.threads.max(1),
            tmp_mb: p.rss_pages * 4 / 1024,
            db_reads,
            db_writes,
            other_io: d.job_aux(n).saturating_sub(db_reads + db_writes + opens),
            opens,
            transactions: t1.count.saturating_sub(t0.count),
            response_ms: t1.ms.saturating_sub(t0.ms),
            aw: t1.waits.saturating_sub(t0.waits),
            wi: t1.inel.saturating_sub(t0.inel),
            ai: 0,
            name: j.name,
            user: j.user,
            number: n,
        });
    }
    let pools = system
        .pools
        .info()
        .into_iter()
        .map(|p| {
            let f1 = now.pools.get(&p.name).copied().unwrap_or_default();
            let f0 = base.pools.get(&p.name).copied().unwrap_or_default();
            let (aw, wi, ai) = d.counters(&p.name);
            let major = f1.major.saturating_sub(f0.major);
            let minor = f1.minor.saturating_sub(f0.minor);
            PoolData {
                number: p.number,
                db_faults: major,
                db_pages: major,
                ndb_faults: minor,
                ndb_pages: minor,
                aw,
                wi,
                ai,
                size_kb: p.size_kb,
                reserved_kb: p.reserved_kb,
                actlvl: p.actlvl,
                active: p.active,
                ineligible: p.ineligible,
                name: p.name,
            }
        })
        .filter(|p| p.size_kb > 0)
        .collect();
    let disks = super::units::views(system)
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let x = now
                .disks
                .get(i)
                .copied()
                .unwrap_or_default()
                .since(&base.disks.get(i).copied().unwrap_or_default());
            DiskData {
                unit: u32::from(u.unit) + 1,
                type_code: u.type_code.clone(),
                model: u.model.clone(),
                resource: format!("DD{:03}", u.unit + 1),
                serial: u.serial.clone(),
                asp: 1,
                capacity: u.usable * 4096,
                available: u.usable.saturating_sub(u.used) * 4096,
                reads: x.reads,
                writes: x.writes,
                read_blocks: x.read_sectors,
                write_blocks: x.write_sectors,
                busy_ms: x.busy_ms,
            }
        })
        .collect();
    let lines = now
        .lines
        .iter()
        .enumerate()
        .map(|(i, (line, st))| {
            let b = base
                .lines
                .iter()
                .find(|(l, _)| l == line)
                .map(|(_, s)| *s)
                .unwrap_or_default();
            LineData {
                line: line.clone(),
                resource: format!("CMN{:02}", i + 1),
                speed: st.speed_mbps * 1_000_000,
                rx_bytes: st.rx_bytes.saturating_sub(b.rx_bytes),
                tx_bytes: st.tx_bytes.saturating_sub(b.tx_bytes),
                rx_frames: st.rx_packets.saturating_sub(b.rx_packets),
                tx_frames: st.tx_packets.saturating_sub(b.tx_packets),
                rx_errors: st.rx_errors.saturating_sub(b.rx_errors),
                tx_errors: st.tx_errors.saturating_sub(b.tx_errors),
            }
        })
        .collect();
    let (asp_mb, asp_pct, _) = asp_figures(system);
    let asp_bytes = asp_mb << 20;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )] // a percentage of a size
    let asp_used = (asp_bytes as f64 * asp_pct / 100.0) as u64;
    let jobs_n = u32::try_from(jobs.len()).unwrap_or(u32::MAX);
    let system_data = SysData {
        cpu_used_ms: tick_ms(busy),
        cpu_avail_ms: ms * cpus,
        cpus: now.cpu.cpus.max(1),
        main_kb: now.main_kb,
        asp_bytes,
        asp_free: asp_bytes.saturating_sub(asp_used),
        jobs: jobs_n,
        threads: now.procs.values().map(|p| p.threads).sum(),
        opens: jobs.iter().map(|j: &JobData| j.opens).sum(),
        db_reads: jobs.iter().map(|j| j.db_reads).sum(),
        db_writes: jobs.iter().map(|j| j.db_writes).sum(),
        transactions: jobs.iter().map(|j| j.transactions).sum(),
        response_ms: jobs.iter().map(|j| j.response_ms).sum(),
    };
    Interval {
        number,
        end,
        secs: secs_between(now, base),
        system: system_data,
        jobs,
        pools,
        disks,
        lines,
    }
}

/// `QSYS2.SYSTEM_STATUS_INFO`: `WRKSYSSTS`'s figures over its interval,
/// which stays where the display left it.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // counts far below 2^32
pub fn sql_status(system: &mut System) -> sql::exec::catalog::services::host::SystemStatus {
    let (now, base) = interval(system, |s| &mut s.syssts, false);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let (asp_mb, asp_pct, temp_mb) = asp_figures(system);
    let jobs = system.jobs.all();
    let sv = |n: &str| crate::stgsvc::security::sysval_text(system, n);
    let max_jobs: u32 = sv("QMAXJOB").trim().parse().unwrap_or(0);
    let interactive = jobs.iter().filter(|j| j.kind == 'I').count() as u32;
    let batch = |code: &str| {
        jobs.iter()
            .filter(|j| j.kind == 'B' && j.job_status().code() == code)
            .count() as u32
    };
    let running = jobs.iter().filter(|j| j.kind == 'B').count() as u32;
    let threads = now.procs.values().map(|p| p.threads.max(1)).sum();
    let ipl_mode = if system.mode == licpart::IplMode::Manual {
        "MANUAL"
    } else {
        "NORMAL"
    };
    sql::exec::catalog::services::host::SystemStatus {
        total_jobs: (jobs.len() + system.queued.len() + system.joblog_pending.len()) as u32,
        max_jobs,
        active_jobs: jobs.len() as u32,
        interactive_pct: if max_jobs == 0 {
            0.0
        } else {
            f64::from(interactive) * 100.0 / f64::from(max_jobs)
        },
        signed_on: interactive,
        batch: [
            running,
            system.queued.len() as u32,
            0,
            batch("MSGW"),
            0,
            0,
            0,
            0,
        ],
        elapsed_ms: d.whole_secs() * 1000,
        cpu_pct: d.cpu_used(),
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
        main_storage_kb: now.main_kb,
        asp_mb,
        asp_used_pct: asp_pct,
        temp_mb,
        temp_max_mb: temp_mb,
        threads,
        restricted: system.sbs.is_empty(),
        host_name: read("/proc/sys/kernel/hostname")
            .map(|h| h.trim().to_string())
            .unwrap_or_default(),
        machine: (
            String::new(),
            sv("QMODEL").trim().to_string(),
            sv("QSRLNBR").trim().to_string(),
        ),
        ipl_mode: ipl_mode.into(),
        ipl_type: sv("QIPLTYPE").trim().to_string(),
        table: [
            system.queued.len() as u32,
            0,
            system.joblog_pending.len() as u32,
        ],
        scheduler_active: jobs.iter().any(|j| j.name == "QEJSCHED"),
    }
}

/// The storage pools as `MEMORY_POOL_INFO` shows them, over the
/// `WRKSYSSTS` interval.
#[allow(clippy::cast_precision_loss)] // counters far below 2^52
pub fn sql_pools(system: &mut System) -> Vec<sql::exec::catalog::services::host::MemoryPool> {
    let (now, base) = interval(system, |s| &mut s.syssts, false);
    let d = Delta {
        now: &now,
        base: &base,
    };
    let per_min = |n: u64| n as f64 * 60.0 / d.secs();
    system
        .pools
        .info()
        .into_iter()
        .map(|p| {
            let (db, ndb) = d.faults(&p.name);
            let (aw, wi, ai) = d.counters(&p.name);
            let subsystem = p.name.split_once('/').map(|(sbs, _)| {
                (
                    system
                        .sbs
                        .get(sbs)
                        .map_or_else(String::new, |a| a.lib.clone()),
                    sbs.to_string(),
                )
            });
            sql::exec::catalog::services::host::MemoryPool {
                number: p.number,
                name: p.name.clone(),
                subsystem,
                size_kb: p.size_kb,
                reserved_kb: p.reserved_kb,
                max_active: p.actlvl,
                active: p.active,
                ineligible: p.ineligible,
                elapsed_ms: d.whole_secs() * 1000,
                db_faults: db,
                ndb_faults: ndb,
                // As WRKSYSSTS shows them: a fault is a page read.
                db_pages: db,
                ndb_pages: ndb,
                transitions: [per_min(aw), per_min(wi), per_min(ai)],
            }
        })
        .collect()
}

/// The disk units as `SYSDISKSTAT` shows them, over the `WRKDSKSTS`
/// interval.
pub fn sql_disks(system: &mut System) -> Vec<sql::exec::catalog::services::host::DiskUnit> {
    let (now, base) = interval(system, |s| &mut s.dsksts, false);
    let ms = u64::try_from(now.at.duration_since(base.at).as_millis()).unwrap_or(u64::MAX);
    super::units::views(system)
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let t = now.disks.get(i).copied().unwrap_or_default();
            let x = t.since(&base.disks.get(i).copied().unwrap_or_default());
            sql::exec::catalog::services::host::DiskUnit {
                unit: u.unit + 1,
                type_code: u.type_code.clone(),
                model: u.model.clone(),
                serial: u.serial.clone(),
                status: u.status.clone(),
                protection: u.protection.clone(),
                capacity: u.usable * 4096,
                free: u.usable.saturating_sub(u.used) * 4096,
                totals: [
                    t.reads,
                    t.writes,
                    t.read_sectors,
                    t.write_sectors,
                    t.busy_ms,
                ],
                elapsed: [
                    x.reads,
                    x.writes,
                    x.read_sectors,
                    x.write_sectors,
                    x.busy_ms,
                ],
                elapsed_ms: ms,
            }
        })
        .collect()
}

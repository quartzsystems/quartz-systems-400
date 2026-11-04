// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The soak (`build soak`, `conformance --soak SECS`): a fresh image
//! under continuous interactive (TN5250 sessions), batch, SQL,
//! save/restore, journaling and network load for the time given. Every
//! minute it samples each guest process's resident memory and open
//! descriptors (`procstat`) and the ASP's use. It fails on a leak
//! (growth past a bound that never came back down), on any SRC the run
//! added to the IPL's log, and on jobs of the load still active after it
//! stopped. It also records `QSSECCHK` on the fresh system and the batch
//! load's job accounting entries (`QACGLVL(*JOB *PRINT)`, read through
//! `QAJBACG`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tn5250_driver::script::Context;
use tn5250_driver::{Connect, ScriptOptions};

use crate::error::HarnessError;
use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The soak users' password.
const SOAK_PASSWORD: &str = "SOAK2026X";
/// Interactive sessions running at once.
const SESSIONS: usize = 4;
/// Samples taken before the leak bounds count (the warm-up).
const WARMUP: usize = 5;
/// Growth of a process's resident memory past this, kB, is a leak.
const RSS_BOUND_KB: u64 = 48 * 1024;
/// Growth of a process's descriptors past this is a leak.
const FD_BOUND: u64 = 16;
/// Growth of the ASP's used pages past this (4 KiB pages) is a leak.
const ASP_BOUND_PAGES: u64 = 128 * 1024;
const LONG: Duration = Duration::from_secs(120);

/// One look at the guest.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    /// Seconds since the load started.
    pub at: u64,
    /// Each process: command line, resident kB, descriptors.
    pub procs: BTreeMap<u32, (String, u64, u64)>,
    /// ASP 1's used pages.
    pub asp_used: u64,
    /// Active jobs.
    pub jobs: usize,
}

/// What the soak did and found.
#[derive(Clone, Debug, Default)]
pub struct SoakReport {
    /// How long the load ran.
    pub secs: u64,
    /// Console cycles (batch, SQL, journal, network).
    pub cycles: u64,
    /// Interactive sign-on/sign-off rounds completed, and failed.
    pub sessions: (u64, u64),
    /// Batch jobs submitted.
    pub batch: u64,
    /// Save and restore rounds.
    pub saves: u64,
    /// Operations whose answer was not the expected one, by kind.
    pub op_errors: BTreeMap<String, u64>,
    /// Every sample.
    pub samples: Vec<Sample>,
    /// What failed the run.
    pub failures: Vec<String>,
    /// What the side checks found: `QSSECCHK` on the fresh system, the
    /// job accounting entries of the batch load.
    pub notes: Vec<String>,
}

impl SoakReport {
    /// The text report.
    #[must_use]
    pub fn render(&self) -> String {
        let mut o = String::new();
        let _ = writeln!(
            o,
            "SOAK: {}s of load, {} console cycles, {} batch jobs, {} save/restore rounds, \
             {} interactive rounds ({} failed)",
            self.secs, self.cycles, self.batch, self.saves, self.sessions.0, self.sessions.1
        );
        for (k, n) in &self.op_errors {
            let _ = writeln!(o, "  unexpected answers: {k} x{n}");
        }
        if let (Some(first), Some(last)) = (self.samples.get(WARMUP.min(self.samples.len() - 1)), self.samples.last()) {
            let _ = writeln!(
                o,
                "  ASP used pages: {} at {}s, {} at {}s ({:+} MiB); active jobs {} -> {}",
                first.asp_used,
                first.at,
                last.asp_used,
                last.at,
                (i128::from(last.asp_used) - i128::from(first.asp_used)) * 4096 / (1 << 20),
                first.jobs,
                last.jobs
            );
            let _ = writeln!(o, "  long-lived processes (RSS kB and descriptors, after warm-up -> end, peak):");
            for (pid, (cmd, rss, fds)) in &last.procs {
                let Some((_, r0, f0)) = first.procs.get(pid) else {
                    continue;
                };
                let peak = self
                    .samples
                    .iter()
                    .filter_map(|s| s.procs.get(pid).map(|p| p.1))
                    .max()
                    .unwrap_or(0);
                let _ = writeln!(o, "    {pid:>5} rss {r0:>7} -> {rss:>7} (peak {peak:>7})  fds {f0:>4} -> {fds:>4}  {cmd}");
            }
        }
        for n in &self.notes {
            let _ = writeln!(o, "  {n}");
        }
        if self.failures.is_empty() {
            o.push_str("SOAK: PASS\n");
        } else {
            for f in &self.failures {
                let _ = writeln!(o, "SOAK: FAIL: {f}");
            }
        }
        o
    }
}

/// `procstat`, `asp` and `wrkactjob` as one sample.
fn sample(s: &mut Session, at: u64) -> Sample {
    let mut out = Sample {
        at,
        ..Sample::default()
    };
    if let Ok(t) = s.command("procstat", CMD) {
        for l in t.lines().skip(1) {
            let w: Vec<&str> = l.split_whitespace().collect();
            if let (Some(pid), Some(rss), Some(fds)) = (
                w.first().and_then(|v| v.parse().ok()),
                w.get(1).and_then(|v| v.parse().ok()),
                w.get(2).and_then(|v| v.parse().ok()),
            ) {
                out.procs.insert(pid, (w[3..].join(" "), rss, fds));
            }
        }
    }
    if let Ok(t) = s.command("svc.asp", CMD) {
        // "ASP 1: n unit(s), T pages (M MiB), F free ..."
        let w: Vec<&str> = t.split_whitespace().collect();
        let num = |after: &str| {
            w.windows(2)
                .find(|p| p[1].trim_end_matches(',') == after)
                .and_then(|p| p[0].parse::<u64>().ok())
        };
        if let (Some(total), Some(free)) = (num("pages"), num("free")) {
            out.asp_used = total.saturating_sub(free);
        }
    }
    out.jobs = active_jobs(s).len();
    out
}

/// The active jobs as `number/user/name`.
fn active_jobs(s: &mut Session) -> BTreeSet<String> {
    let t = s.command("svc.wrkactjob VIEW(3)", Duration::from_secs(30)).unwrap_or_default();
    t.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let i = w.iter().position(|x| x.len() == 6 && x.chars().all(|c| c.is_ascii_digit()))?;
            if i < 2 {
                return None;
            }
            Some(format!("{}/{}/{}", w[i], w[i - 1], w[i - 2]))
        })
        .collect()
}

/// The SRC log's lines.
fn srcs(s: &mut Session) -> Vec<String> {
    s.command("srcs", CMD)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// One interactive round: sign on, a few displays, sign off.
fn round(port: u16, user: &str) -> Result<(), String> {
    let c = Connect::qs400(port, user, SOAK_PASSWORD);
    let options = ScriptOptions {
        timeout_ms: 120_000,
        ..ScriptOptions::default()
    };
    let mut ctx = Context::new(&c, &options);
    let mut d = connect(port, user, SOAK_PASSWORD)?;
    run(&mut d, &mut ctx, "wait \"Sign On\"\nsignon\nwait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n")?;
    for cmd in ["WRKACTJOB", "DSPLIBL", "WRKSPLF", "DSPJOB"] {
        run(&mut d, &mut ctx, &format!("type {cmd}\nkey enter\nsettle 1500\nkey f3\nsettle 800\nkey eraseinput\n"))?;
    }
    run(&mut d, &mut ctx, "type signoff\nkey enter\nwait \"Sign On\"\n")?;
    d.disconnect();
    Ok(())
}

/// A port mapper lookup through the forwarded 449.
fn mapper(port: u16) -> bool {
    use std::io::{Read, Write};
    let Ok(mut c) = std::net::TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = c.set_read_timeout(Some(Duration::from_secs(20)));
    let mut b = [0u8; 5];
    c.write_all(b"as-rmtcmd").is_ok() && c.read_exact(&mut b).is_ok()
}

/// Whether a series grew past `bound` from its first value and ended
/// at its highest: growth that never came back down.
fn grew(series: &[u64], bound: u64) -> bool {
    match (series.first(), series.last()) {
        (Some(&a), Some(&z)) => z > a + bound && series.iter().all(|&v| v <= z),
        _ => false,
    }
}

/// The batch load's job accounting: its `JB` entries through
/// `QAJBACG`'s format, counted, with their CPU and the `SP` entries.
fn accounting(s: &mut Session) -> String {
    let _ = s.command("CPYF FROMFILE(QSYS/QAJBACG) TOFILE(QGPL/SOAKJB) CRTFILE(*YES)", LONG);
    let w = s
        .command(
            "DSPJRN JRN(QSYS/QACGJRN) JRNCDE(A) ENTTYP(JB) OUTPUT(*OUTFILE) OUTFILE(QGPL/SOAKJB)",
            LONG,
        )
        .unwrap_or_default();
    let rows = s
        .command(
            "svc.sql SELECT COUNT(*), SUM(JACPU), MAX(JACCDE) FROM QGPL.SOAKJB WHERE JAJOB = 'SOAKB' AND JACDE = 'SOAKACG';",
            LONG,
        )
        .unwrap_or_default();
    let figures = rows
        .lines()
        .map(str::trim)
        .take_while(|l| !l.ends_with("row(s)"))
        .filter(|l| l.split_whitespace().next().is_some_and(|w| w.parse::<u64>().is_ok()))
        .last()
        .unwrap_or("none")
        .to_string();
    let _ = s.command("svc.sql DROP TABLE QGPL.SOAKJB;", LONG);
    format!(
        "job accounting: {}; SOAKB under SOAKACG (count, CPU ms, highest end code): {figures}",
        w.lines().find(|l| l.contains("entries")).unwrap_or("no DSPJRN outfile").trim()
    )
}

impl Suite {
    /// The soak for `dur`.
    #[allow(clippy::too_many_lines)] // one run, start to end
    pub fn run_soak(&self, dur: Duration) -> Result<SoakReport, HarnessError> {
        let launch = Launch {
            memory_mb: 4096,
            cpus: 4,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let fail = |e: String| HarnessError::Failed {
            step: "soak",
            reason: e,
            log: String::new(),
        };
        s.expect(crate::steps::SRC_IPL_COMPLETE, Duration::from_secs(300)).map_err(fail)?;
        // The console's banner may come before the last SRC or after it.
        let _ = s.expect("Service console ready", Duration::from_secs(60));
        s.discard();
        // A loaded host makes the first commands slow: a few tries.
        let mut tcp = Err(String::new());
        for _ in 0..3 {
            tcp = crate::tcp::guest_address(&mut s);
            if tcp.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_secs(10));
            s.discard();
        }
        tcp.map_err(fail)?;
        let port = launch.telnet_port.ok_or_else(|| fail("no telnet port".into()))?;
        {
            let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
            let options = ScriptOptions::default();
            let mut ctx = Context::new(&c, &options);
            let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD).map_err(fail)?;
            run(&mut d, &mut ctx, "wait \"Sign On\"\n").map_err(fail)?;
            secofr(&mut d, &mut ctx).map_err(fail)?;
            run(&mut d, &mut ctx, "wait \"Main Menu\"\ntype signoff\nkey enter\nwait \"Sign On\"\n").map_err(fail)?;
            d.disconnect();
        }
        s.discard();
        let _ = s.command("svc.release", CMD);
        for i in 1..=SESSIONS {
            let out = s
                .command(&format!("crtusrprf USRPRF(SOAK{i:02}) PASSWORD({SOAK_PASSWORD}) PWDEXP(*NO)"), CMD)
                .map_err(fail)?;
            if !out.contains("created") {
                return Err(fail(format!("crtusrprf SOAK{i:02}: {out}")));
            }
        }
        let setup = [
            "strsbs QSYS/QBATCH",
            "STRHOSTSVR SERVER(*ALL)",
            "CRTLIB LIB(SOAKLIB) TEXT('soak')",
            "CRTLIB LIB(SOAKSAV) TEXT('soak saves')",
            "crtpf FILE(SOAKSAV/DATA) RCDLEN(80)",
            "svc.sql CREATE TABLE SOAKLIB.T (N INTEGER, T CHAR(40));",
            "CRTJRNRCV JRNRCV(SOAKLIB/R0001)",
            "CRTJRN JRN(SOAKLIB/SOAKJRN) JRNRCV(SOAKLIB/R0001)",
            "STRJRNPF FILE(SOAKLIB/T) JRN(SOAKLIB/SOAKJRN)",
            "crtsavf FILE(QGPL/SOAKSAVF)",
            "CRTJRNRCV JRNRCV(QSYS/ACGRCV0001) AUT(*EXCLUDE)",
            "CRTJRN JRN(QSYS/QACGJRN) JRNRCV(QSYS/ACGRCV0001) AUT(*EXCLUDE)",
            "svc.sysval QACGLVL *JOB *PRINT",
        ];
        for c in setup {
            s.command(c, LONG).map_err(fail)?;
        }
        // The fresh system as QSSECCHK sees it.
        let sec = s.command("QSSECCHK OUTPUT(*)", LONG).unwrap_or_default();
        let total = sec.lines().find(|l| l.contains("checks:")).unwrap_or("no QSSECCHK report").trim().to_string();
        let fails: Vec<&str> = sec.lines().filter(|l| l.contains(" FAIL ")).map(str::trim).collect();
        let mut notes = vec![format!("QSSECCHK at the start: {total}")];
        notes.extend(fails.iter().map(|l| format!("QSSECCHK: {l}")));
        let mapper_port = launch.host_ports.iter().find(|(_, g)| *g == 449).map(|(h, _)| *h);
        let base_srcs: BTreeSet<String> = srcs(&mut s).into_iter().collect();
        let base_jobs = active_jobs(&mut s);
        let mut report = SoakReport {
            notes,
            ..SoakReport::default()
        };
        let start = Instant::now();
        report.samples.push(sample(&mut s, 0));

        // Interactive load.
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));
        let first_err = Arc::new(Mutex::new(None::<String>));
        let mut threads = Vec::new();
        for i in 1..=SESSIONS {
            let (stop, done, failed, first_err) =
                (Arc::clone(&stop), Arc::clone(&done), Arc::clone(&failed), Arc::clone(&first_err));
            let user = format!("SOAK{i:02}");
            threads.push(
                std::thread::Builder::new()
                    .name(format!("soak-{i}"))
                    .stack_size(8 << 20)
                    .spawn(move || {
                        while !stop.load(Ordering::Relaxed) {
                            match round(port, &user) {
                                Ok(()) => {
                                    done.fetch_add(1, Ordering::Relaxed);
                                }
                                Err(e) => {
                                    failed.fetch_add(1, Ordering::Relaxed);
                                    first_err.lock().expect("err").get_or_insert(e);
                                    std::thread::sleep(Duration::from_secs(5));
                                }
                            }
                        }
                    })
                    .map_err(|e| fail(e.to_string()))?,
            );
            std::thread::sleep(Duration::from_millis(500));
        }

        // Console load: batch, SQL, journal, save/restore, network.
        let mut next_sample = Instant::now() + Duration::from_secs(60);
        let mut errors: BTreeMap<String, u64> = BTreeMap::new();
        let mut check = |what: &str, out: Result<String, String>, word: &str| {
            let ok = out.as_ref().is_ok_and(|t| t.contains(word));
            if !ok {
                *errors.entry(what.to_string()).or_default() += 1;
            }
        };
        let mut i: u64 = 0;
        while start.elapsed() < dur {
            i += 1;
            check(
                "batch insert",
                s.command(
                    &format!("SBMJOB CMD(RUNSQL SQL('INSERT INTO SOAKLIB/T VALUES({i}, ''batch'')') COMMIT(*NONE)) JOB(SOAKB) JOBQ(QGPL/QBATCH) ACGCDE(SOAKACG)"),
                    CMD,
                ),
                "submitted",
            );
            check(
                "batch delay",
                s.command("SBMJOB CMD(DLYJOB DLY(2)) JOB(SOAKD) JOBQ(QGPL/QBATCH)", CMD),
                "submitted",
            );
            report.batch += 2;
            check(
                "sql insert",
                s.command(&format!("svc.sql INSERT INTO SOAKLIB.T VALUES({i}, 'console');"), LONG),
                "row",
            );
            check("sql count", s.command("svc.sql SELECT COUNT(*) FROM SOAKLIB.T;", LONG), "row(s)");
            if i.is_multiple_of(10) {
                check("sql delete", s.command("svc.sql DELETE FROM SOAKLIB.T;", LONG), "row");
                check("receiver change", s.command("CHGJRN JRN(SOAKLIB/SOAKJRN) JRNRCV(*GEN)", LONG), "SOAKJRN");
            }
            if i.is_multiple_of(5) {
                check(
                    "save",
                    s.command("savlib LIB(SOAKSAV) DEV(*SAVF) SAVF(QGPL/SOAKSAVF) CLEAR(*ALL)", LONG),
                    "CPC3722",
                );
                check(
                    "restore",
                    s.command("rstlib SAVLIB(SOAKSAV) DEV(*SAVF) SAVF(QGPL/SOAKSAVF) RSTLIB(SOAKRST)", LONG),
                    "CPC3703",
                );
                check("library delete", s.command("DLTLIB LIB(SOAKRST)", LONG), "SOAKRST");
                report.saves += 1;
            }
            if let Some(p) = mapper_port {
                let ok = if mapper(p) { Ok("ok".to_string()) } else { Err(String::new()) };
                check("port mapper", ok, "ok");
            }
            if Instant::now() >= next_sample {
                next_sample += Duration::from_secs(60);
                let at = start.elapsed().as_secs();
                report.samples.push(sample(&mut s, at));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        report.cycles = i;
        report.secs = start.elapsed().as_secs();
        stop.store(true, Ordering::Relaxed);
        for t in threads {
            let _ = t.join();
        }
        report.sessions = (done.load(Ordering::Relaxed), failed.load(Ordering::Relaxed));
        report.op_errors = errors;

        // The load's jobs drain, then the last look.
        let until = Instant::now() + Duration::from_secs(180);
        let mut left: Vec<String> = Vec::new();
        while Instant::now() < until {
            left = active_jobs(&mut s)
                .into_iter()
                .filter(|j| !base_jobs.contains(j))
                .filter(|j| j.contains("/SOAK") || j.ends_with("/SOAKB") || j.ends_with("/SOAKD"))
                .collect();
            if left.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
        let at = start.elapsed().as_secs();
        report.samples.push(sample(&mut s, at));
        if !left.is_empty() {
            report.failures.push(format!("jobs of the load still active: {}", left.join(" ")));
        }
        let new_srcs: Vec<String> = srcs(&mut s).into_iter().filter(|l| !base_srcs.contains(l)).collect();
        if !new_srcs.is_empty() {
            report.failures.push(format!("new SRCs: {}", new_srcs.join(" | ")));
        }
        if let Some(e) = first_err.lock().expect("err").clone()
            && report.sessions.1 * 10 > report.sessions.0
        {
            report.failures.push(format!(
                "{} of {} interactive rounds failed; the first: {}",
                report.sessions.1,
                report.sessions.0 + report.sessions.1,
                e.lines().next().unwrap_or_default()
            ));
        }
        // Leaks: the long-lived processes and the ASP, after warm-up.
        let warm: Vec<&Sample> = report.samples.iter().skip(WARMUP).collect();
        if warm.len() >= 3 {
            let last = warm.last().expect("some");
            for (pid, (cmd, ..)) in &last.procs {
                let rss: Vec<u64> = warm.iter().filter_map(|x| x.procs.get(pid).map(|p| p.1)).collect();
                let fds: Vec<u64> = warm.iter().filter_map(|x| x.procs.get(pid).map(|p| p.2)).collect();
                if rss.len() != warm.len() {
                    continue;
                }
                if grew(&rss, RSS_BOUND_KB) {
                    report.failures.push(format!("pid {pid} ({cmd}) memory grew {} -> {} kB", rss[0], rss[rss.len() - 1]));
                }
                if grew(&fds, FD_BOUND) {
                    report.failures.push(format!("pid {pid} ({cmd}) descriptors grew {} -> {}", fds[0], fds[fds.len() - 1]));
                }
            }
            let asp: Vec<u64> = warm.iter().map(|x| x.asp_used).collect();
            if grew(&asp, ASP_BOUND_PAGES) {
                report.failures.push(format!("ASP use grew {} -> {} pages", asp[0], asp[asp.len() - 1]));
            }
        }
        report.notes.push(accounting(&mut s));
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        s.discard();
        let dir = crate::target_dir().join("conformance");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("soak-serial.log"), &s.log);
        let _ = std::fs::write(dir.join("soak-report.txt"), report.render());
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::grew;

    #[test]
    fn growth_that_stays() {
        assert!(grew(&[100, 150, 200, 260], 100));
        assert!(!grew(&[100, 300, 150, 210], 100));
        assert!(!grew(&[100, 120, 130], 100));
    }
}

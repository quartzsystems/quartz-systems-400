// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The load run (`conformance --load N`): a fresh image with more
//! memory and CPUs, N concurrent TN5250 sessions signing on as
//! `QSECOFR`, each refreshing `WRKACTJOB` while all are connected, then
//! signing off. Reports sign-on and refresh latencies, the guest's
//! status and the QEMU process's memory and CPU time.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use tn5250::term::Key;
use tn5250_driver::Connect;
use tn5250_driver::script::Context;
use tn5250_driver::{ScriptOptions, session::Session as Display};

use crate::error::HarnessError;
use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The load users' password.
const LOAD_PASSWORD: &str = "LOAD2026X";

/// Refreshes per session.
const REFRESHES: usize = 5;

/// A meeting point every session passes, failed or not: the sessions
/// that reached it wait until all have, or until the time is up.
#[derive(Default)]
struct Gate {
    passed: Mutex<usize>,
    all: Condvar,
}

impl Gate {
    /// Count one session through.
    fn pass(&self) {
        *self.passed.lock().expect("gate") += 1;
        self.all.notify_all();
    }

    /// Wait until `n` sessions passed or `limit` elapsed.
    fn wait(&self, n: usize, limit: Duration) {
        let g = self.passed.lock().expect("gate");
        let _ = self.all.wait_timeout_while(g, limit, |p| *p < n);
    }
}

/// Where a session is: which gates it has passed.
struct Gates {
    n: usize,
    signed_on: Arc<Gate>,
    done: Arc<Gate>,
}

/// The time a gate waits for the slowest session.
const GATE_LIMIT: Duration = Duration::from_mins(20);

/// One session's figures.
#[derive(Clone, Debug, Default)]
struct Figures {
    sign_on: Duration,
    first: Duration,
    refreshes: Vec<Duration>,
    error: Option<String>,
}

/// What the run measured.
#[derive(Clone, Debug)]
pub struct LoadReport {
    /// Sessions asked for.
    pub sessions: usize,
    /// Sessions that signed on, refreshed and signed off.
    pub completed: usize,
    /// The first errors, one per failed session (at most ten).
    pub errors: Vec<String>,
    /// Sign-on latency: median and 95th percentile.
    pub sign_on: (Duration, Duration),
    /// The first WRKACTJOB display: median and 95th percentile.
    pub first: (Duration, Duration),
    /// A WRKACTJOB refresh (F5) with every session connected: median
    /// and 95th percentile.
    pub refresh: (Duration, Duration),
    /// Wall time from the first connection to the last sign-off.
    pub wall: Duration,
    /// QEMU's resident memory at the peak, MiB, and its CPU seconds.
    pub qemu_rss_mib: u64,
    /// QEMU's user and system CPU time over the run, seconds.
    pub qemu_cpu_secs: f64,
    /// The guest's `WRKSYSSTS` text at the peak.
    pub sys_status: String,
}

fn percentiles(mut v: Vec<Duration>) -> (Duration, Duration) {
    if v.is_empty() {
        return (Duration::ZERO, Duration::ZERO);
    }
    v.sort();
    // Nearest rank: the median and the 95th percentile.
    let at = |pct: usize| v[(v.len() - 1) * pct / 100];
    (at(50), at(95))
}

/// Resident memory (MiB) and CPU seconds of a host process.
fn proc_figures(pid: u32) -> (u64, f64) {
    let rss = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1).map(str::to_string))
        })
        .and_then(|k| k.parse::<u64>().ok())
        .map_or(0, |kib| kib / 1024);
    let cpu = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| {
            // Fields after the command's closing parenthesis; utime and
            // stime are the 12th and 13th of those.
            let rest = s.rsplit_once(')')?.1;
            let f: Vec<&str> = rest.split_whitespace().collect();
            let ticks = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
            Some(ticks / 100.0)
        })
        .unwrap_or(0.0);
    (rss, cpu)
}

/// One session: sign on, WRKACTJOB, wait for every session, refresh,
/// wait again, sign off.
fn session(port: u16, user: &str, g: &Gates) -> Figures {
    let mut f = Figures::default();
    let mut passed = (false, false);
    let r = (|| -> Result<(), String> {
        let connect_args = Connect::qs400(port, user, LOAD_PASSWORD);
        let options = ScriptOptions {
            timeout_ms: 120_000,
            ..ScriptOptions::default()
        };
        let mut ctx = Context::new(&connect_args, &options);
        let t0 = Instant::now();
        let mut d: Display = connect(port, user, LOAD_PASSWORD)?;
        run(&mut d, &mut ctx, "wait \"Sign On\"\nsignon\n")?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        f.sign_on = t0.elapsed();
        let t1 = Instant::now();
        run(
            &mut d,
            &mut ctx,
            "type wrkactjob\nkey enter\nwait \"Work with Active Jobs\"\n",
        )?;
        f.first = t1.elapsed();
        g.signed_on.pass();
        passed.0 = true;
        g.signed_on.wait(g.n, GATE_LIMIT);
        for _ in 0..REFRESHES {
            let t = Instant::now();
            d.key(Key::F(5)).map_err(|e| format!("{e:#}"))?;
            if !d
                .wait_unlocked(Duration::from_secs(600))
                .map_err(|e| format!("{e:#}"))?
            {
                return Err("WRKACTJOB did not come back after F5".into());
            }
            f.refreshes.push(t.elapsed());
        }
        g.done.pass();
        passed.1 = true;
        g.done.wait(g.n, GATE_LIMIT);
        run(
            &mut d,
            &mut ctx,
            "key f3\nwait \"Main Menu\"\ntype signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        Ok(())
    })();
    if !passed.0 {
        g.signed_on.pass();
    }
    if !passed.1 {
        g.done.pass();
    }
    if let Err(e) = r {
        f.error = Some(e);
    }
    f
}

impl Suite {
    /// The load run with `n` sessions.
    #[allow(clippy::too_many_lines)] // one run, start to end
    pub fn run_load(&self, n: usize) -> Result<LoadReport, HarnessError> {
        let launch = Launch {
            memory_mb: 6144,
            cpus: 4,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let fail = |e: String| HarnessError::Failed {
            step: "load",
            reason: e,
            log: String::new(),
        };
        s.expect("Service console ready", Duration::from_secs(180))
            .map_err(fail)?;
        let port = launch.telnet_port.ok_or_else(|| fail("no telnet port".into()))?;
        // The first sign-on changes QSECOFR's expired password; the
        // others sign on with the new one.
        {
            let connect_args = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
            let options = ScriptOptions::default();
            let mut ctx = Context::new(&connect_args, &options);
            let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD).map_err(fail)?;
            run(&mut d, &mut ctx, "wait \"Sign On\"\n").map_err(fail)?;
            secofr(&mut d, &mut ctx).map_err(fail)?;
            run(
                &mut d,
                &mut ctx,
                "wait \"Main Menu\"\ntype signoff\nkey enter\nwait \"Sign On\"\n",
            )
            .map_err(fail)?;
            d.disconnect();
        }
        // One profile per session, as many users would sign on. The
        // first command reads what the boot left on the console.
        s.discard();
        let _ = s.command("svc.release", CMD);
        for i in 0..n {
            let out = s
                .command(
                    &format!(
                        "crtusrprf USRPRF(LOAD{:03}) PASSWORD({LOAD_PASSWORD}) PWDEXP(*NO)",
                        i + 1
                    ),
                    CMD,
                )
                .map_err(fail)?;
            if !out.contains("created") {
                return Err(fail(format!("crtusrprf LOAD{:03}: {out}", i + 1)));
            }
        }
        let pid = s.pid();
        let (_, cpu_before) = proc_figures(pid);
        let signed_on = Arc::new(Gate::default());
        let done = Arc::new(Gate::default());
        let figures = Arc::new(Mutex::new(Vec::new()));
        let start = Instant::now();
        let mut threads = Vec::new();
        for i in 0..n {
            let gates = Gates {
                n,
                signed_on: Arc::clone(&signed_on),
                done: Arc::clone(&done),
            };
            let out = Arc::clone(&figures);
            threads.push(
                std::thread::Builder::new()
                    .name(format!("load-{i}"))
                    .stack_size(8 << 20)
                    .spawn(move || {
                        let f = session(port, &format!("LOAD{:03}", i + 1), &gates);
                        out.lock().expect("figures").push(f);
                    })
                    .map_err(|e| fail(e.to_string()))?,
            );
            // A modest ramp: QEMU's user network takes connections
            // one at a time.
            std::thread::sleep(Duration::from_millis(50));
        }
        signed_on.wait(n, GATE_LIMIT);
        s.discard();
        let sys_status = s
            .command("wrksyssts", Duration::from_secs(60))
            .unwrap_or_default();
        done.wait(n, GATE_LIMIT);
        for t in threads {
            let _ = t.join();
        }
        let wall = start.elapsed();
        let (rss, cpu_after) = proc_figures(pid);
        let figs = figures.lock().expect("figures").clone();
        let ok: Vec<&Figures> = figs.iter().filter(|f| f.error.is_none()).collect();
        let errors = figs
            .iter()
            .filter_map(|f| f.error.clone())
            .take(10)
            .collect();
        let report = LoadReport {
            sessions: n,
            completed: ok.len(),
            errors,
            sign_on: percentiles(ok.iter().map(|f| f.sign_on).collect()),
            first: percentiles(ok.iter().map(|f| f.first).collect()),
            refresh: percentiles(ok.iter().flat_map(|f| f.refreshes.clone()).collect()),
            wall,
            qemu_rss_mib: rss,
            qemu_cpu_secs: cpu_after - cpu_before,
            sys_status,
        };
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(30));
        s.discard();
        let _ = std::fs::write(crate::target_dir().join("conformance/load-serial.log"), &s.log);
        Ok(report)
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 106: Collection Services under load. A 15-second collection
//! runs while batch jobs and a telnet session work; `CRTPFRDTA` makes
//! the `QAPM*` files and their figures are checked: present, not zero,
//! and consistent with each other (the jobs' CPU within the machine's,
//! one record per job in both job files, the intervals numbered in
//! order).

use std::time::{Duration, Instant};

use tn5250_driver::ScriptOptions;
use tn5250_driver::script::Context;

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, words: &[&str]) -> Result<(), String> {
    if words.iter().all(|w| out.contains(w)) {
        Ok(())
    } else {
        Err(format!("{what}:\n{out}"))
    }
}

/// The numbers of a one-row `SELECT`: the last line with numbers before
/// `row(s)`.
fn select(s: &mut Session, sql: &str) -> Result<Vec<f64>, String> {
    let out = s.command(&format!("svc.sql {sql};"), CMD)?;
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let end = lines
        .iter()
        .position(|l| l.ends_with("row(s)"))
        .ok_or_else(|| format!("{sql}:\n{out}"))?;
    let nums: Vec<f64> = lines[..end]
        .iter()
        .rev()
        .map(|l| {
            l.split_whitespace()
                .filter_map(|w| w.parse::<f64>().ok())
                .collect::<Vec<_>>()
        })
        .find(|v| !v.is_empty())
        .ok_or_else(|| format!("{sql}: no numbers:\n{out}"))?;
    Ok(nums)
}

impl Suite {
    /// Step 106: a short collection, its files and their figures.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn collection_services(&self, s: &mut Session) -> Result<String, String> {
        let _ = s.command("ENDPFRCOL FRCCOLEND(*YES)", CMD);
        let cfg = s.command(
            "CFGPFRCOL INTERVAL(.25) LIB(QPFRDATA) RETPERIOD(*PERM) CRTDBF(*NO)",
            CMD,
        )?;
        want(&cfg, "CFGPFRCOL", &["INTERVAL(.25)"])?;
        let started = s.command("STRPFRCOL COLPRF(*STANDARD)", CMD)?;
        let name = started
            .split_whitespace()
            .find(|w| {
                w.len() == 10 && w.starts_with('Q') && w[1..].chars().all(|c| c.is_ascii_digit())
            })
            .ok_or_else(|| format!("STRPFRCOL:\n{started}"))?
            .to_string();
        let again = s.command("STRPFRCOL", CMD)?;
        want(&again, "a second STRPFRCOL", &["CPFB993"])?;
        // Load: batch jobs and an interactive session.
        let _ = s.command("strsbs QSYS/QBATCH", CMD);
        let _ = s.command(
            "svc.sql CREATE TABLE QGPL/PFRLOAD (N INTEGER, T CHAR(40));",
            CMD,
        );
        for i in 0..4 {
            let _ = s.command(
                &format!("sbmjob CMD(RUNSQL SQL('INSERT INTO QGPL/PFRLOAD VALUES({i}, ''load'')') COMMIT(*NONE)) JOB(PFRLOAD{i}) JOBQ(QGPL/QBATCH)"),
                CMD,
            );
        }
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = tn5250_driver::Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n",
        )?;
        for cmd in ["DSPLIBL", "WRKACTJOB", "WRKSYSSTS", "WRKSYSACT"] {
            run(
                &mut d,
                &mut ctx,
                &format!(
                    "type {cmd}\nkey enter\nsettle 2000\nkey f3\nsettle 1000\nkey eraseinput\n"
                ),
            )?;
        }
        let _ = s.command("svc.dbbench 500", Duration::from_secs(120));
        // At least six intervals (90 seconds) before it ends.
        let until = Instant::now() + Duration::from_secs(240);
        let mut chk = String::new();
        while Instant::now() < until {
            chk = s.command("CHKPFRCOL", CMD)?;
            let n: u32 = chk
                .split_whitespace()
                .zip(chk.split_whitespace().skip(1))
                .find(|(_, w)| w.starts_with("interval"))
                .and_then(|(n, _)| n.parse().ok())
                .unwrap_or(0);
            if n >= 6 {
                break;
            }
            run(
                &mut d,
                &mut ctx,
                "type DSPJOB\nkey enter\nsettle 1500\nkey f3\nsettle 1000\nkey eraseinput\n",
            )?;
            std::thread::sleep(Duration::from_secs(5));
        }
        want(&chk, "CHKPFRCOL while active", &["is active", &name])?;
        run(
            &mut d,
            &mut ctx,
            "type signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        let ended = s.command("ENDPFRCOL", CMD)?;
        want(&ended, "ENDPFRCOL", &["ended collection", &name])?;
        let none = s.command("CHKPFRCOL", CMD)?;
        want(&none, "CHKPFRCOL after the end", &["CPFB994"])?;
        let obj = s.command("svc.obj ls QPFRDATA", CMD)?;
        want(&obj, "the management collection", &[&name, "*MGTCOL"])?;
        let made = s.command(
            &format!("CRTPFRDTA FROMMGTCOL(QPFRDATA/{name})"),
            Duration::from_secs(120),
        )?;
        want(&made, "CRTPFRDTA", &["records written"])?;
        // The figures.
        let lib = "QPFRDATA";
        let sys = select(
            s,
            &format!(
                "SELECT COUNT(*), MIN(INTNUM), MAX(INTNUM), SUM(SYSPTU), SUM(SYSCTA), MIN(INTSEC) FROM {lib}/QAPMSYSTEM"
            ),
        )?;
        let (n, first, last, used, avail, secs) = (
            sys[0],
            sys[1],
            sys[2],
            sys[3],
            sys.get(4).copied().unwrap_or(0.0),
            sys.get(5).copied().unwrap_or(0.0),
        );
        if n < 6.0
            || (first - 1.0).abs() > 0.1
            || (last - n).abs() > 0.1
            || used <= 0.0
            || avail < used
            || secs < 1.0
        {
            return Err(format!("QAPMSYSTEM: {sys:?}"));
        }
        let jobs = select(
            s,
            &format!(
                "SELECT COUNT(*), SUM(JBCPU), SUM(JBDBR + JBNDB + JBDBW), MAX(JBNTR) FROM {lib}/QAPMJOBMI"
            ),
        )?;
        let (rows, cpu_s) = (jobs[0], jobs[1]);
        // The jobs' CPU within the machine's (seconds and milliseconds).
        if rows < n
            || cpu_s <= 0.0
            || cpu_s > used / 1000.0 * 1.05 + 1.0
            || jobs.get(2).copied().unwrap_or(0.0) <= 0.0
        {
            return Err(format!("QAPMJOBMI: {jobs:?} against SYSPTU {used}"));
        }
        if jobs.get(3).copied().unwrap_or(0.0) < 1.0 {
            return Err(format!(
                "no interactive transactions in QAPMJOBMI: {jobs:?}"
            ));
        }
        let os = select(s, &format!("SELECT COUNT(*) FROM {lib}/QAPMJOBOS"))?;
        if (os[0] - rows).abs() > 0.1 {
            return Err(format!("QAPMJOBOS has {} rows, QAPMJOBMI {rows}", os[0]));
        }
        let pools = select(
            s,
            &format!("SELECT COUNT(*), MIN(POSIZ), COUNT(DISTINCT PONBR) FROM {lib}/QAPMPOOL"),
        )?;
        if pools[0] < n || pools[1] <= 0.0 || pools.get(2).copied().unwrap_or(0.0) < 2.0 {
            return Err(format!("QAPMPOOL: {pools:?}"));
        }
        let disks = select(
            s,
            &format!("SELECT COUNT(*), MIN(DSCAP), SUM(DSRDS + DSWRTS) FROM {lib}/QAPMDISK"),
        )?;
        if disks[0] < n || disks[1] <= 0.0 || disks.get(2).copied().unwrap_or(0.0) <= 0.0 {
            return Err(format!("QAPMDISK: {disks:?}"));
        }
        let jsum = select(
            s,
            &format!("SELECT COUNT(*), SUM(JSCPU) FROM {lib}/QAPMJSUM"),
        )?;
        if jsum[0] < n || jsum.get(1).copied().unwrap_or(0.0) <= 0.0 {
            return Err(format!("QAPMJSUM: {jsum:?}"));
        }
        let conf = select(s, &format!("SELECT COUNT(*) FROM {lib}/QAPMCONF"))?;
        if conf[0] < 6.0 {
            return Err(format!("QAPMCONF: {conf:?}"));
        }
        let _ = s.command("svc.sql DROP TABLE QGPL/PFRLOAD;", CMD);
        let _ = s.command(
            "CFGPFRCOL INTERVAL(15.0) RETPERIOD(120 *HOURS) CRTDBF(*YES)",
            CMD,
        );
        Ok(format!(
            "{name}: {n} intervals; CPU {:.1}s of {:.1}s used, jobs {cpu_s:.1}s over {rows} records; {} pool, {} disk records",
            used / 1000.0,
            avail / 1000.0,
            pools[0],
            disks[0]
        ))
    }
}

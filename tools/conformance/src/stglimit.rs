// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 140, the system ASP's storage limits: the ASP filled past a
//! threshold set just above its use sends `CPF0907`; `QSTGLOWLMT` set
//! above what is available sends `CPI099C`, `*CRITMSG` adds `CPI099B`,
//! `*REGFAC` submits a job that runs the `QIBM_QWC_QSTGLOWACN` exit
//! program, and a subsystem cannot start (`CPF1050`). Everything is put
//! back after.

use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The exit program: it leaves a data area behind.
const EXIT_PGM: &str = "\
             PGM
             CRTDTAARA  DTAARA(QGPL/STGLOWRAN) TYPE(*CHAR) LEN(10) +
                          VALUE('RAN')
             ENDPGM
";

/// Poll a service console command until `done`, `secs` at most.
fn poll(
    s: &mut Session,
    cmd: &str,
    secs: u64,
    done: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let until = Instant::now() + Duration::from_secs(secs);
    loop {
        let out = s.command(cmd, CMD)?;
        if done(&out) || Instant::now() > until {
            return Ok(out);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// ASP 1's pages and free pages, from `svc.asp`'s first line.
fn asp_pages(s: &mut Session) -> Result<(u64, u64), String> {
    let out = s.command("svc.asp", CMD)?;
    let line = out
        .lines()
        .find(|l| l.starts_with("ASP 1:"))
        .ok_or_else(|| format!("svc.asp:\n{out}"))?;
    let num_before = |word: &str| -> Option<u64> {
        let i = line.find(word)?;
        line[..i].split_whitespace().last()?.parse().ok()
    };
    match (num_before(" pages"), num_before(" free")) {
        (Some(p), Some(f)) => Ok((p, f)),
        _ => Err(format!("svc.asp: {line}")),
    }
}

impl Suite {
    /// Step 140.
    #[allow(clippy::too_many_lines, clippy::unused_self)] // one walk through the limits; every step has the same shape
    pub(crate) fn storage_limits(&self, s: &mut Session) -> Result<String, String> {
        let (pages, free) = asp_pages(s)?;
        let used = pages - free;
        // The threshold two percent above the use, then the ASP filled
        // past it.
        let pct = used * 100 / pages;
        let threshold = (pct + 2).min(99);
        let set = s.command(&format!("svc.asp threshold {threshold}"), CMD)?;
        if !set.contains("threshold set") {
            return Err(format!("asp threshold: {set}"));
        }
        let want = pages * threshold / 100 + pages / 200;
        let kib = want.saturating_sub(used) * 4;
        let _ = s.command("CRTLIB LIB(STGFILL)", CMD);
        let each = kib / 8 + 1;
        let filled = s.command(
            &format!("svc.obj fill STGFILL 8 {each}"),
            Duration::from_mins(5),
        )?;
        let hist = poll(s, "dsplog MSGID(CPF0907)", 60, |o| o.contains("CPF0907"))?;
        if !hist.contains("CPF0907") {
            let asp = s.command("svc.asp", CMD)?;
            return Err(format!(
                "no CPF0907 at {threshold}%:\n{hist}\n--- fill\n{filled}\n--- asp\n{asp}"
            ));
        }
        let opr = s.command("DSPMSG MSGQ(QSYSOPR)", CMD)?;
        if !opr.contains("storage threshold") {
            return Err(format!("QSYSOPR has no CPF0907:\n{opr}"));
        }
        // The lower limit above what is available: CPI099C, *MSG.
        let free_pct = 100 - (used * 100 / pages);
        let limit = (free_pct + 1).min(100);
        s.command(&format!("CHGSYSVAL SYSVAL(QSTGLOWLMT) VALUE({limit})"), CMD)?;
        let low = poll(s, "dsplog MSGID(CPI099C)", 60, |o| o.contains("CPI099C"))?;
        if !low.contains("CPI099C") {
            return Err(format!("no CPI099C below QSTGLOWLMT({limit}):\n{low}"));
        }
        // *CRITMSG: CPI099B to QSECOFR's queue at once (the value changed).
        s.command("CHGSYSVAL SYSVAL(QSTGLOWACN) VALUE(*CRITMSG)", CMD)?;
        let crit = poll(s, "DSPMSG MSGQ(QSECOFR)", 60, |o| {
            o.contains("critical storage lower limit")
        })?;
        if !crit.contains("critical storage lower limit") {
            return Err(format!("no CPI099B for *CRITMSG:\n{crit}"));
        }
        // *REGFAC: the exit program runs in a job of its own.
        let _ = s.command("DLTDTAARA DTAARA(QGPL/STGLOWRAN)", CMD);
        let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
        crate::qir::qemu::push(s, "STGLOWSRC", EXIT_PGM)?;
        let put = s.command(
            "svc.srcput QGPL/QCLSRC STGLOWX SRCTYPE(CLLE) FROMSTMF(STGLOWSRC)",
            CMD,
        )?;
        if !put.contains("written") {
            return Err(format!("srcput: {put}"));
        }
        let made = s.command(
            "CRTBNDCL PGM(QGPL/STGLOWX) SRCFILE(QGPL/QCLSRC)",
            Duration::from_secs(60),
        )?;
        if !made.contains("created") {
            return Err(format!("CRTBNDCL: {made}"));
        }
        let add = s.command(
            "ADDEXITPGM EXITPNT(QIBM_QWC_QSTGLOWACN) FORMAT(STGL0100) PGMNBR(1) PGM(QGPL/STGLOWX)",
            CMD,
        )?;
        if add.trim_start().starts_with("CPF") {
            return Err(format!("ADDEXITPGM: {add}"));
        }
        s.command("CHGSYSVAL SYSVAL(QSTGLOWACN) VALUE(*REGFAC)", CMD)?;
        let ran = poll(s, "DSPDTAARA DTAARA(QGPL/STGLOWRAN)", 90, |o| {
            o.contains("RAN")
        })?;
        if !ran.contains("RAN") {
            let jobs = s.command("wrkactjob", CMD)?;
            let log = s.command("dsplog", CMD)?;
            return Err(format!(
                "the exit program did not run:\n{ran}\n--- wrkactjob\n{jobs}\n--- dsplog\n{log}"
            ));
        }
        // No subsystem starts while *REGFAC holds below the limit.
        let _ = s.command("CRTSBSD SBSD(QGPL/STGTEST) POOLS((1 *BASE))", CMD);
        let strsbs = s.command("STRSBS SBSD(QGPL/STGTEST)", CMD)?;
        let refused = strsbs.contains("CPF1050") || strsbs.contains("cannot start while storage");
        // Everything back.
        s.command("CHGSYSVAL SYSVAL(QSTGLOWACN) VALUE(*MSG)", CMD)?;
        s.command("CHGSYSVAL SYSVAL(QSTGLOWLMT) VALUE(5)", CMD)?;
        let _ = s.command(
            "RMVEXITPGM EXITPNT(QIBM_QWC_QSTGLOWACN) FORMAT(STGL0100) PGMNBR(1)",
            CMD,
        );
        let _ = s.command("ENDSBS SBS(STGTEST) OPTION(*IMMED)", CMD);
        let _ = s.command("DLTSBSD SBSD(QGPL/STGTEST)", CMD);
        let _ = s.command("DLTDTAARA DTAARA(QGPL/STGLOWRAN)", CMD);
        let _ = s.command("DLTPGM PGM(QGPL/STGLOWX)", CMD);
        let _ = s.command("DLTLIB LIB(STGFILL)", Duration::from_mins(2));
        let _ = s.command("svc.asp threshold 90", CMD);
        if !refused {
            return Err(format!("STRSBS below the limit with *REGFAC: {strsbs}"));
        }
        Ok(format!(
            "CPF0907 at a {threshold}% threshold after filling {kib} KiB; CPI099C below QSTGLOWLMT({limit}); CPI099B for *CRITMSG; the QIBM_QWC_QSTGLOWACN program ran for *REGFAC; STRSBS refused with CPF1050"
        ))
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 107 and 108. 107: `QSSECCHK` on the freshly built system finds
//! no failure, then catches a profile with a default password. 108: job
//! accounting: `QACGLVL(*JOB *PRINT)`, jobs submitted with accounting
//! codes, `CHGACGCDE` in a job, a spooled file printed, and the `JB` and
//! `SP` entries of `QSYS/QACGJRN` read through IBM's model files
//! `QAJBACG` and `QAPTACG`.

use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

const LONG: Duration = Duration::from_secs(60);

fn want(out: &str, what: &str, words: &[&str]) -> Result<(), String> {
    if words.iter().all(|w| out.contains(w)) {
        Ok(())
    } else {
        Err(format!("{what}: want {words:?}:\n{out}"))
    }
}

/// The report's lines of one status.
fn status_lines<'a>(out: &'a str, status: &str) -> Vec<&'a str> {
    out.lines()
        .filter(|l| {
            l.split_whitespace().nth(2) == Some(status) || l.contains(&format!(" {status} "))
        })
        .filter(|l| !l.contains("checks:"))
        .collect()
}

/// Wait until `cmd`'s answer contains `word`.
fn wait_for(s: &mut Session, cmd: &str, word: &str, secs: u64) -> Result<String, String> {
    let until = Instant::now() + Duration::from_secs(secs);
    let mut out = String::new();
    while Instant::now() < until {
        out = s.command(cmd, CMD)?;
        if out.contains(word) {
            return Ok(out);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err(format!("{cmd}: no {word} after {secs}s:\n{out}"))
}

#[allow(
    clippy::unused_self,
    clippy::many_single_char_names,
    clippy::too_many_lines
)] // the steps share one signature
impl Suite {
    /// Step 107: `QSSECCHK` clean, then a default password caught.
    pub(crate) fn security_check(&self, s: &mut Session) -> Result<String, String> {
        let out = s.command("QSSECCHK OUTPUT(*)", LONG)?;
        want(&out, "QSSECCHK", &["System Security Check", "checks:"])?;
        let fails = status_lines(&out, "FAIL");
        if !fails.is_empty() {
            return Err(format!(
                "QSSECCHK failures on a fresh system:\n{}\n\n{out}",
                fails.join("\n")
            ));
        }
        let warns: Vec<String> = status_lines(&out, "WARN")
            .iter()
            .filter_map(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                Some(format!("{}:{}", w.first()?, w.get(1)?))
            })
            .collect();
        let total = out
            .lines()
            .find(|l| l.contains("checks:"))
            .unwrap_or_default()
            .trim()
            .to_string();
        // A profile whose password is its name is a failure.
        let made = s.command(
            "CRTUSRPRF USRPRF(SECCHECK1) PASSWORD(SECCHECK1) PWDEXP(*NO)",
            CMD,
        )?;
        let caught = if made.contains("created") {
            let again = s.command("QSSECCHK OUTPUT(*)", LONG)?;
            let _ = s.command("DLTUSRPRF USRPRF(SECCHECK1)", CMD);
            let hit = status_lines(&again, "FAIL")
                .iter()
                .any(|l| l.contains("SECCHECK1"));
            if !hit {
                return Err(format!(
                    "QSSECCHK missed SECCHECK1's default password:\n{again}"
                ));
            }
            "default password caught"
        } else {
            "default password not tried (CRTUSRPRF refused it)"
        };
        let printed = s.command("QSSECCHK", LONG)?;
        want(&printed, "QSSECCHK OUTPUT(*PRINT)", &["QPSECCHK printed"])?;
        Ok(format!("{total} warnings {}; {caught}", warns.join(" ")))
    }

    /// Step 108: job accounting end to end.
    pub(crate) fn job_accounting(&self, s: &mut Session) -> Result<String, String> {
        let _ = s.command("CRTJRNRCV JRNRCV(QSYS/ACGRCV0001) AUT(*EXCLUDE)", CMD);
        let j = s.command(
            "CRTJRN JRN(QSYS/QACGJRN) JRNRCV(QSYS/ACGRCV0001) AUT(*EXCLUDE)",
            CMD,
        )?;
        want(&j, "CRTJRN QACGJRN", &["QACGJRN"])?;
        let v = s.command("CHGSYSVAL SYSVAL(QACGLVL) VALUE('*JOB *PRINT')", CMD)?;
        let mut shown = s.command("svc.sysval QACGLVL", CMD)?;
        if !shown.contains("*PRINT") {
            let _ = s.command("svc.sysval QACGLVL *JOB *PRINT", CMD)?;
            shown = format!(
                "CHGSYSVAL did not set it; {}",
                s.command("svc.sysval QACGLVL", CMD)?
            );
        }
        want(
            &shown,
            &format!("QACGLVL after CHGSYSVAL ({v})"),
            &["*JOB", "*PRINT"],
        )?;
        let d = s.command(
            "CRTJOBD JOBD(QGPL/ACGJOBD) ACGCDE(DEPTJOBD) TEXT('accounting')",
            CMD,
        )?;
        want(&d, "CRTJOBD ACGCDE", &["ACGJOBD"])?;
        let shown = s.command("svc.dspjobd QGPL/ACGJOBD", CMD)?;
        want(
            &shown,
            "the job description's accounting code",
            &["DEPTJOBD"],
        )?;
        let _ = s.command("strsbs QSYS/QBATCH", CMD);
        s.command(
            "SBMJOB CMD(DLYJOB DLY(1)) JOB(ACGJOB1) JOBQ(QGPL/QBATCH) ACGCDE(DEPT42)",
            CMD,
        )?;
        s.command(
            "SBMJOB CMD(CHGACGCDE ACGCDE(SECOND)) JOB(ACGJOB2) JOBQ(QGPL/QBATCH) ACGCDE(FIRST)",
            CMD,
        )?;
        // The console job's own code, then a report it prints.
        let c = s.command("CHGACGCDE ACGCDE(PRINTACG)", CMD)?;
        want(&c, "CHGACGCDE", &["Accounting code changed"])?;
        s.command("strsbs QSYS/QSPL", CMD)?;
        let _ = s.command("strprtwtr DEV(QSCAPTURE)", CMD);
        wait_for(s, "wrkwtr", "QSCAPTURE", 20)?;
        let printed = s.command("PRTSYSSECA", LONG)?;
        want(&printed, "PRTSYSSECA", &["printed"])?;
        s.command(
            "chgsplfa QPSYSSEC JOB(QSVCJOB) SPLNBR(*LAST) OUTQ(QUSRSYS/QSCAPTURE)",
            CMD,
        )?;
        // Both jobs ended and the file printed.
        let jrn = "DSPJRN JRN(QSYS/QACGJRN) JRNCDE(A)";
        wait_for(s, jrn, "SP", 60)?;
        let until = Instant::now() + Duration::from_secs(60);
        let mut all = String::new();
        while Instant::now() < until {
            all = s.command(jrn, CMD)?;
            if all.contains("ACGJOB1") && all.matches("ACGJOB2").count() >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        if !all.contains("ACGJOB1") || all.matches("ACGJOB2").count() < 2 {
            let mut why = String::new();
            for cmd in ["wrkjobq QGPL/QBATCH", "wrksbs", "wrkactjob", "svc.txn stats"] {
                why.push_str(&s.command(cmd, CMD).unwrap_or_default());
            }
            return Err(format!(
                "the batch jobs left no JB entries in 60s:\n{all}\n{why}"
            ));
        }
        // Through the model files.
        let mk = s.command(
            "svc.sql CREATE TABLE QGPL.ACGJB AS (SELECT * FROM QSYS.QAJBACG) WITH NO DATA;",
            LONG,
        )?;
        let mk = if s
            .command("svc.sql SELECT COUNT(*) FROM QGPL.ACGJB;", LONG)?
            .contains("row(s)")
        {
            mk
        } else {
            format!(
                "{mk}; {}",
                s.command(
                    "CPYF FROMFILE(QSYS/QAJBACG) TOFILE(QGPL/ACGJB) CRTFILE(*YES)",
                    LONG
                )?
            )
        };
        let w = s.command(
            "DSPJRN JRN(QSYS/QACGJRN) JRNCDE(A) ENTTYP(JB) OUTPUT(*OUTFILE) OUTFILE(QGPL/ACGJB)",
            LONG,
        )?;
        want(
            &w,
            &format!("DSPJRN JB outfile ({mk})"),
            &["entries written"],
        )?;
        // The heading's sequence number is the model's JASEQN.
        let jb = s.command(
            "svc.sql SELECT JAJOB, JACDE, JATYPE, JACCDE, JARTGS, JACPU FROM QGPL.ACGJB WHERE JAJOB LIKE 'ACGJOB%' ORDER BY JASEQN;",
            LONG,
        )?;
        want(
            &jb,
            &format!("JB entries in QAJBACG's format\n{all}"),
            &["ACGJOB1", "DEPT42", "ACGJOB2", "FIRST", "SECOND"],
        )?;
        let rows: Vec<&str> = jb.lines().filter(|l| l.contains("ACGJOB")).collect();
        if !rows
            .iter()
            .all(|l| l.split_whitespace().nth(2) == Some("B"))
        {
            return Err(format!("JATYPE is not B for the batch jobs:\n{jb}"));
        }
        let first = rows.iter().position(|l| l.contains("FIRST"));
        let second = rows.iter().position(|l| l.contains("SECOND"));
        if first >= second {
            return Err(format!("ACGJOB2's segments out of order:\n{jb}"));
        }
        let _ = s.command(
            "svc.sql CREATE TABLE QGPL.ACGSP AS (SELECT * FROM QSYS.QAPTACG) WITH NO DATA;",
            LONG,
        )?;
        if !s
            .command("svc.sql SELECT COUNT(*) FROM QGPL.ACGSP;", LONG)?
            .contains("row(s)")
        {
            let _ = s.command(
                "CPYF FROMFILE(QSYS/QAPTACG) TOFILE(QGPL/ACGSP) CRTFILE(*YES)",
                LONG,
            )?;
        }
        let w = s.command(
            "DSPJRN JRN(QSYS/QACGJRN) JRNCDE(A) ENTTYP(SP) OUTPUT(*OUTFILE) OUTFILE(QGPL/ACGSP)",
            LONG,
        )?;
        want(&w, "DSPJRN SP outfile", &["entries written"])?;
        let sp = s.command(
            "svc.sql SELECT JASPFN, JACDE, JADFN, JATPAG, JATLIN FROM QGPL.ACGSP;",
            LONG,
        )?;
        want(
            &sp,
            "the SP entry in QAPTACG's format",
            &["QPSYSSEC", "PRINTACG", "QSYSPRT"],
        )?;
        // As found.
        let _ = s.command("CHGACGCDE ACGCDE(*BLANK)", CMD);
        let _ = s.command("CHGSYSVAL SYSVAL(QACGLVL) VALUE(*NONE)", CMD);
        let _ = s.command("svc.sql DROP TABLE QGPL.ACGJB;", LONG);
        let _ = s.command("svc.sql DROP TABLE QGPL.ACGSP;", LONG);
        let _ = s.command("DLTJOBD JOBD(QGPL/ACGJOBD)", CMD);
        let _ = s.command("DLTJRN JRN(QSYS/QACGJRN)", CMD);
        let _ = s.command("DLTJRNRCV JRNRCV(QSYS/ACGRCV0001)", CMD);
        let _ = s.command("endwtr QSCAPTURE OPTION(*IMMED)", CMD);
        Ok(format!(
            "{} JB rows for the test jobs, SP entry for QPSYSSEC under PRINTACG",
            rows.len()
        ))
    }
}

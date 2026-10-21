// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 5 steps of the disk suite: work management.

use std::time::Duration;

use crate::qemu::session::Session;
use crate::steps::Suite;

const CMD: Duration = Duration::from_secs(10);

impl Suite {
    /// Step 16: the active job table lists SCPF and the service console
    /// job, running the command that asked; the job has a `QTEMP` of
    /// its own.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn active_jobs(&self, s: &mut Session) -> Result<String, String> {
        let list = s.command("wrkactjob", CMD)?;
        let row = |name: &str| {
            list.lines()
                .find(|l| l.starts_with(name))
                .map(str::to_string)
        };
        let scpf = row("SCPF").ok_or_else(|| format!("no SCPF:\n{list}"))?;
        if !scpf.contains("SYS") {
            return Err(format!("SCPF is not a system job: {scpf}"));
        }
        let svc = row("QSVCJOB").ok_or_else(|| format!("no QSVCJOB:\n{list}"))?;
        if !svc.contains("RUN") || !svc.contains("CMD-WRKACTJOB") {
            return Err(format!("QSVCJOB is not running wrkactjob: {svc}"));
        }
        let made = s.command("obj crt *DTAARA QTEMP/WMQTEMP", CMD)?;
        let shown = s.command("obj ls QTEMP", CMD)?;
        if !shown.contains("WMQTEMP") {
            return Err(format!("QTEMP does not hold the area:\n{made}{shown}"));
        }
        let number = svc
            .split_whitespace()
            .nth(2)
            .unwrap_or_default()
            .to_string();
        Ok(format!(
            "SCPF and QSVCJOB {number} active; QTEMP is the job's own"
        ))
    }
}

impl Suite {
    /// Step 17: shipped job descriptions and classes, `CRTJOBD`,
    /// `CHGJOB` and `DSPJOB`.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn job_descriptions(&self, s: &mut Session) -> Result<String, String> {
        let shipped = s.command("dspjobd QGPL/QDFTJOBD", CMD)?;
        if !shipped.contains("Default job description") || !shipped.contains("QGPL/QBATCH") {
            return Err(format!("QDFTJOBD:\n{shipped}"));
        }
        let cls = s.command("dspcls QGPL/QINTER", CMD)?;
        if !cls
            .lines()
            .any(|l| l.contains("Run priority") && l.trim_end().ends_with("20"))
        {
            return Err(format!("QINTER class:\n{cls}"));
        }
        let made = s.command(
            "crtjobd QGPL/WMTEST USER(QSECOFR) JOBPTY(3) LOG(4 0 *SECLVL) TEXT('Harness')",
            CMD,
        )?;
        if !made.contains("created") {
            return Err(format!("crtjobd: {made}"));
        }
        let shown = s.command("dspjobd QGPL/WMTEST", CMD)?;
        if !shown.contains("QSECOFR") || !shown.contains("*SECLVL") {
            return Err(format!("dspjobd:\n{shown}"));
        }
        let bad = s.command("chgjobd QGPL/WMTEST JOBPTY(12)", CMD)?;
        if !bad.contains("CPD0085") {
            return Err(format!("chgjobd accepted JOBPTY(12): {bad}"));
        }
        let changed = s.command("chgjob LOG(2 20 *SECLVL) SWS(10000001)", CMD)?;
        if !changed.contains("changed") {
            return Err(format!("chgjob: {changed}"));
        }
        let job = s.command("dspjob", CMD)?;
        if !job.contains("2 20 *SECLVL") || !job.contains("10000001") {
            return Err(format!("dspjob:\n{job}"));
        }
        let gone = s.command("dltjobd QGPL/WMTEST", CMD)?;
        if !gone.contains("deleted") {
            return Err(format!("dltjobd: {gone}"));
        }
        Ok("QDFTJOBD and QINTER shipped; created, shown, rejected, changed the job".into())
    }
}

impl Suite {
    /// Step 18: the job's library list from `QSYSLIBL` and `QUSRLIBL`,
    /// changed by the list commands and followed by name resolution;
    /// the job's own `*LDA`.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn library_list(&self, s: &mut Session) -> Result<String, String> {
        let first = s.command("dsplibl", CMD)?;
        let part = |list: &str, lib: &str| {
            list.lines()
                .find(|l| l.split_whitespace().next() == Some(lib))
                .and_then(|l| l.split_whitespace().nth(1))
                .map(str::to_string)
        };
        if part(&first, "QSYS").as_deref() != Some("SYS")
            || part(&first, "QGPL").as_deref() != Some("USR")
        {
            return Err(format!("initial library list:\n{first}"));
        }
        s.command("addlible QRCL POSITION(*LAST)", CMD)?;
        let dup = s.command("chgcurlib QRCL", CMD)?;
        if !dup.contains("CPF2103") {
            return Err(format!("chgcurlib accepted a library on the list: {dup}"));
        }
        s.command("rmvlible QRCL", CMD)?;
        let cur = s.command("chgcurlib QRCL", CMD)?;
        if !cur.contains("changed") {
            return Err(format!("chgcurlib: {cur}"));
        }
        let made = s.command("obj crt *DTAARA *CURLIB/WMCUR", CMD)?;
        let listed = s.command("obj ls QRCL", CMD)?;
        let second = s.command("dsplibl", CMD)?;
        s.command("chgcurlib *CRTDFT", CMD)?;
        s.command("obj dlt *DTAARA QRCL/WMCUR", CMD)?;
        if part(&second, "QRCL").as_deref() != Some("CUR") {
            return Err(format!("QRCL is not the current library:\n{second}"));
        }
        if !listed.contains("WMCUR") {
            return Err(format!("*CURLIB did not go to QRCL:\n{made}{listed}"));
        }
        s.command("dtaara set *LDA HELLO FROM THE JOB", CMD)?;
        let lda = s.command("dtaara get *LDA", CMD)?;
        if !lda.contains("'HELLO FROM THE JOB'") {
            return Err(format!("*LDA: {lda}"));
        }
        Ok("QSYSLIBL and QUSRLIBL at start; list commands; *CURLIB followed; *LDA kept".into())
    }
}

impl Suite {
    /// Step 19: the job log holds the start message and the requests
    /// the console job ran, and `LOG` level 1 keeps only messages.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn job_log(&self, s: &mut Session) -> Result<String, String> {
        // Step 17 left LOG(2 20 *SECLVL); level 4 keeps everything.
        s.command("chgjob LOG(4 0 *NOLIST)", CMD)?;
        let log = s.command("dspjoblog", CMD)?;
        if !log.lines().any(|l| l.starts_with("CPF1124")) {
            return Err(format!("no CPF1124 in the job log:\n{log}"));
        }
        let requests = log
            .lines()
            .filter(|l| l.starts_with(">>") && l.contains("Request"))
            .count();
        if !log
            .lines()
            .any(|l| l.starts_with(">>") && l.contains("wrkactjob"))
        {
            return Err(format!("the wrkactjob request is not logged:\n{log}"));
        }
        s.command("chgjob LOG(1 0 *MSG)", CMD)?;
        let quiet = s.command("dspjoblog", CMD)?;
        s.command("chgjob LOG(4 0 *NOLIST)", CMD)?;
        if quiet.lines().any(|l| l.starts_with(">>")) {
            return Err(format!("LOG level 1 kept requests:\n{quiet}"));
        }
        Ok(format!(
            "CPF1124 and {requests} requests logged; level 1 keeps messages only"
        ))
    }
}

impl Suite {
    /// Step 20: message queues: shipped `QSYSOPR`, send and receive, an
    /// inquiry answered, and a break message delivered to the job.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn message_queues(&self, s: &mut Session) -> Result<String, String> {
        let opr = s.command("dspmsg QSYSOPR", CMD)?;
        if !opr.contains("QSYSOPR") || !opr.contains("QSYS") {
            return Err(format!("dspmsg QSYSOPR:\n{opr}"));
        }
        s.command("crtmsgq QGPL/WMQ TEXT('Harness queue')", CMD)?;
        s.command("sndmsg MSG('Hello queue') TOMSGQ(QGPL/WMQ)", CMD)?;
        let got = s.command("rcvmsg MSGQ(QGPL/WMQ)", CMD)?;
        if !got.contains("Hello queue") {
            return Err(format!("rcvmsg: {got}"));
        }
        let sent = s.command(
            "sndmsg MSG('Please answer') TOMSGQ(QGPL/WMQ) MSGTYPE(*INQ)",
            CMD,
        )?;
        let key = sent
            .split("key ")
            .nth(1)
            .map(|k| k.trim().trim_end_matches('.').to_string())
            .ok_or_else(|| format!("sndmsg *INQ: {sent}"))?;
        let waiting = s.command("dspmsg QGPL/WMQ", CMD)?;
        if !waiting.contains("(waiting") {
            return Err(format!("the inquiry is not waiting:\n{waiting}"));
        }
        s.command(
            &format!("sndrpymsg MSGKEY({key}) MSGQ(QGPL/WMQ) RPY('YES')"),
            CMD,
        )?;
        let answered = s.command("dspmsg QGPL/WMQ", CMD)?;
        if !answered.contains("Reply . . . :   YES") {
            return Err(format!("the reply is not shown:\n{answered}"));
        }
        s.command("chgmsgq QGPL/WMQ DLVRY(*BREAK)", CMD)?;
        s.command("sndbrkmsg MSG('Break now') TOMSGQ(QGPL/WMQ)", CMD)?;
        s.expect("Break message on QGPL/WMQ", Duration::from_secs(5))?;
        s.expect("Break now", CMD)?;
        s.command("dltmsgq QGPL/WMQ", CMD)?;
        Ok(format!(
            "sent, received, inquiry {key} answered, break delivered"
        ))
    }
}

impl Suite {
    /// Step 21: the history log has the IPLs, the job starts, the
    /// system value change of step 3, and what is sent to `QHST`;
    /// `MSGID` selects.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn history_log(&self, s: &mut Session) -> Result<String, String> {
        s.command("sndmsg MSG('History test entry') TOMSGQ(QHST)", CMD)?;
        let all = s.command("dsplog", CMD)?;
        for want in ["CPF0900", "CPF1124", "CPF1806", "History test entry"] {
            if !all.contains(want) {
                return Err(format!("{want} is not in the history log:\n{all}"));
            }
        }
        let ipls = all.lines().filter(|l| l.contains("CPF0900")).count();
        let starts = s.command("dsplog MSGID(CPF1124)", CMD)?;
        if starts.contains("History test entry") || !starts.contains("QSVCJOB") {
            return Err(format!("dsplog MSGID(CPF1124):\n{starts}"));
        }
        Ok(format!(
            "{ipls} IPL entries, job starts, the QSECURITY change, a sent entry"
        ))
    }
}

impl Suite {
    /// Step 22: the system reply list: shipped entries, and a job with
    /// `INQMSGRPY(*SYSRPYL)` whose function-check inquiry the list
    /// answers.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn reply_list(&self, s: &mut Session) -> Result<String, String> {
        let list = s.command("wrkrpyle", CMD)?;
        for id in ["CPA0700", "RPG0000", "CBE0000", "CPA5305"] {
            if !list.contains(id) {
                return Err(format!("{id} is not shipped:\n{list}"));
            }
        }
        s.command(
            "addrpyle SEQNBR(1) MSGID(CEE9901) CMPDTA('FAILPGM' 1) RPY(D)",
            CMD,
        )?;
        s.command("chgjob INQMSGRPY(*SYSRPYL)", CMD)?;
        s.command("call QGPL/FAILPGM", CMD)?;
        let q = s.command("dspmsgq *EXT", CMD)?;
        s.command("chgjob INQMSGRPY(*DFT)", CMD)?;
        s.command("rmvrpyle SEQNBR(1)", CMD)?;
        let newest = q
            .lines()
            .rev()
            .find(|l| l.contains("reply:"))
            .unwrap_or_default()
            .to_string();
        if !newest.contains("reply: D") {
            return Err(format!("the reply list did not answer D:\n{q}"));
        }
        Ok("shipped entries listed; CEE9901 answered D from entry 1".into())
    }
}

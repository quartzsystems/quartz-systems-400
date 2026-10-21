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
                .find(|l| l.trim_start().starts_with(name))
                .map(str::to_string)
        };
        let scpf = row("SCPF").ok_or_else(|| format!("no SCPF:\n{list}"))?;
        if !scpf.contains("SYS") {
            return Err(format!("SCPF is not a system job: {scpf}"));
        }
        for name in [
            "QSYSARB",
            "QSYSARB5",
            "QLUS",
            "QDBSRV01",
            "QDBSRV05",
            "QSPLMAINT",
            "QALERT",
            "QSYSCOMM1",
        ] {
            let r = row(&format!("{name} ")).ok_or_else(|| format!("no {name}:\n{list}"))?;
            if !r.contains("SYS") {
                return Err(format!("{name} is not a system job: {r}"));
            }
        }
        for sbs in ["QSYSWRK", "QBASE", "QSPL", "QSERVER", "QUSRWRK"] {
            let r =
                row(&format!("{sbs} ")).ok_or_else(|| format!("{sbs} is not active:\n{list}"))?;
            if !r.contains("SBS") {
                return Err(format!("{sbs} is not a subsystem monitor: {r}"));
            }
        }
        if row("QSYSSCD").is_none() || row("QSTRUPJD").is_some() {
            return Err(format!(
                "QSYSSCD should run and QSTRUPJD have ended:\n{list}"
            ));
        }
        let hist = s.command("dsplog MSGID(CPF0900)", CMD)?;
        if !hist.contains("CPF0900") {
            return Err(format!("no CPF0900 in QHST:\n{hist}"));
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
        let numbers = s.command("wrkactjob VIEW(3)", CMD)?;
        let number = numbers
            .lines()
            .find(|l| l.trim_start().starts_with("QSVCJOB"))
            .and_then(|l| l.split_whitespace().nth(2))
            .unwrap_or_default()
            .to_string();
        Ok(format!(
            "SCPF, 14 system jobs, QSYSWRK, QBASE and the startup subsystems; QSVCJOB {number}; QTEMP is the job's own"
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

impl Suite {
    /// Step 23: subsystems: a description with a private pool, a routing
    /// entry and an autostart job; `STRSBS` starts the monitor, the job
    /// runs routed into the pool, `ENDSBS *IMMED` ends both.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn subsystems(&self, s: &mut Session) -> Result<String, String> {
        let rtg = s.command("dspsbsd QSYS/QBATCH OPTION(7)", CMD)?;
        if !rtg.contains("QCMDB") {
            return Err(format!("QBATCH routing entries:\n{rtg}"));
        }
        for c in [
            "crtsbsd QGPL/WMSBS POOLS((1 *BASE) (2 8192 2)) TEXT('Harness subsystem')",
            "addrtge QGPL/WMSBS SEQNBR(10) CMPVAL(WMRTG) PGM(QSYS/QCMD) CLS(QGPL/QBATCH) POOLID(2)",
            "crtjobd QGPL/WMAJD RTGDTA(WMRTG) RQSDTA('dlyjob DLY(60)')",
            "addaje QGPL/WMSBS JOB(WMAUTO) JOBD(QGPL/WMAJD)",
        ] {
            let out = s.command(c, CMD)?;
            if !out.contains("created") && !out.contains("changed") {
                return Err(format!("{c}: {out}"));
            }
        }
        let started = s.command("strsbs QGPL/WMSBS", CMD)?;
        if !started.contains("started") {
            return Err(format!("strsbs: {started}"));
        }
        let sbs = s.command("wrksbs", CMD)?;
        if !sbs.contains("WMSBS") {
            return Err(format!("wrksbs:\n{sbs}"));
        }
        let mut active = String::new();
        for _ in 0..20 {
            active = s.command("wrkactjob", CMD)?;
            if active
                .lines()
                .any(|l| l.trim_start().starts_with("WMAUTO") && l.contains("DLYW"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let auto = active
            .lines()
            .find(|l| l.trim_start().starts_with("WMAUTO"))
            .ok_or_else(|| format!("the autostart job is not active:\n{active}"))?;
        if !auto.contains("ASJ") || !auto.contains("DLYW") {
            return Err(format!("the autostart job is not waiting: {auto}"));
        }
        if !active
            .lines()
            .any(|l| l.trim_start().starts_with("WMSBS") && l.contains("SBS"))
        {
            return Err(format!("the monitor is not listed:\n{active}"));
        }
        let ended = s.command("endsbs WMSBS OPTION(*IMMED)", CMD)?;
        if !ended.contains("ending") {
            return Err(format!("endsbs: {ended}"));
        }
        let mut gone = false;
        for _ in 0..40 {
            let now = s.command("wrkactjob", CMD)?;
            if !now.contains("WMAUTO") && !now.contains("WMSBS") {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !gone {
            return Err("the subsystem's jobs did not end".into());
        }
        let hist = s.command("dsplog MSGID(CPF0902 CPF0903)", CMD)?;
        if !hist.contains("WMSBS") {
            return Err(format!(
                "the subsystem start and end are not logged:\n{hist}"
            ));
        }
        s.command("dltsbsd QGPL/WMSBS", CMD)?;
        s.command("dltjobd QGPL/WMAJD", CMD)?;
        Ok("monitor started, autostart job routed and delayed, ENDSBS *IMMED ended both".into())
    }
}

impl Suite {
    /// Step 24: sign on at the console: an interactive job routed
    /// through the controlling subsystem to `QCMD`'s Command Entry runs a
    /// command typed on its line; `SIGNOFF` brings the sign-on back.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn console_sign_on(&self, s: &mut Session) -> Result<String, String> {
        let screen_has = |s: &mut Session, want: &str| -> Result<String, String> {
            let mut last = String::new();
            for _ in 0..40 {
                last = s.command("screen", CMD)?;
                if last.contains(want) {
                    return Ok(last);
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(format!("the console never showed {want:?}:\n{last}"))
        };
        screen_has(s, "Sign On")?;
        s.command("type QSECOFR", CMD)?;
        s.command("key enter", CMD)?;
        screen_has(s, "Command Entry")?;
        s.command("type wrkactjob", CMD)?;
        s.command("key enter", CMD)?;
        // The output, which lists the job running it; with the system
        // jobs it is long enough to scroll the command line away.
        screen_has(s, "QSYSARB")?;
        let active = s.command("wrkactjob", CMD)?;
        let row = active
            .lines()
            .find(|l| l.trim_start().starts_with("QSCONSOLE"))
            .ok_or_else(|| format!("no QSCONSOLE job:\n{active}"))?
            .to_string();
        if !row.contains("QSECOFR") || !row.contains("INT") {
            return Err(format!(
                "the console job is not QSECOFR's interactive job: {row}"
            ));
        }
        s.command("type signoff", CMD)?;
        s.command("key enter", CMD)?;
        screen_has(s, "Sign On")?;
        let log = s.command("dsplog JOB(QSCONSOLE)", CMD)?;
        if !log.contains("CPF1124") || !log.contains("CPF1164") {
            return Err(format!(
                "the console job's start and end are not logged:\n{log}"
            ));
        }
        Ok(format!(
            "signed on, Command Entry ran wrkactjob, signed off: {}",
            row.trim()
        ))
    }
}

/// The `number/user/name` in `SBMJOB`'s message.
fn submitted(out: &str) -> Result<String, String> {
    out.split_whitespace()
        .nth(1)
        .filter(|j| j.split('/').count() == 3)
        .map(str::to_string)
        .ok_or_else(|| format!("sbmjob: {out}"))
}

impl Suite {
    /// Step 25: job queues: `SBMJOB` onto a held queue, released, the
    /// job started by `QBATCH`'s monitor; a held job ended while queued;
    /// `ENDJOB *IMMED` of the running job and a job that completes, each
    /// with its completion message.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn job_queues(&self, s: &mut Session) -> Result<String, String> {
        let queues = s.command("wrkjobq", CMD)?;
        if !queues.contains("QBATCH") || !queues.contains("QSYSNOMAX") {
            return Err(format!("wrkjobq:\n{queues}"));
        }
        s.command("strsbs QSYS/QBATCH", CMD)?;
        s.command("hldjobq QGPL/QBATCH", CMD)?;
        let out = s.command(
            "sbmjob CMD('dlyjob DLY(60)') JOB(WMDLY) JOBQ(QGPL/QBATCH) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let dly = submitted(&out)?;
        let waiting = s.command("wrkjobq QGPL/QBATCH", CMD)?;
        if !waiting.contains("WMDLY") || !waiting.contains("HLD") || !waiting.contains("QBATCH") {
            return Err(format!(
                "the job is not waiting on the held queue:\n{waiting}"
            ));
        }
        let held = submitted(&s.command(
            "sbmjob CMD('wrkactjob') JOB(WMHELD) JOBQ(QGPL/QBATCH) HOLD(*YES)",
            CMD,
        )?)?;
        let gone = s.command(&format!("endjob JOB({held})"), CMD)?;
        if !gone.contains("ended while on job queue") {
            return Err(format!("endjob of a queued job: {gone}"));
        }
        s.command("rlsjobq QGPL/QBATCH", CMD)?;
        let mut active = String::new();
        for _ in 0..30 {
            active = s.command("wrkactjob", CMD)?;
            if active
                .lines()
                .any(|l| l.trim_start().starts_with("WMDLY") && l.contains("DLYW"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let row = active
            .lines()
            .find(|l| l.trim_start().starts_with("WMDLY"))
            .ok_or_else(|| format!("WMDLY did not start:\n{active}"))?
            .to_string();
        if !row.contains("BCH") || !row.contains("DLYW") {
            return Err(format!("WMDLY is not a delayed batch job: {row}"));
        }
        s.command(&format!("endjob JOB({dly}) OPTION(*IMMED)"), CMD)?;
        let quick = submitted(&s.command(
            "sbmjob CMD('wrkactjob') JOB(WMQUICK) JOBQ(QGPL/QBATCH) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?)?;
        let mut msgs = String::new();
        for _ in 0..40 {
            msgs = s.command("dspmsg QUSRSYS/QSECOFR", CMD)?;
            if msgs.contains("CPF1240") && msgs.contains("CPF1241") {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        if !msgs.contains("CPF1240") || !msgs.contains("WMDLY") {
            return Err(format!("no abnormal end message for {dly}:\n{msgs}"));
        }
        if !msgs.contains("CPF1241") || !msgs.contains("WMQUICK") {
            return Err(format!("no completion message for {quick}:\n{msgs}"));
        }
        Ok(format!(
            "{dly} held then started by QBATCH, ended *IMMED; {held} ended on the queue; {quick} completed"
        ))
    }

    /// Step 26: the job scheduler: `QSYSWRK` starts `QSYSSCD`; a once
    /// entry due a few seconds ahead is submitted and completes; a
    /// weekly entry is held and removed.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn job_scheduler(&self, s: &mut Session) -> Result<String, String> {
        s.command("strsbs QSYS/QSYSWRK", CMD)?;
        let mut active = String::new();
        for _ in 0..20 {
            active = s.command("wrkactjob", CMD)?;
            if active.lines().any(|l| l.trim_start().starts_with("QSYSSCD")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let row = active
            .lines()
            .find(|l| l.trim_start().starts_with("QSYSSCD"))
            .ok_or_else(|| {
                format!(
                    "QSYSSCD did not start:
{active}"
                )
            })?
            .to_string();
        if !row.contains("ASJ") {
            return Err(format!("QSYSSCD is not an autostart job: {row}"));
        }
        let (date, time) = utc_in(15);
        s.command(
            &format!(
                "addjobscde JOB(WMSCD) CMD('wrkactjob') FRQ(*ONCE) SCDDATE({date})                  SCDTIME({time}) JOBQ(QSYS/QSYSNOMAX) MSGQ(QUSRSYS/QSECOFR)"
            ),
            CMD,
        )?;
        s.command(
            "addjobscde JOB(WMWEEK) CMD('wrkactjob') FRQ(*WEEKLY) SCDDAY(*ALL) SCDTIME(235959)",
            CMD,
        )?;
        let held = s.command("hldjobscde WMWEEK", CMD)?;
        let list = s.command("wrkjobscde", CMD)?;
        let week = list
            .lines()
            .find(|l| l.starts_with("WMWEEK"))
            .ok_or_else(|| {
                format!(
                    "WMWEEK not listed:
{list}"
                )
            })?;
        if !held.contains("held") || !week.contains("HLD") || !week.contains("*WEEKLY") {
            return Err(format!(
                "hldjobscde: {held}
{list}"
            ));
        }
        let mut pending = String::new();
        for _ in 0..60 {
            pending = s.command("wrkjobscde", CMD)?;
            if !pending.lines().any(|l| l.starts_with("WMSCD")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if pending.lines().any(|l| l.starts_with("WMSCD")) {
            return Err(format!(
                "WMSCD was not submitted:
{pending}"
            ));
        }
        let mut msgs = String::new();
        for _ in 0..40 {
            msgs = s.command("dspmsg QUSRSYS/QSECOFR", CMD)?;
            if msgs.contains("CPF1241") && msgs.contains("WMSCD") {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let hist = s.command("dsplog MSGID(CPI1E63)", CMD)?;
        s.command("rmvjobscde WMWEEK", CMD)?;
        let after = s.command("wrkjobscde", CMD)?;
        s.command("endsbs QSYSWRK OPTION(*IMMED)", CMD)?;
        if !msgs.contains("CPF1241") || !msgs.contains("WMSCD") {
            return Err(format!(
                "WMSCD did not complete:
{msgs}"
            ));
        }
        if !hist.contains("CPI1E63") || !hist.contains("WMSCD") {
            return Err(format!(
                "no CPI1E63 in QHST:
{hist}"
            ));
        }
        if after.contains("WMWEEK") {
            return Err(format!(
                "rmvjobscde left WMWEEK:
{after}"
            ));
        }
        Ok(format!(
            "QSYSSCD active; WMSCD submitted at {date} {time} and completed; WMWEEK held and removed"
        ))
    }
}

impl Suite {
    /// Step 27: output queues and spooled files: the shipped queues; a
    /// batch job's log spooled as `QPJOBLOG` at its end, displayed, held,
    /// released, moved, copied to a file with FCFC controls and deleted.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn spooled_files(&self, s: &mut Session) -> Result<String, String> {
        let queues = s.command("wrkoutq", CMD)?;
        for q in ["QPRINT", "QEZJOBLOG", "QEZDEBUG"] {
            if !queues.contains(q) {
                return Err(format!(
                    "{q} not shipped:
{queues}"
                ));
            }
        }
        s.command("crtoutq QGPL/WMOUTQ SEQ(*FIFO) TEXT('Harness queue')", CMD)?;
        let desc = s.command("wrkoutqd QGPL/WMOUTQ", CMD)?;
        if !desc.contains("Harness queue") || !desc.contains("*FIFO") {
            return Err(format!(
                "wrkoutqd:
{desc}"
            ));
        }
        s.command("strsbs QSYS/QBATCH", CMD)?;
        let out = s.command(
            "sbmjob CMD('wrkactjob') JOB(WMLOG) JOBQ(QGPL/QBATCH) LOG(4 00 *SECLVL) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let job = submitted(&out)?;
        let mut files = String::new();
        for _ in 0..40 {
            files = s.command(&format!("wrksplf SELECT(*ALL) JOB({job})"), CMD)?;
            if files.contains("QPJOBLOG") && files.contains("RDY") {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        let row = files
            .lines()
            .find(|l| l.contains("QPJOBLOG"))
            .ok_or_else(|| {
                format!(
                    "no QPJOBLOG for {job}:
{files}"
                )
            })?
            .to_string();
        if !row.contains("QEZJOBLOG") || !row.contains("RDY") {
            return Err(format!("QPJOBLOG is not ready on QEZJOBLOG: {row}"));
        }
        let shown = s.command(&format!("dspsplf QPJOBLOG JOB({job})"), CMD)?;
        for want in ["Display Job Log", "CPF1124", "wrkactjob", "CPF1164"] {
            if !shown.contains(want) {
                return Err(format!(
                    "the job log lacks {want}:
{shown}"
                ));
            }
        }
        s.command(&format!("hldsplf QPJOBLOG JOB({job})"), CMD)?;
        let held = s.command(&format!("wrksplf SELECT(*ALL) JOB({job})"), CMD)?;
        s.command(&format!("rlssplf QPJOBLOG JOB({job})"), CMD)?;
        s.command(
            &format!("chgsplfa QPJOBLOG JOB({job}) OUTQ(QGPL/WMOUTQ) COPIES(2)"),
            CMD,
        )?;
        let moved = s.command("wrkoutq QGPL/WMOUTQ", CMD)?;
        if !held.contains("HLD") || !moved.contains("QPJOBLOG") || !moved.contains("RDY") {
            return Err(format!(
                "hold, release and move:
{held}
{moved}"
            ));
        }
        s.command("crtpf QGPL/WMSPLF RCDLEN(133)", CMD)?;
        let copied = s.command(
            &format!("cpysplf QPJOBLOG QGPL/WMSPLF JOB({job}) CTLCHAR(*FCFC)"),
            CMD,
        )?;
        let records = s.command("dsppfm QGPL/WMSPLF NBRRCDS(2)", CMD)?;
        if !copied.contains("records copied") || !records.contains("1QS400") {
            return Err(format!(
                "cpysplf:
{copied}
{records}"
            ));
        }
        let busy = s.command("dltoutq QGPL/WMOUTQ", CMD)?;
        s.command(&format!("dltsplf QPJOBLOG JOB({job})"), CMD)?;
        let gone = s.command("dltoutq QGPL/WMOUTQ", CMD)?;
        s.command("dltf QGPL/WMSPLF", CMD)?;
        if !busy.contains("CPF3417") || !gone.contains("deleted") {
            return Err(format!(
                "dltoutq:
{busy}
{gone}"
            ));
        }
        Ok(format!(
            "{job}'s log spooled to QEZJOBLOG, shown, held, released, moved, copied with FCFC, deleted"
        ))
    }
}

impl Suite {
    /// Step 28: printer files: `CRTPRTF` from DDS, a program printing
    /// through it with the spooled lines matching the in-memory run,
    /// and `OVRPRTF` sending the next run's file elsewhere, held.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn printer_files(&self, s: &mut Session) -> Result<String, String> {
        crate::qir::qemu::push(s, "KITDDS", crate::qir::KITPRT)?;
        s.command("crtsrcpf QGPL/WMSRC", CMD)?;
        s.command(
            "srcput QGPL/WMSRC KITPRT SRCTYPE(PRTF) FROMSTMF(KITDDS)",
            CMD,
        )?;
        let made = s.command("crtprtf QGPL/KITPRT SRCFILE(QGPL/WMSRC)", CMD)?;
        let fd = s.command("dspfd QGPL/KITPRT", CMD)?;
        if !made.contains("3 record format(s)") || !fd.contains("PRTF") || !fd.contains("DETAIL") {
            return Err(format!(
                "crtprtf:
{made}
{fd}"
            ));
        }
        let kit = crate::qir::Kit::locate();
        let program = std::fs::read_to_string(kit.root.join("programs").join("PRT02.qir"))
            .map_err(|e| e.to_string())?;
        crate::qir::qemu::push(s, "PRT02M0", &program)?;
        s.command("qir asm PRT02M0 MODULE(QGPL/PRT02)", CMD)?;
        s.command("crtpgm QGPL/PRT02 MODULE(QGPL/PRT02) ACTGRP(*CALLER)", CMD)?;
        let out = s.command("call QGPL/PRT02", CMD)?;
        let files = s.command("wrksplf SELECT(*ALL) JOB(QSVCJOB)", CMD)?;
        if !files
            .lines()
            .any(|l| l.contains("KITPRT") && l.contains("RDY"))
        {
            return Err(format!(
                "no ready KITPRT after the call:
{out}
{files}"
            ));
        }
        let shown = s.command("dspsplf KITPRT JOB(QSVCJOB) SPLNBR(*LAST)", CMD)?;
        for want in [
            "Kit_report                             Page    1",
            "    ACME            1,234.50  *",
            "    ZENITH              7.50-",
            "    End of report",
        ] {
            if !shown.contains(want) {
                return Err(format!(
                    "the report lacks {want:?}:
{shown}"
                ));
            }
        }
        s.command(
            "ovrprtf FILE(KITPRT) OUTQ(QGPL/QPRINT2) SPLFNAME(WMOVR) HOLD(*YES)",
            CMD,
        )?;
        s.command("call QGPL/PRT02", CMD)?;
        s.command("dltovr FILE(*ALL)", CMD)?;
        let moved = s.command("wrkoutq QGPL/QPRINT2", CMD)?;
        if !moved
            .lines()
            .any(|l| l.contains("WMOVR") && l.contains("HLD"))
        {
            return Err(format!(
                "the override did not take:
{moved}"
            ));
        }
        s.command("dltsplf WMOVR JOB(QSVCJOB) SPLNBR(*LAST)", CMD)?;
        s.command("dltsplf KITPRT JOB(QSVCJOB) SPLNBR(*LAST)", CMD)?;
        let gone = s.command("dltf QGPL/KITPRT", CMD)?;
        s.command("dltf QGPL/WMSRC", CMD)?;
        if !gone.contains("deleted") {
            return Err(format!(
                "dltf of the printer file:
{gone}"
            ));
        }
        Ok("KITPRT created from DDS; PRT02 printed headings, edited amounts, an indicator and an underline; OVRPRTF moved and held the next file".into())
    }
}

impl Suite {
    /// Step 29: writers and the capture device: `STRPRTWTR QSCAPTURE` in
    /// `QSPL`, a spooled file moved to its queue printed to text and a
    /// PDF whose structure checks, the writer held, released and ended.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn writers(&self, s: &mut Session) -> Result<String, String> {
        s.command("strsbs QSYS/QSPL", CMD)?;
        let started = s.command("strprtwtr DEV(QSCAPTURE)", CMD)?;
        let mut wtr = String::new();
        for _ in 0..40 {
            wtr = s.command("wrkwtr", CMD)?;
            if wtr
                .lines()
                .any(|l| l.contains("QSCAPTURE") && l.contains("STR"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !wtr
            .lines()
            .any(|l| l.contains("QSCAPTURE") && l.contains("STR"))
        {
            return Err(format!(
                "the writer did not start:
{started}
{wtr}"
            ));
        }
        s.command(
            "chgsplfa QSYSPRT JOB(QSVCJOB) SPLNBR(*LAST) OUTQ(QUSRSYS/QSCAPTURE) SAVE(*YES)",
            CMD,
        )?;
        let mut files = String::new();
        for _ in 0..40 {
            files = s.command("wrksplf SELECT(*ALL) JOB(QSVCJOB)", CMD)?;
            if files
                .lines()
                .any(|l| l.contains("QSYSPRT") && l.contains("SAV"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !files
            .lines()
            .any(|l| l.contains("QSYSPRT") && l.contains("SAV"))
        {
            return Err(format!(
                "QSYSPRT was not printed:
{files}"
            ));
        }
        let text = s.command("prtcapture get QSYSPRT JOB(QSVCJOB)", CMD)?;
        if !text.contains("00A") || !text.contains("06E") {
            return Err(format!(
                "the captured text:
{text}"
            ));
        }
        let pdf_out = s.command("prtcapture get QSYSPRT JOB(QSVCJOB) FORMAT(*PDF)", CMD)?;
        let b64: String = pdf_out
            .lines()
            .skip_while(|l| !l.starts_with("-----BEGIN PDF"))
            .skip(1)
            .take_while(|l| !l.starts_with("-----END PDF"))
            .collect();
        let pdf = unbase64(&b64).ok_or("the PDF is not base64")?;
        let pages = spool::pdf::check(&pdf).map_err(|e| format!("the PDF: {e}"))?;
        s.command("hldwtr QSCAPTURE", CMD)?;
        let held = s.command("wrkwtr", CMD)?;
        s.command("rlswtr QSCAPTURE", CMD)?;
        s.command("endwtr QSCAPTURE", CMD)?;
        let mut after = String::new();
        for _ in 0..40 {
            after = s.command("wrkwtr", CMD)?;
            if !after.contains("QSCAPTURE") {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        s.command("endsbs QSPL OPTION(*IMMED)", CMD)?;
        s.command("dltsplf QSYSPRT JOB(QSVCJOB) SPLNBR(*LAST)", CMD)?;
        s.command("prtcapture clear", CMD)?;
        if !held
            .lines()
            .any(|l| l.contains("QSCAPTURE") && l.contains("HLD"))
        {
            return Err(format!(
                "hldwtr:
{held}"
            ));
        }
        if after.contains("QSCAPTURE") {
            return Err(format!(
                "endwtr:
{after}"
            ));
        }
        Ok(format!(
            "QSCAPTURE printed QSYSPRT: text and a {}-byte PDF of {pages} page(s); held, released, ended",
            pdf.len()
        ))
    }
}

/// Base64 back to bytes.
fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        let pad = chunk.iter().rev().take_while(|b| **b == b'=').count();
        let mut n = 0u32;
        for &c in chunk {
            n = (n << 6) | if c == b'=' { 0 } else { val(c)? };
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - pad]);
    }
    Some(out)
}

/// Wait for the console display to show `want`; the screen.
fn screen_wait(s: &mut Session, want: &str) -> Result<String, String> {
    let mut last = String::new();
    for _ in 0..80 {
        last = s.command("screen", CMD)?;
        if last.contains(want) {
            return Ok(last);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!("the console never showed {want:?}:\n{last}"))
}

impl Suite {
    /// Step 30: `PWRDWNSYS *IMMED RESTART(*YES)` typed at Command Entry:
    /// the subsystems and system jobs end, the store closes, the system
    /// IPLs again normally with `QIPLSTS` 2.
    pub(crate) fn power_down(&self, s: &mut Session) -> Result<String, String> {
        screen_wait(s, "Sign On")?;
        s.command("type QSECOFR", CMD)?;
        s.command("key enter", CMD)?;
        screen_wait(s, "Command Entry")?;
        s.command("type pwrdwnsys OPTION(*IMMED) RESTART(*YES)", CMD)?;
        s.send("key enter")?;
        for code in [
            "SRC D1001000",
            "SRC D1001005",
            "SRC D1001007",
            "SRC D1001010",
            "SRC D1001090",
        ] {
            s.expect(code, Duration::from_secs(60))?;
        }
        let t0 = s.elapsed();
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let boot = s.elapsed().saturating_sub(t0);
        let hist = s.command("hist", CMD)?;
        let entries: Vec<&str> = hist
            .lines()
            .filter(|l| {
                l.split_whitespace()
                    .next()
                    .is_some_and(|w| w.chars().all(|c| c.is_ascii_digit()))
            })
            .collect();
        let before = entries
            .len()
            .checked_sub(2)
            .and_then(|i| entries.get(i))
            .ok_or_else(|| format!("too few history entries:\n{hist}"))?;
        if !before.contains("Normal") {
            return Err(format!("the powered-down IPL is not Normal: {before}"));
        }
        let sts = s.command("sysval QIPLSTS", CMD)?;
        if !sts.contains("QIPLSTS: 2") {
            return Err(format!("QIPLSTS after the restart: {sts}"));
        }
        Ok(format!(
            "D1001000 to D1001090 from Command Entry; IPL again in {:.1}s, previous IPL Normal, QIPLSTS 2",
            boot.as_secs_f64()
        ))
    }
}

/// A program that touches 60 MB of heap, a page at a time.
const MEMHOG: &str = "\
module MEMHOG
proc MEMHOG() entry {
L0:
  %k = const.i32 0
  br OUTER
OUTER:
  %kd = cmp.ge %k, 4
  brif %kd, DONE, BLOCK
BLOCK:
  %p = alloc 15000000
  %i = const.i32 0
  br TOUCH
TOUCH:
  %d = cmp.ge %i, 15000000
  brif %d, NEXT, BODY
BODY:
  %q = ptradd %p, %i
  store [%q]:i32, 1
  %i = add %i, 4096
  br TOUCH
NEXT:
  %k = add %k, 1
  br OUTER
DONE:
  dsply \"touched\"
  ret
}
";

/// A program that asks `QWCRSSTS` for `SSTS0200` and shows the jobs in
/// the system (offset 36) and the receiver's length.
const WMSSTS: &str = "\
module WMSSTS
extern sysapi QWCRSSTS($r: char(200), $rl: i32, $fmt: char(8), $rst: char(10), $err: char(16))
static $r: char(200)
static $rl: i32 inz(200)
static $fmt: char(8) inz(\"SSTS0200\")
static $rst: char(10) inz(\"*NO\")
static $err: char(16)
proc WMSSTS() entry {
L0:
  sysapi QWCRSSTS($r, $rl, $fmt, $rst, $err)
  %a = addr $r
  %n = load [%a +36]:i32
  dsply %n
  %l = load [%a +4]:i32
  dsply %l
  ret
}
";

impl Suite {
    /// Step 31: statistics: `WRKSYSSTS` shows `*BASE` taking page faults
    /// while a batch job touches 60 MB; `WRKACTJOB`'s views,
    /// `WRKDSKSTS`, `WRKSYSACT`, `DSPJOB`'s run figures and `QWCRSSTS`.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn statistics(&self, s: &mut Session) -> Result<String, String> {
        crate::qir::qemu::push(s, "MEMHOG", MEMHOG)?;
        s.command("qir asm MEMHOG MODULE(QGPL/MEMHOG)", CMD)?;
        s.command("crtpgm QGPL/MEMHOG MODULE(QGPL/MEMHOG)", CMD)?;
        s.command("wrksyssts RESET(*YES)", CMD)?;
        s.command("strsbs QSYS/QBATCH", CMD)?;
        s.command(
            "sbmjob CMD('call QGPL/MEMHOG') JOB(WMMEM) JOBQ(QGPL/QBATCH) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let mut msgs = String::new();
        for _ in 0..80 {
            msgs = s.command("dspmsg QUSRSYS/QSECOFR", CMD)?;
            if msgs.contains("WMMEM") && (msgs.contains("CPF1241") || msgs.contains("CPF1240")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let sts = s.command("wrksyssts", CMD)?;
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        let left = poll(s, "wrkactjob", 60, |o| active_row(o, "QBATCH").is_none())?;
        if let Some(r) = active_row(&left, "QBATCH") {
            let v2 = s.command("wrkactjob VIEW(3)", CMD)?;
            let sbs = s.command("wrksbs", CMD)?;
            return Err(format!("QBATCH did not end: {r}\n{v2}\n{sbs}"));
        }
        let base = sts
            .lines()
            .find(|l| l.split_whitespace().next() == Some("2"))
            .ok_or_else(|| format!("no *BASE in wrksyssts:\n{sts}"))?
            .to_string();
        let cols: Vec<f64> = base
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        let faults = cols.get(4).copied().unwrap_or(0.0) + cols.get(6).copied().unwrap_or(0.0);
        if faults <= 0.0 {
            return Err(format!("no faults in *BASE after WMMEM ran:\n{msgs}\n{sts}"));
        }
        let elapsed = s.command("wrkactjob VIEW(2)", CMD)?;
        let threads = s.command("wrkactjob VIEW(3)", CMD)?;
        let disks = s.command("wrkdsksts", CMD)?;
        let top = s.command("wrksysact", CMD)?;
        let job = s.command("dspjob", CMD)?;
        if !elapsed.contains("AuxIO")
            || !threads.contains("Threads")
            || !disks.lines().any(|l| l.split_whitespace().next() == Some("1"))
            || !top.contains("CPU Util")
            || !job.contains("Processing unit time used")
        {
            return Err(format!("the displays:\n{elapsed}\n{threads}\n{disks}\n{top}\n{job}"));
        }
        crate::qir::qemu::push(s, "WMSSTS", WMSSTS)?;
        s.command("qir asm WMSSTS MODULE(QGPL/WMSSTS)", CMD)?;
        s.command("crtpgm QGPL/WMSSTS MODULE(QGPL/WMSSTS)", CMD)?;
        let api = s.command("call QGPL/WMSSTS", CMD)?;
        let nums: Vec<i64> = api
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect();
        if nums.len() < 2 || nums[0] < 10 || nums[1] < 100 {
            return Err(format!("QWCRSSTS SSTS0200:\n{api}"));
        }
        Ok(format!(
            "*BASE took {faults:.1} faults/s while WMMEM touched 60 MB; QWCRSSTS saw {} jobs",
            nums[0]
        ))
    }
}

/// A report of 100 lines on `QSYSPRT`, slowed so it is seen running,
/// with a completion message in the job log.
const RPT100: &str = "\
module RPT100
file REPORT file(QSYSPRT) dev(*PRT)
static $line: char(20) inz(\"REPORT LINE\")
proc RPT100() entry {
L0:
  %o = dbopen REPORT
  %i = const.i32 1
  br TEST
TEST:
  %done = cmp.gt %i, 100
  brif %done, OUT, BODY
BODY:
  %j = const.i32 0
  br SPIN
SPIN:
  %s = cmp.ge %j, 20000000
  brif %s, PRINT, STEP
STEP:
  %j = add %j, 1
  br SPIN
PRINT:
  %w = prtio REPORT WRITE fmt(QSYSPRT) rec($line)
  %i = add %i, 1
  br TEST
OUT:
  %c = dbclose REPORT
  sndpgmmsg msgid(\"CPF9897\") msgf(\"QCPFMSG\") data(\"100 report lines printed\") type(*COMP) to(*EXT)
  ret
}
";

/// Reads the record of `QGPL/WMLCK` for update, keeping its lock, and
/// spins until it is ended.
const LOCKER: &str = "\
module LOCKER
file F file(WMLCK) lib(QGPL) usage(*UPDATE)
static $rec: char(10)
proc LOCKER() entry {
L0:
  %o = dbopen F
  %r = dbio F READ rec($rec)
  br SPIN
SPIN:
  br SPIN
}
";

/// Reads the same record for update: 0 when no one holds its lock.
const CHECKER: &str = "\
module CHECKER
file F file(WMLCK) lib(QGPL) usage(*UPDATE)
static $rec: char(10)
proc CHECKER() entry {
L0:
  %o = dbopen F
  %r = dbio F READ rec($rec)
  dsply %r
  %c = dbclose F
  ret
}
";

/// Assemble and bind a program into `QGPL`.
fn build(s: &mut Session, name: &str, text: &str) -> Result<(), String> {
    crate::qir::qemu::push(s, name, text)?;
    let m = s.command(&format!("qir asm {name} MODULE(QGPL/{name})"), CMD)?;
    let p = s.command(&format!("crtpgm QGPL/{name} MODULE(QGPL/{name})"), CMD)?;
    if !m.contains("created") || !p.contains("created") {
        return Err(format!("{name}:\n{m}\n{p}"));
    }
    Ok(())
}

/// Poll `cmd` until its output satisfies `done`; the last output.
fn poll(
    s: &mut Session,
    cmd: &str,
    tries: u32,
    done: impl Fn(&str) -> bool,
) -> Result<String, String> {
    let mut out = String::new();
    for _ in 0..tries {
        out = s.command(cmd, CMD)?;
        if done(&out) {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(out)
}

/// The active job named `name` in `WRKACTJOB`'s status view.
fn active_row(list: &str, name: &str) -> Option<String> {
    list.lines()
        .find(|l| l.split_whitespace().next() == Some(name))
        .map(str::to_string)
}

impl Suite {
    /// Step 32: a submitted report: waits on the held queue, runs, prints
    /// 100 lines and logs its completion; `QSYSPRT` and `QPJOBLOG` are
    /// ready, displayed, and the capture writer prints the report to text
    /// and a two-page PDF.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn report_job(&self, s: &mut Session) -> Result<String, String> {
        build(s, "RPT100", RPT100)?;
        s.command("strsbs QSYS/QBATCH", CMD)?;
        s.command("hldjobq QGPL/QBATCH", CMD)?;
        let out = s.command(
            "sbmjob CMD('call QGPL/RPT100') JOB(WMRPT) JOBQ(QGPL/QBATCH) LOG(4 00 *SECLVL) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let job = submitted(&out)?;
        let queued = s.command("wrkjobq QGPL/QBATCH", CMD)?;
        if !queued.contains("WMRPT") {
            return Err(format!("WMRPT is not on the queue:\n{queued}"));
        }
        s.command("rlsjobq QGPL/QBATCH", CMD)?;
        let running = poll(s, "wrkactjob", 80, |o| {
            active_row(o, "WMRPT").is_some_and(|r| r.contains("RUN"))
        })?;
        let row = active_row(&running, "WMRPT")
            .ok_or_else(|| format!("WMRPT was never seen running:\n{running}"))?;
        let msgs = poll(s, "dspmsg QUSRSYS/QSECOFR", 120, |o| {
            o.contains("CPF1241") && o.contains("WMRPT")
        })?;
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        if !msgs.contains("WMRPT") {
            return Err(format!("WMRPT did not complete:\n{msgs}"));
        }
        let files = s.command(&format!("wrksplf SELECT(*ALL) JOB({job})"), CMD)?;
        for f in ["QSYSPRT", "QPJOBLOG"] {
            if !files.lines().any(|l| l.contains(f) && l.contains("RDY")) {
                return Err(format!("{f} is not ready:\n{files}"));
            }
        }
        let report = s.command(&format!("dspsplf QSYSPRT JOB({job})"), CMD)?;
        if report.matches("REPORT LINE").count() != 100 || !report.contains("   2/") {
            return Err(format!("the report:\n{report}"));
        }
        let log = s.command(&format!("dspsplf QPJOBLOG JOB({job})"), CMD)?;
        if !log.contains("CPF9897") || !log.contains("100 report lines printed") {
            return Err(format!("the completion message is not logged:\n{log}"));
        }
        s.command(
            &format!("chgsplfa QSYSPRT JOB({job}) OUTQ(QUSRSYS/QSCAPTURE)"),
            CMD,
        )?;
        let caps = poll(s, "prtcapture list", 80, |o| o.contains(&job))?;
        if !caps.contains(&job) {
            return Err(format!("the writer did not print WMRPT:\n{caps}"));
        }
        let text = s.command(&format!("prtcapture get QSYSPRT JOB({job})"), CMD)?;
        let pdf_out = s.command(
            &format!("prtcapture get QSYSPRT JOB({job}) FORMAT(*PDF)"),
            CMD,
        )?;
        let b64: String = pdf_out
            .lines()
            .skip_while(|l| !l.starts_with("-----BEGIN PDF"))
            .skip(1)
            .take_while(|l| !l.starts_with("-----END PDF"))
            .collect();
        let pdf = unbase64(&b64).ok_or("the PDF is not base64")?;
        let pages = spool::pdf::check(&pdf).map_err(|e| format!("the PDF: {e}"))?;
        if text.matches("REPORT LINE").count() != 100 || pages != 2 {
            return Err(format!("the capture: {pages} PDF page(s)\n{text}"));
        }
        let wrkjob = s.command(&format!("wrkjob JOB({job}) OPTION(*SPLF)"), CMD)?;
        if !wrkjob.contains("QPJOBLOG") {
            return Err(format!("wrkjob *SPLF:\n{wrkjob}"));
        }
        Ok(format!(
            "{job} queued, ran ({}), printed 100 lines on 2 pages, logged CPF9897; QSCAPTURE made text and a {}-byte PDF",
            row.split_whitespace().last().unwrap_or(""),
            pdf.len()
        ))
    }

    /// Step 33: a program failing unmonitored in a batch job: the job
    /// waits in `MSGW` on `CEE9901` at `QSYSOPR`, the reply `C` ends it,
    /// and its job log holds the escape and the inquiry.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn batch_inquiry(&self, s: &mut Session) -> Result<String, String> {
        s.command("strsbs QSYS/QBATCH", CMD)?;
        let out = s.command(
            "sbmjob CMD('call QGPL/FAILPGM') JOB(WMFAIL) JOBQ(QGPL/QBATCH) INQMSGRPY(*RQD) LOG(4 00 *SECLVL) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let job = submitted(&out)?;
        let waiting = poll(s, "wrkactjob", 80, |o| {
            active_row(o, "WMFAIL").is_some_and(|r| r.contains("MSGW"))
        })?;
        if !active_row(&waiting, "WMFAIL").is_some_and(|r| r.contains("MSGW")) {
            s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
            return Err(format!("WMFAIL is not waiting on a message:\n{waiting}"));
        }
        let opr = s.command("dspmsg QSYSOPR", CMD)?;
        let key = opr
            .lines()
            .rfind(|l| l.contains("CEE9901"))
            .and_then(|l| l.split_whitespace().next())
            .ok_or_else(|| format!("no CEE9901 on QSYSOPR:\n{opr}"))?
            .to_string();
        s.command(
            &format!("sndrpymsg MSGKEY({key}) MSGQ(QSYS/QSYSOPR) RPY('C')"),
            CMD,
        )?;
        let msgs = poll(s, "dspmsg QUSRSYS/QSECOFR", 80, |o| {
            o.contains("CPF1240") && o.contains("WMFAIL")
        })?;
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        if !msgs.contains("WMFAIL") {
            return Err(format!("WMFAIL did not end abnormally:\n{msgs}"));
        }
        let log = poll(s, &format!("dspsplf QPJOBLOG JOB({job})"), 40, |o| {
            o.contains("CEE9901")
        })?;
        if !log.contains("CEE9901") || !log.contains("Escape") {
            return Err(format!("the job log lacks the escape or the inquiry:\n{log}"));
        }
        Ok(format!(
            "{job} waited in MSGW on CEE9901 ({key}); reply C ended it; the job log has the escape and the inquiry"
        ))
    }

    /// Step 34: `ENDSBS QBATCH *IMMED` while five jobs run, one holding a
    /// record lock and four waiting for it: all end, the lock is free,
    /// the store checks clean.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn end_busy_subsystem(&self, s: &mut Session) -> Result<String, String> {
        s.command("crtpf QGPL/WMLCK RCDLEN(10)", CMD)?;
        crate::qir::qemu::push(s, "WMLDATA", "LOCKME")?;
        s.command("cpyfrmimpf FROMSTMF(WMLDATA) TOFILE(QGPL/WMLCK)", CMD)?;
        build(s, "LOCKER", LOCKER)?;
        build(s, "CHECKER", CHECKER)?;
        s.command("chgjobqe QSYS/QBATCH JOBQ(QGPL/QBATCH) MAXACT(*NOMAX)", CMD)?;
        s.command("strsbs QSYS/QBATCH", CMD)?;
        for i in 1..=5 {
            s.command(
                &format!("sbmjob CMD('call QGPL/LOCKER') JOB(WMLCK{i}) JOBQ(QGPL/QBATCH) MSGQ(*NONE)"),
                CMD,
            )?;
        }
        let busy = poll(s, "wrkactjob", 80, |o| {
            (1..=5).all(|i| active_row(o, &format!("WMLCK{i}")).is_some())
        })?;
        if !(1..=5).all(|i| active_row(&busy, &format!("WMLCK{i}")).is_some()) {
            return Err(format!("the five jobs are not all active:\n{busy}"));
        }
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD)?;
        let after = poll(s, "wrkactjob", 80, |o| !o.contains("WMLCK") && active_row(o, "QBATCH").is_none())?;
        s.command("chgjobqe QSYS/QBATCH JOBQ(QGPL/QBATCH) MAXACT(1)", CMD)?;
        if after.contains("WMLCK") {
            return Err(format!("jobs survived ENDSBS *IMMED:\n{after}"));
        }
        let check = s.command("call QGPL/CHECKER", Duration::from_secs(90))?;
        let chk = s.command("chkstg", Duration::from_secs(300))?;
        let rcl = s.command("rclstg *CHECK", Duration::from_secs(300))?;
        s.command("dltf QGPL/WMLCK", CMD)?;
        if !check.lines().any(|l| l.trim() == "0") {
            return Err(format!("the record is still locked:\n{check}"));
        }
        if !chk.contains(": 0 bad") || !rcl.contains("0 problem(s)") {
            return Err(format!("the store after ENDSBS:\n{chk}\n{rcl}"));
        }
        Ok("five jobs ended by ENDSBS *IMMED; the record lock was free; chkstg and rclstg *CHECK clean".into())
    }

    /// Step 35: `dbgkill` kills the service console job in the middle of
    /// a commitment-control cycle: the storage service rolls it back,
    /// the job is gone from `WRKACTJOB`, its job log is spooled with the
    /// abnormal end, and the store checks clean.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn killed_job(&self, s: &mut Session) -> Result<String, String> {
        let numbers = s.command("wrkactjob VIEW(3)", CMD)?;
        let old = numbers
            .lines()
            .find(|l| l.trim_start().starts_with("QSVCJOB"))
            .and_then(|l| l.split_whitespace().nth(2))
            .ok_or_else(|| format!("no QSVCJOB:\n{numbers}"))?
            .to_string();
        let lib = crate::db::kit::LIBRARY;
        let sum = |s: &mut Session| -> Result<String, String> {
            let out = s.command(&format!("sql SELECT SUM(BAL) FROM {lib}/CUSTOMER;"), CMD)?;
            let lines: Vec<&str> = out.lines().map(str::trim).collect();
            lines
                .iter()
                .position(|l| l.ends_with("row(s)"))
                .and_then(|i| lines[..i].iter().rev().find(|l| l.parse::<f64>().is_ok()))
                .map(|l| (*l).to_string())
                .ok_or_else(|| format!("no balance in:\n{out}"))
        };
        let before = sum(s)?;
        s.command("strcmtctl LCKLVL(*CHG)", CMD)?;
        s.command(&format!("sql UPDATE {lib}/CUSTOMER SET BAL = BAL + 1000;"), CMD)?;
        let during = sum(s)?;
        if during == before {
            return Err(format!("the update did not show: {before}"));
        }
        let killed = s.command("dbgkill JOB(QSVCJOB)", CMD)?;
        if !killed.contains("killed") {
            return Err(format!("dbgkill: {killed}"));
        }
        // slic starts the job again and shows its prompt.
        s.expect("starting it again", Duration::from_secs(10))?;
        s.expect(crate::PROMPT, Duration::from_secs(15))?;
        let now = s.command("wrkactjob VIEW(3)", CMD)?;
        if !now.contains("QSVCJOB") || now.contains(&old) {
            return Err(format!("QSVCJOB {old} is still listed, or none is:\n{now}"));
        }
        let after = sum(s)?;
        let log = s.command(&format!("dspsplf QPJOBLOG JOB({old}/QSYS/QSVCJOB)"), CMD)?;
        let chk = s.command("chkstg", Duration::from_secs(300))?;
        if after != before {
            return Err(format!("not rolled back: {before} before, {during} during, {after} after"));
        }
        if !log.contains("CPF1164") || !log.contains(" 70") {
            return Err(format!("the killed job's log:\n{log}"));
        }
        if !chk.contains(": 0 bad") {
            return Err(format!("chkstg:\n{chk}"));
        }
        Ok(format!(
            "QSVCJOB {old} killed mid-cycle; balance back to {after}; job log spooled with end code 70; chkstg clean"
        ))
    }
}

/// `YYYYMMDD` and `hhmmss` in UTC, `secs` seconds from now.
fn utc_in(secs: u64) -> (String, String) {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        + secs;
    let days = i64::try_from(unix / 86_400).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let secs = unix % 86_400;
    (
        format!("{year:04}{month:02}{day:02}"),
        format!("{:02}{:02}{:02}", secs / 3600, secs % 3600 / 60, secs % 60),
    )
}

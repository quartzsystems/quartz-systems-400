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
                .any(|l| l.starts_with("WMAUTO") && l.contains("DLYW"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let auto = active
            .lines()
            .find(|l| l.starts_with("WMAUTO"))
            .ok_or_else(|| format!("the autostart job is not active:\n{active}"))?;
        if !auto.contains("ASJ") || !auto.contains("DLYW") {
            return Err(format!("the autostart job is not waiting: {auto}"));
        }
        if !active
            .lines()
            .any(|l| l.starts_with("WMSBS") && l.contains("SBS"))
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
        // The history line, not the input line ("===> wrkactjob").
        let shown = screen_has(s, "  > wrkactjob")?;
        if !shown.contains("QSCONSOLE") {
            return Err(format!(
                "the job's own command shows no QSCONSOLE:\n{shown}"
            ));
        }
        let active = s.command("wrkactjob", CMD)?;
        let row = active
            .lines()
            .find(|l| l.starts_with("QSCONSOLE"))
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
                .any(|l| l.starts_with("WMDLY") && l.contains("DLYW"))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let row = active
            .lines()
            .find(|l| l.starts_with("WMDLY"))
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
}

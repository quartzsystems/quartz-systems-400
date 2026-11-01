// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `DSPJOB` shows of the sample jobs: the prototype's `ADMIN` job
//! in full, the others from their list entries.

use crate::env::work::{Frame, Job, JobPerf, JobProps, LibEntry, Lock, LogEntry, Thread};
use crate::util::{mdy, secs};

/// The properties of `job`.
pub fn props(job: &Job) -> JobProps {
    let started = if job.number == "123456" {
        secs(2026, 10, 29, 6, 0, 1)
    } else {
        secs(2026, 10, 27, 6, 0, 0) + u64::from(job.number.parse::<u32>().unwrap_or(0) % 600) * 60
    };
    let (desc, queue) = match job.subsystem.as_str() {
        "QHTTPSVR" => ("QHTTPSVR/QZHBHTTP", "QHTTPSVR/QZHBHTTP"),
        "QINTER" => ("QGPL/QDFTJOBD", "QGPL/QINTER"),
        _ => ("QGPL/QDFTJOBD", "QGPL/QBATCH"),
    };
    let submitted_by = if job.subsystem == "QHTTPSVR" {
        "123202/QSYS/QHTTPSVR"
    } else {
        ""
    };
    JobProps {
        job: job.clone(),
        entered: started,
        started,
        submitted_by: submitted_by.into(),
        submitted_how: if submitted_by.is_empty() {
            String::new()
        } else {
            "autostart job entry".into()
        },
        jobd: desc.into(),
        jobq: queue.into(),
        outq: "QUSRSYS/QEZJOBLOG".into(),
        run_priority: if job.kind == "INT" { 20 } else { 25 },
        time_slice: 2000,
        logging: "4 00 *SECLVL".into(),
        log_cl: true,
        inquiry: "*RQD".into(),
        ccsid: "37".into(),
        language: "ENU".into(),
        country: "US".into(),
        perf: if job.number == "123456" {
            JobPerf {
                cpu_ms: 12_447,
                elapsed: 8 * 3600 + 3 * 60 + 41,
                io_async: 41_002,
                io_sync: 3_318,
                faults: 9_117,
                threads: 41,
                temp_mb: 212,
                temp_peak_mb: 230,
                interval: 60,
                interval_cpu: 3,
                interval_io: 17,
                transactions: 0,
                response: None,
                pool: 3,
                pool_name: "*BASE".into(),
                activity: 100,
                pool_mb: 32_768,
            }
        } else {
            JobPerf {
                cpu_ms: u64::from(job.cpu) * 100 + 250,
                elapsed: crate::fixture::now().saturating_sub(started),
                threads: 1,
                temp_mb: 4,
                temp_peak_mb: 6,
                interval: 60,
                interval_cpu: job.cpu,
                response: (job.kind == "INT").then_some(2),
                pool: 2,
                pool_name: "*BASE".into(),
                activity: 100,
                pool_mb: 32_768,
                ..JobPerf::default()
            }
        },
    }
}

fn entry(
    id: &str,
    kind: &str,
    sev: u8,
    time: &str,
    from: (&str, &str, &str),
    to: (&str, &str, &str),
    text: &str,
) -> LogEntry {
    let t = |x: (&str, &str, &str)| (x.0.to_string(), x.1.to_string(), x.2.to_string());
    LogEntry {
        id: id.into(),
        kind: kind.into(),
        severity: sev,
        date: "10/29/26".into(),
        time: time.into(),
        from: t(from),
        to: t(to),
        text: text.into(),
    }
}

/// The job log of `job`.
#[allow(clippy::many_single_char_names)]
pub fn log(p: &JobProps) -> Vec<LogEntry> {
    let q = p.job.qualified();
    let sbs = &p.job.subsystem;
    if p.job.number != "123456" {
        let (d, t) = mdy(p.started, 0);
        let mut e = entry(
            "CPF1124",
            "Information",
            0,
            &format!("{t}.000000"),
            ("QWTPIIPP", "QSYS", "04C8"),
            ("*EXT", "", "*N"),
            &format!(
                "Job {q} started on {d} at {t} in subsystem {sbs} in QSYS. Job entered system on {d} at {t}."
            ),
        );
        e.date = d;
        return vec![e];
    }
    let wtp = ("QWTPIIPP", "QSYS", "04C8");
    let ext = ("*EXT", "", "*N");
    vec![
        entry(
            "CPF1124",
            "Information",
            0,
            "06:00:01.117332",
            wtp,
            ext,
            &format!(
                "Job {q} started on 10/29/26 at 06:00:01 in subsystem {sbs} in QSYS. Job entered system on 10/29/26 at 06:00:01."
            ),
        ),
        entry(
            "CPI1125",
            "Information",
            0,
            "06:00:01.118004",
            wtp,
            ext,
            &format!("Job {q} submitted."),
        ),
        entry(
            "HTP8000",
            "Information",
            0,
            "06:00:02.410993",
            ("QZHBMAIN", "QHTTPSVR", "0012"),
            ext,
            "HTTP server instance ADMIN starting with configuration /www/admin/conf/httpd.conf.",
        ),
        entry(
            "HTP8010",
            "Information",
            0,
            "06:00:02.902211",
            ("QZHBMAIN", "QHTTPSVR", "0031"),
            ext,
            "Certificate QSHQ400 self-signed loaded for application QS400_HTTP_SERVER_ADMIN.",
        ),
        entry(
            "HTP8001",
            "Information",
            0,
            "06:00:03.004210",
            ("QZHBMAIN", "QHTTPSVR", "0044"),
            ext,
            "HTTP server instance ADMIN started, listening on *:2001.",
        ),
        entry(
            "HTP8110",
            "Diagnostic",
            20,
            "11:42:17.330018",
            ("QZSRHTTP", "QHTTPSVR", "02A1"),
            ("QZHBMAIN", "QHTTPSVR", "0044"),
            "Request from 172.16.0.9 for /qs400/httpadmin/instance/WEBSTORE/general failed, user WEBUSER not authorized.",
        ),
    ]
}

/// The library list of `p`'s job.
pub fn libl(p: &JobProps) -> Vec<LibEntry> {
    let l = |seq: Option<u32>, lib: &str, kind: &str, text: &str| LibEntry {
        seq,
        lib: lib.into(),
        kind: kind.into(),
        text: text.into(),
    };
    let mut v = vec![
        l(Some(10), "QSYS", "SYS", "System library"),
        l(Some(20), "QSYS2", "SYS", "System library for database"),
        l(Some(30), "QHLPSYS", "SYS", "Help library"),
        l(Some(40), "QUSRSYS", "SYS", "User system library"),
        l(None, "QGPL", "CUR", "General purpose library"),
    ];
    if p.job.subsystem == "QHTTPSVR" {
        v.push(l(Some(10), "QHTTPSVR", "USR", "HTTP server"));
        v.push(l(Some(20), "QTEMP", "USR", ""));
        v.push(l(Some(30), "QGPL", "USR", "General purpose library"));
    } else {
        v.push(l(Some(10), "QTEMP", "USR", ""));
        v.push(l(Some(20), "QGPL", "USR", "General purpose library"));
    }
    v
}

/// The locks of `p`'s job.
pub fn locks(p: &JobProps) -> Vec<Lock> {
    let l = |object: &str, lib: &str, typ: &str, lock: &str, scope: &str, thread: &str| Lock {
        object: object.into(),
        lib: lib.into(),
        typ: typ.into(),
        lock: lock.into(),
        status: "HELD".into(),
        scope: scope.into(),
        thread: thread.into(),
    };
    let (jl, jn) = p.jobd.split_once('/').unwrap_or(("QGPL", "QDFTJOBD"));
    let mut v = vec![l(jn, jl, "*JOBD", "*SHRRD", "*JOB", "")];
    if p.job.number == "123456" {
        v.push(l("ADMIN", "QHTTPSVR", "*USRSPC", "*EXCL", "*JOB", ""));
        v.push(l("QZHBMSG", "QHTTPSVR", "*MSGF", "*SHRRD", "*JOB", ""));
    }
    v.push(l(&p.job.user, "QSYS", "*USRPRF", "*SHRRD", "*JOB", ""));
    if p.job.number == "123456" {
        v.push(l(
            "/www/admin/logs/access_log",
            "",
            "*STMF",
            "*SHRUPD",
            "*THREAD",
            "00000001",
        ));
    }
    v
}

fn frame(
    program: &str,
    lib: &str,
    module: &str,
    procedure: &str,
    statement: &str,
    actgrp: &str,
) -> Frame {
    Frame {
        program: program.into(),
        lib: lib.into(),
        module: module.into(),
        procedure: procedure.into(),
        statement: statement.into(),
        actgrp: actgrp.into(),
    }
}

/// The call stack of `p`'s job.
pub fn stack(p: &JobProps) -> Vec<Thread> {
    let qcmd = frame("QCMD", "QSYS", "", "", "", "*DFTACTGRP");
    if p.job.number != "123456" {
        return vec![Thread {
            id: "00000001".into(),
            initial: true,
            frames: vec![qcmd],
        }];
    }
    let worker = |id: &str| Thread {
        id: id.into(),
        initial: false,
        frames: vec![
            frame(
                "QZHBMAIN", "QHTTPSVR", "QZHBSRV", "worker", "0220", "QZHBHTTP",
            ),
            frame(
                "QP0ZCPA",
                "QSYS",
                "QP0ZCPA",
                "pthread_cond_wait",
                "",
                "*DFTACTGRP",
            ),
        ],
    };
    vec![
        Thread {
            id: "00000001".into(),
            initial: true,
            frames: vec![
                qcmd,
                frame(
                    "QZHBMAIN", "QHTTPSVR", "QZHBMAIN", "main", "0412", "QZHBHTTP",
                ),
                frame(
                    "QZHBMAIN",
                    "QHTTPSVR",
                    "QZHBSRV",
                    "serve_loop",
                    "1203",
                    "QZHBHTTP",
                ),
                frame(
                    "QZHBMAIN",
                    "QHTTPSVR",
                    "QZHBSRV",
                    "accept_wait",
                    "0877",
                    "QZHBHTTP",
                ),
                frame(
                    "QP0ZCPA",
                    "QSYS",
                    "QP0ZCPA",
                    "pthread_cond_wait",
                    "",
                    "*DFTACTGRP",
                ),
            ],
        },
        worker("00000012"),
        worker("00000013"),
        worker("00000014"),
    ]
}

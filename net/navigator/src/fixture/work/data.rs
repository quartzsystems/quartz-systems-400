// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's work: 318 active jobs, its job queues,
//! subsystems, output queues with 212 spooled files, writers and job
//! schedule entries.

use crate::env::work::{
    Autostart, Job, JobQueue, OutQueue, Prestart, Routing, SbsPool, ScheduleEntry, Splf, Subsystem,
    SubsystemDesc, Workstation, Writer,
};
use crate::util::secs;

/// Number, user, name, type, subsystem, status, CPU tenths, function,
/// current user.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    u32,
    &'static str,
    &'static str,
);

/// The jobs the prototype's first page shows, in the system's order.
const FIRST: &[Row] = &[
    (
        "123456",
        "QTMHHTTP",
        "ADMIN",
        "BCH",
        "QHTTPSVR",
        "RUN",
        3,
        "PGM-QZHBMAIN",
        "QTMHHTTP",
    ),
    (
        "123457",
        "QTMHHTTP",
        "ADMIN",
        "BCI",
        "QHTTPSVR",
        "TIMW",
        0,
        "PGM-QZSRHTTP",
        "QTMHHTTP",
    ),
    (
        "123461",
        "QTMHHTTP",
        "APACHEDFT",
        "BCH",
        "QHTTPSVR",
        "DEQW",
        0,
        "PGM-QZHBMAIN",
        "QTMHHTTP",
    ),
    (
        "123460",
        "QSECOFR",
        "QPADEV0001",
        "INT",
        "QINTER",
        "MSGW",
        0,
        "CMD-WRKACTJOB",
        "QSECOFR",
    ),
    (
        "123488",
        "CWELLMAN",
        "QPADEV0003",
        "INT",
        "QINTER",
        "RUN",
        12,
        "PGM-HLINV/INVMAIN",
        "CWELLMAN",
    ),
    (
        "123490",
        "OPERATOR",
        "QPADEV0004",
        "INT",
        "QINTER",
        "DSPW",
        0,
        "MNU-MAIN",
        "OPERATOR",
    ),
    (
        "123501",
        "QPGMR",
        "NIGHTLY",
        "BCH",
        "QBATCH",
        "RUN",
        148,
        "PGM-QGPL/NIGHTLY",
        "QPGMR",
    ),
    (
        "123502",
        "QPGMR",
        "RGZPFM01",
        "BCH",
        "QBATCH",
        "RUN",
        61,
        "CMD-RGZPFM",
        "QPGMR",
    ),
    (
        "123503",
        "CWELLMAN",
        "INVBATCH",
        "BCH",
        "QBATCH",
        "MSGW",
        0,
        "PGM-HLINV/INVLOAD",
        "CWELLMAN",
    ),
    (
        "123411",
        "QUSER",
        "QZDASOINIT",
        "PJ",
        "QUSRWRK",
        "RUN",
        22,
        "PGM-QZDASOINIT",
        "WEBUSER",
    ),
    (
        "123412",
        "QUSER",
        "QZDASOINIT",
        "PJ",
        "QUSRWRK",
        "PSRW",
        0,
        "",
        "QUSER",
    ),
    (
        "123413",
        "QUSER",
        "QZDASOINIT",
        "PJ",
        "QUSRWRK",
        "PSRW",
        0,
        "",
        "QUSER",
    ),
    (
        "123420", "QUSER", "QZRCSRVS", "PJ", "QUSRWRK", "PSRW", 0, "", "QUSER",
    ),
    (
        "123421", "QUSER", "QZSOSIGN", "PJ", "QUSRWRK", "PSRW", 0, "", "QUSER",
    ),
    (
        "123380",
        "QTCP",
        "QTVDEVICE",
        "BCH",
        "QSYSWRK",
        "DEQW",
        0,
        "PGM-QTVDEVIC",
        "QTCP",
    ),
    (
        "123381",
        "QTCP",
        "QTVTELNET",
        "BCH",
        "QSYSWRK",
        "SELW",
        1,
        "PGM-QTVTELNT",
        "QTCP",
    ),
    (
        "123382",
        "QTCP",
        "QTFTP00412",
        "BCH",
        "QSYSWRK",
        "SELW",
        0,
        "PGM-QTMFTPS",
        "QTCP",
    ),
    (
        "123383",
        "QSSHD",
        "QTSSHD",
        "BCH",
        "QSYSWRK",
        "SELW",
        0,
        "PGM-QP0ZSPWP",
        "QSSHD",
    ),
    (
        "123385",
        "QTCP",
        "QNTPSRVR",
        "BCH",
        "QSYSWRK",
        "TIMW",
        0,
        "PGM-QTOTNTPS",
        "QTCP",
    ),
    (
        "123301", "QSYS", "QDBSRV01", "SYS", "QSYSWRK", "DEQW", 0, "", "QSYS",
    ),
    (
        "123302",
        "QPGMR",
        "QSYSSCD",
        "BCH",
        "QCTL",
        "EVTW",
        0,
        "PGM-QEZSCNEP",
        "QPGMR",
    ),
    (
        "123200", "QSYS", "QINTER", "SBS", "QINTER", "DEQW", 0, "", "QSYS",
    ),
    (
        "123201", "QSYS", "QBATCH", "SBS", "QBATCH", "DEQW", 0, "", "QSYS",
    ),
    (
        "123202", "QSYS", "QHTTPSVR", "SBS", "QHTTPSVR", "DEQW", 0, "", "QSYS",
    ),
    (
        "123105",
        "QSYS",
        "QSPLMAINT",
        "SYS",
        "QSYSWRK",
        "DEQW",
        0,
        "",
        "QSYS",
    ),
];

fn job(n: u32, user: &str, name: &str, kind: &str, sbs: &str, status: &str, function: &str) -> Job {
    Job {
        number: n.to_string(),
        user: user.into(),
        name: name.into(),
        kind: kind.into(),
        subsystem: sbs.into(),
        status: status.into(),
        cpu: 0,
        function: function.into(),
        current_user: user.into(),
    }
}

/// The 318 active jobs.
#[allow(clippy::too_many_lines)]
pub fn jobs() -> Vec<Job> {
    let mut out: Vec<Job> = FIRST
        .iter()
        .map(|&(n, u, name, k, s, st, cpu, f, cu)| Job {
            number: n.into(),
            user: u.into(),
            name: name.into(),
            kind: k.into(),
            subsystem: s.into(),
            status: st.into(),
            cpu,
            function: f.into(),
            current_user: cu.into(),
        })
        .collect();
    let mut n = 122_000;
    let mut next = || {
        n += 1;
        n
    };
    out.push(job(next(), "QSYS", "QCTL", "SBS", "QCTL", "DEQW", ""));
    let users = ["CWELLMAN", "OPERATOR", "QPGMR", "WEBUSER"];
    for i in 0..11 {
        let u = users[i % users.len()];
        out.push(job(
            next(),
            u,
            &format!("QPADEV{:04}", 5 + i),
            "INT",
            "QINTER",
            "DSPW",
            "MNU-MAIN",
        ));
    }
    for (name, st) in [
        ("RPTDAILY", "TIMW"),
        ("INVEXPORT", "DEQW"),
        ("CLNUPSPL", "RUN"),
    ] {
        out.push(job(
            next(),
            "QPGMR",
            name,
            "BCH",
            "QBATCH",
            st,
            &format!("PGM-QGPL/{name}"),
        ));
    }
    for (name, kind) in [
        ("NAVIGATOR", "BCH"),
        ("NAVIGATOR", "BCI"),
        ("NAVIGATOR", "BCI"),
        ("NAVIGATOR", "BCI"),
        ("ACCESSWEB", "BCH"),
        ("ACCESSWEB", "BCI"),
        ("ACCESSWEB", "BCI"),
        ("ACCESSWEB", "BCI"),
        ("APACHEDFT", "BCI"),
        ("APACHEDFT", "BCI"),
    ] {
        let (st, f) = if kind == "BCH" {
            ("DEQW", "PGM-QZHBMAIN")
        } else {
            ("TIMW", "PGM-QZSRHTTP")
        };
        out.push(job(next(), "QTMHHTTP", name, kind, "QHTTPSVR", st, f));
    }
    let sys = [
        ("QSYSWRK", "QSYS", "SBS", ""),
        ("QTCPIP", "QTCP", "BCH", "PGM-QTOSTCPIP"),
        ("QTCPMONITR", "QTCP", "BCH", "PGM-QTOSMONITR"),
        ("QTLPD00001", "QTCP", "BCH", "PGM-QTMPLPD"),
        ("QTSMTPSRVR", "QTCP", "BCH", "PGM-QTMSSRCP"),
        ("QZSCSRVSD", "QUSER", "BCH", "PGM-QZSCSRVSD"),
        ("QZDASRVSD", "QUSER", "BCH", "PGM-QZDASRVSD"),
        ("QZRCSRVSD", "QUSER", "BCH", "PGM-QZRCSRVSD"),
        ("QZSOSGND", "QUSER", "BCH", "PGM-QZSOSGND"),
        ("QSYSCOMM1", "QSYS", "SYS", ""),
        ("QJOBSCD", "QSYS", "SYS", ""),
        ("QDBSRV02", "QSYS", "SYS", ""),
        ("QDBSRV03", "QSYS", "SYS", ""),
    ];
    for (name, u, k, f) in sys {
        out.push(job(next(), u, name, k, "QSYSWRK", "DEQW", f));
    }
    for _ in 0..131 - sys.len() {
        out.push(job(next(), "QUSER", "QSQSRVR", "PJ", "QSYSWRK", "PSRW", ""));
    }
    out.push(job(next(), "QSYS", "QUSRWRK", "SBS", "QUSRWRK", "DEQW", ""));
    for i in 0..(96 - 5 - 1) {
        let name = match i % 3 {
            0 => "QZRCSRVS",
            1 => "QZSOSIGN",
            _ => "QZDASOINIT",
        };
        out.push(job(next(), "QUSER", name, "PJ", "QUSRWRK", "PSRW", ""));
    }
    out.push(job(next(), "QSYS", "QSERVER", "SBS", "QSERVER", "DEQW", ""));
    out.push(job(
        next(),
        "QPGMR",
        "QPWFSERVSD",
        "BCH",
        "QSERVER",
        "SELW",
        "PGM-QPWFSERVSD",
    ));
    for _ in 0..10 {
        out.push(job(
            next(),
            "QUSER",
            "QPWFSERVSO",
            "PJ",
            "QSERVER",
            "PSRW",
            "",
        ));
    }
    out.push(job(next(), "QSYS", "QSPL", "SBS", "QSPL", "DEQW", ""));
    for (name, st) in [
        ("PRT01", "RUN"),
        ("QPDFWTR", "DEQW"),
        ("RMTLASER", "DEQW"),
        ("QSPLSRV", "DEQW"),
        ("QLPDOUT", "DEQW"),
    ] {
        out.push(job(next(), "QSPLJOB", name, "WTR", "QSPL", st, ""));
    }
    out.push(job(next(), "QSYS", "QCMN", "SBS", "QCMN", "DEQW", ""));
    for _ in 0..27 {
        out.push(job(next(), "QUSER", "QNPSERVS", "PJ", "QCMN", "PSRW", ""));
    }
    // The numbers System activity shows for these.
    for j in &mut out {
        match (j.name.as_str(), j.kind.as_str()) {
            ("NAVIGATOR", "BCH") => j.number = "123462".into(),
            ("QDBSRV03", _) => j.number = "123303".into(),
            _ => {}
        }
    }
    out
}

/// The job queues.
pub fn job_queues() -> Vec<JobQueue> {
    let q = |name: &str,
             lib: &str,
             held: bool,
             sbs: &str,
             w: u32,
             h: u32,
             s: u32,
             max: Option<u32>,
             text: &str| JobQueue {
        name: name.into(),
        lib: lib.into(),
        held,
        subsystem: sbs.into(),
        waiting: w,
        held_jobs: h,
        scheduled: s,
        max_active: max,
        text: text.into(),
    };
    vec![
        q(
            "QBATCH",
            "QGPL",
            false,
            "QBATCH",
            31,
            2,
            4,
            Some(3),
            "Batch job queue",
        ),
        q("QS36EVOKE", "QGPL", false, "QBATCH", 0, 0, 0, None, ""),
        q(
            "QINTER",
            "QGPL",
            false,
            "QINTER",
            0,
            0,
            0,
            None,
            "Interactive job queue",
        ),
        q(
            "QPGMR",
            "QGPL",
            true,
            "QBATCH",
            4,
            4,
            0,
            Some(1),
            "Programmer job queue",
        ),
        q("QTXTSRCH", "QGPL", false, "QBATCH", 0, 0, 0, None, ""),
        q(
            "QZHBHTTP",
            "QHTTPSVR",
            false,
            "QHTTPSVR",
            0,
            0,
            0,
            None,
            "HTTP server job queue",
        ),
        q(
            "INVLOAD",
            "HLINV",
            false,
            "QBATCH",
            2,
            0,
            0,
            Some(1),
            "Inventory load jobs",
        ),
    ]
}

/// Every subsystem description.
#[allow(clippy::too_many_lines)]
pub fn subsystems() -> Vec<Subsystem> {
    let s = |name: &str,
             lib: &str,
             active: bool,
             jobs: u32,
             pools: &[&str],
             max: Option<u32>,
             text: &str| Subsystem {
        name: name.into(),
        lib: lib.into(),
        active,
        jobs,
        pools: pools.iter().map(ToString::to_string).collect(),
        max_jobs: max,
        text: text.into(),
    };
    vec![
        s(
            "QCTL",
            "QSYS",
            true,
            2,
            &["*BASE"],
            None,
            "Controlling subsystem",
        ),
        s(
            "QINTER",
            "QSYS",
            true,
            15,
            &["*BASE", "*INTERACT"],
            None,
            "Interactive subsystem",
        ),
        s(
            "QBATCH",
            "QSYS",
            true,
            7,
            &["*BASE"],
            None,
            "Batch subsystem",
        ),
        s(
            "QHTTPSVR",
            "QHTTPSVR",
            true,
            14,
            &["*BASE"],
            None,
            "HTTP server subsystem",
        ),
        s(
            "QSYSWRK",
            "QSYS",
            true,
            138,
            &["*BASE"],
            None,
            "System work subsystem",
        ),
        s(
            "QUSRWRK",
            "QSYS",
            true,
            96,
            &["*BASE"],
            None,
            "User work subsystem (host servers)",
        ),
        s(
            "QSERVER",
            "QSYS",
            true,
            12,
            &["*BASE"],
            None,
            "File server subsystem",
        ),
        s(
            "QSPL",
            "QSYS",
            true,
            6,
            &["*BASE", "*SPOOL"],
            None,
            "Spooling subsystem",
        ),
        s(
            "QCMN",
            "QSYS",
            true,
            28,
            &["*BASE"],
            None,
            "Communications subsystem",
        ),
        s(
            "QPGMR",
            "QSYS",
            false,
            0,
            &["*BASE"],
            None,
            "Programmer subsystem",
        ),
        s(
            "HLINVSBS",
            "HLINV",
            false,
            0,
            &["*BASE"],
            Some(5),
            "Inventory batch subsystem",
        ),
    ]
}

fn base_pool(system_pool: u32, size_mb: u64) -> SbsPool {
    SbsPool {
        id: 1,
        storage: "*BASE".into(),
        activity: None,
        system_pool,
        size_mb,
        reserved_mb: 412,
        faults: 31,
        pages: 87,
    }
}

fn route(seq: u32, compare: &str, class: &str) -> Routing {
    Routing {
        seq,
        compare: compare.into(),
        start: 1,
        program: "QCMD".into(),
        lib: "QSYS".into(),
        class: class.into(),
        max_active: None,
        pool: 1,
    }
}

/// A subsystem's description, `monitor` its active monitor job.
pub fn description(sbs: &Subsystem, monitor: Option<String>, queues: &[JobQueue]) -> SubsystemDesc {
    let mut d = SubsystemDesc {
        sbs: sbs.clone(),
        since: sbs.active.then(|| secs(2026, 10, 27, 6, 0, 0)),
        monitor,
        signon: "QSYS/QDSIGNON".into(),
        job_queues: queues
            .iter()
            .filter(|q| q.subsystem == sbs.name)
            .map(|q| format!("{}/{}", q.lib, q.name))
            .collect(),
        pools: vec![base_pool(2, 32_768)],
        routing: vec![
            route(10, "QCMDI", "QGPL/QBATCH"),
            route(9999, "*ANY", "QGPL/QBATCH"),
        ],
        ..SubsystemDesc::default()
    };
    match sbs.name.as_str() {
        "QHTTPSVR" => {
            d.routing = vec![
                route(10, "QCMDI", "QHTTPSVR/QZHBHTTP"),
                route(9999, "*ANY", "QHTTPSVR/QZHBHTTP"),
            ];
            d.autostart = ["ADMIN", "APACHEDFT", "NAVIGATOR", "ACCESSWEB"]
                .iter()
                .map(|j| Autostart {
                    job: (*j).into(),
                    jobd: "QZHBHTTP".into(),
                    lib: "QHTTPSVR".into(),
                })
                .collect();
        }
        "QINTER" => {
            d.pools.push(SbsPool {
                id: 2,
                storage: "*INTERACT".into(),
                activity: None,
                system_pool: 4,
                size_mb: 8_192,
                reserved_mb: 0,
                faults: 12,
                pages: 40,
            });
            d.routing = vec![
                route(10, "QCMDI", "QGPL/QINTER"),
                route(9999, "*ANY", "QGPL/QINTER"),
            ];
            d.routing[0].pool = 2;
            d.routing[1].pool = 2;
            d.workstations = vec![
                Workstation {
                    name: "*ALL".into(),
                    by_type: true,
                    jobd: "*USRPRF".into(),
                    max_active: None,
                    at: "*SIGNON".into(),
                },
                Workstation {
                    name: "QPADEV*".into(),
                    by_type: false,
                    jobd: "*USRPRF".into(),
                    max_active: None,
                    at: "*SIGNON".into(),
                },
            ];
        }
        "QUSRWRK" => {
            d.prestart = [("QZDASOINIT", 3), ("QZRCSRVS", 2), ("QZSOSIGN", 1)]
                .iter()
                .map(|(p, n)| Prestart {
                    program: (*p).into(),
                    lib: "QSYS".into(),
                    user: "QUSER".into(),
                    initial: *n,
                    threshold: 1,
                    max_jobs: None,
                })
                .collect();
        }
        "QCTL" => d.autostart.push(Autostart {
            job: "QSTARTUP".into(),
            jobd: "QSTRUPJD".into(),
            lib: "QSYS".into(),
        }),
        _ => {}
    }
    d
}

/// The output queues.
pub fn output_queues() -> Vec<OutQueue> {
    let q = |name: &str, lib: &str, files: u32, wtr: &str, held: bool, text: &str| OutQueue {
        name: name.into(),
        lib: lib.into(),
        files,
        writer: wtr.into(),
        held,
        order: "*FIFO".into(),
        operator_control: true,
        text: text.into(),
    };
    vec![
        q("QPRINT", "QGPL", 148, "PRT01", false, "Default print queue"),
        q("QEZJOBLOG", "QUSRSYS", 41, "", false, "Job logs"),
        q(
            "QEZDEBUG",
            "QUSRSYS",
            3,
            "",
            false,
            "Dumps and debug output",
        ),
        q(
            "PRT02",
            "QGPL",
            12,
            "PRT02",
            true,
            "Warehouse label printer",
        ),
        q("INVRPT", "HLINV", 8, "", false, "Inventory reports"),
        q("QPDF", "QGPL", 0, "", false, "PDF output (file system)"),
    ]
}

/// The prototype's QPRINT files: a ten-file cycle of names, five users
/// with their jobs, seven user data values with their status.
fn qprint(i: u32) -> Splf {
    const FILES: [&str; 10] = [
        "QPJOBLOG",
        "QSYSPRT",
        "QPRINT",
        "INVRPT",
        "QPDSPJOB",
        "QPQUPRFIL",
        "QPRTSPLQ",
        "QSYSPRT",
        "QPJOBLOG",
        "INVLABEL",
    ];
    const PAGES: [u32; 10] = [1, 3, 12, 1, 44, 2, 7, 1, 2, 118];
    const USERS: [(&str, &str); 5] = [
        ("QPGMR", "NIGHTLY"),
        ("CWELLMAN", "INVBATCH"),
        ("QSECOFR", "QPADEV0001"),
        ("OPERATOR", "QPADEV0004"),
        ("QTMHHTTP", "ADMIN"),
    ];
    const DATA: [(&str, &str); 7] = [
        ("NIGHTLY", "RDY"),
        ("INVBATCH", "RDY"),
        ("", "HLD"),
        ("DSPJOB", "RDY"),
        ("WRKSPLF", "SAV"),
        ("RGZPFM01", "RDY"),
        ("ADMIN", "WTR"),
    ];
    let k = i as usize;
    let (user, job) = USERS[k % 5];
    let (ud, st) = DATA[k % 7];
    // Three files an hour from 06:00, seven minutes apart; the older
    // ones a day earlier each 27 files.
    let day = u64::from(i / 27) * 86_400;
    let at = secs(2026, 10, 29, 6, 0, 0)
        + u64::from(i % 27 / 3) * 3600
        + u64::from(i % 27 * 7 % 60) * 60;
    Splf {
        file: FILES[k % 10].into(),
        user: user.into(),
        user_data: ud.into(),
        status: st.into(),
        pages: PAGES[k % 10],
        copies: 1,
        form_type: "*STD".into(),
        created: at - day,
        job: format!("{}/{user}/{job}", 123_400 + i),
        number: 400 + i,
        outq: "QGPL/QPRINT".into(),
    }
}

/// Every spooled file: 148 on QGPL/QPRINT, the rest on the other
/// queues.
pub fn spooled_files() -> Vec<Splf> {
    let mut out: Vec<Splf> = (0..147).map(qprint).collect();
    // The inventory report the viewer opens.
    out.push(Splf {
        file: "INVRPT".into(),
        user: "CWELLMAN".into(),
        user_data: "INVBATCH".into(),
        status: "RDY".into(),
        pages: 12,
        copies: 1,
        form_type: "*STD".into(),
        created: secs(2026, 10, 29, 6, 14, 2),
        job: "123402/CWELLMAN/INVBATCH".into(),
        number: 402,
        outq: "QGPL/QPRINT".into(),
    });
    let others = [
        ("QUSRSYS/QEZJOBLOG", 41, "QPJOBLOG"),
        ("QUSRSYS/QEZDEBUG", 3, "QPSRVDMP"),
        ("QGPL/PRT02", 12, "INVLABEL"),
        ("HLINV/INVRPT", 8, "INVRPT"),
    ];
    let mut n = 0;
    for (q, count, file) in others {
        for _ in 0..count {
            n += 1;
            out.push(Splf {
                file: file.into(),
                user: "QPGMR".into(),
                user_data: String::new(),
                status: if q == "QGPL/PRT02" {
                    "HLD".into()
                } else {
                    "RDY".into()
                },
                pages: 1 + n % 4,
                copies: 1,
                form_type: if q == "QGPL/PRT02" {
                    "*LABEL".into()
                } else {
                    "*STD".into()
                },
                created: secs(2026, 10, 28, 6, 0, 0) + u64::from(n) * 600,
                job: format!("{}/QPGMR/NIGHTLY", 123_000 + n),
                number: 1,
                outq: q.into(),
            });
        }
    }
    out
}

/// The printers and writers.
pub fn writers() -> Vec<Writer> {
    let w =
        |dev: &str, started: bool, on: Option<bool>, oq: &str, form: &str, model: &str| Writer {
            device: dev.into(),
            started,
            held: false,
            varied_on: on,
            outq: oq.into(),
            form_type: form.into(),
            model: model.into(),
            ..Writer::default()
        };
    let mut prt01 = w("PRT01", true, Some(true), "QGPL/QPRINT", "*STD", "3812 / 1");
    prt01.file = "QSYSPRT".into();
    prt01.user = "QPGMR".into();
    prt01.pages_left = Some(3);
    vec![
        prt01,
        w(
            "PRT02",
            false,
            Some(true),
            "QGPL/PRT02",
            "*LABEL",
            "*LAN 3812",
        ),
        w(
            "PRT03",
            false,
            Some(false),
            "QGPL/PRT03",
            "*STD",
            "*LAN 3812",
        ),
        w(
            "QPDFWTR",
            true,
            None,
            "QGPL/QPDF",
            "*ALL",
            "PDF (remote output queue)",
        ),
        w(
            "RMTLASER",
            true,
            None,
            "QGPL/RMTLASER",
            "*ALL",
            "Remote (LPR 10.0.10.90)",
        ),
    ]
}

/// The job schedule.
pub fn schedule() -> Vec<ScheduleEntry> {
    let day = |m: u32, d: u32| secs(2026, m, d, 0, 0, 0);
    let at = |m: u32, d: u32, h: u32, mi: u32| Some(secs(2026, m, d, h, mi, 0));
    let e =
        |name: &str, n: u32, cmd: &str, frq: &str, days: &[&str], time: &str, next, user: &str| {
            ScheduleEntry {
                name: name.into(),
                entry: n,
                command: cmd.into(),
                frequency: frq.into(),
                date: None,
                days: days.iter().map(ToString::to_string).collect(),
                time: time.into(),
                next,
                jobq: "QGPL/QBATCH".into(),
                user: user.into(),
                held: false,
                text: String::new(),
            }
        };
    let mut rgz = e(
        "RGZPFM01",
        4,
        "RGZPFM FILE(HLINV/ASSETS)",
        "*MONTHLY",
        &[],
        "03:00",
        at(11, 1, 3, 0),
        "QPGMR",
    );
    rgz.date = Some(day(11, 1));
    rgz.held = true;
    let mut inv = e(
        "INVLOAD",
        5,
        "CALL PGM(HLINV/INVLOAD) PARM('*ALL')",
        "*WEEKLY",
        &["*ALL"],
        "06:00",
        at(10, 30, 6, 0),
        "CWELLMAN",
    );
    inv.jobq = "HLINV/INVLOAD".into();
    let mut cln = e(
        "CLNUPJOBLOG",
        7,
        "DLTSPLF FILE(QPJOBLOG) SELECT(*ALL *ALL *ALL *ALL) JOB(*) SPLNBR(*ALL)",
        "*ONCE",
        &[],
        "05:00",
        at(11, 15, 5, 0),
        "QSECOFR",
    );
    cln.date = Some(day(11, 15));
    vec![
        e(
            "NIGHTLY",
            1,
            "CALL PGM(QGPL/NIGHTLY)",
            "*WEEKLY",
            &["*ALL"],
            "02:00",
            at(10, 30, 2, 0),
            "QPGMR",
        ),
        e(
            "SAVDAILY",
            2,
            "SAVLIB LIB(*ALLUSR) DEV(TAP01) SAVACT(*LIB)",
            "*WEEKLY",
            &["*MON", "*TUE", "*WED", "*THU", "*FRI"],
            "23:30",
            at(10, 29, 23, 30),
            "QSECOFR",
        ),
        e(
            "SAVWEEKLY",
            3,
            "SAVSYS DEV(TAP01)",
            "*WEEKLY",
            &["*SUN"],
            "01:00",
            at(11, 1, 1, 0),
            "QSECOFR",
        ),
        rgz,
        inv,
        e(
            "CHKFIX",
            6,
            "CHKPTF UPDSVR(*QUARTZ)",
            "*WEEKLY",
            &["*SUN"],
            "04:00",
            at(11, 1, 4, 0),
            "QSECOFR",
        ),
        cln,
    ]
}

/// Page 1 of the inventory report (`INVRPT` number 402).
pub const INVRPT_PAGE1: &[&str] = &[
    "QPRINT    INVRPT      INVENTORY REPORT BY LOCATION                       QSHQ400   10/29/26  06:14:02   PAGE    1",
    "                                                                            HLINV/INVRPT      USER CWELLMAN",
    "",
    "LOCATION  ASSET TAG   MANUFACTURER      MODEL                 SERIAL              TYPE        STATUS   ACQUIRED    NOTES",
    "--------  ----------  ----------------  --------------------  ------------------  ----------  -------  ----------  ------------------------",
    "QS-HQ-R1  QS000101    QUANTA            T4048-IX8             QTFCEV2340021       SWITCH      ACTIVE   2025-03-11  SPINE-1 SONIC",
    "QS-HQ-R1  QS000102    QUANTA            T4048-IX8             QTFCEV2340022       SWITCH      ACTIVE   2025-03-11  SPINE-2 SONIC",
    "QS-HQ-R1  QS000110    EDGECORE          AS5712-54X            571254X2041B0019    SWITCH      ACTIVE   2024-11-02  LEAF-1",
    "QS-HQ-R1  QS000111    EDGECORE          AS5712-54X            571254X2041B0020    SWITCH      ACTIVE   2024-11-02  LEAF-2",
    "QS-HQ-R2  QS000201    DELL              R740XD                7XK2Q33             SERVER      ACTIVE   2024-06-20  PVE-01 CEPH",
    "QS-HQ-R2  QS000202    DELL              R740XD                7XK2Q34             SERVER      ACTIVE   2024-06-20  PVE-02 CEPH",
    "QS-HQ-R2  QS000203    DELL              R740XD                7XK2Q35             SERVER      ACTIVE   2024-06-20  PVE-03 CEPH",
    "QS-HQ-R2  QS000210    DELL              QLE2562 HBA           BFE1234567          ADAPTER     ACTIVE   2026-09-20  QS-HQ-VTL",
    "QS-HQ-R3  QS000301    SUPERMICRO        SYS-5019D-FN8TP       S412345X9A01234     SERVER      ACTIVE   2023-02-14  LAB BUILD BOX",
    "QS-HQ-R3  QS000302    WEIDMULLER        UR20-FBC-EIP          2024-11-00341       IO-COUPLER  ACTIVE   2026-10-01  LAB RACK",
    "QS-BR1-R1 QS000401    DELL              S4048-ON              8H3K2P2             SWITCH      ACTIVE   2024-09-08  BRANCH CORE",
    "QS-BR1-R1 QS000402    WATCHGUARD        M370                  80BA0F2B1C33        FIREWALL    ACTIVE   2024-09-08  BRANCH EDGE",
    "",
    "                                                   TOTAL ASSETS THIS LOCATION GROUP:    12",
];

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped system panel groups against the pub400 goldens: each
//! panel rendered with the golden's visible values and compared row by
//! row (`common::compare`). One test per golden, named after it.

mod common;

use common::{compare, golden, shipped};
use uim::render::{ListState, MapDialog, Options, render};

/// A complete list from rows of `(variable, value)` pairs.
fn list(rows: &[Vec<(&str, &str)>]) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect()
            })
            .collect(),
        top: 0,
        total: Some(rows.len()),
        complete: true,
    }
}

/// An incomplete list: more entries follow the ones given.
fn partial(rows: &[Vec<(&str, &str)>]) -> ListState {
    ListState {
        total: None,
        complete: false,
        ..list(rows)
    }
}

fn opts(system: &str) -> Options {
    Options {
        system: system.into(),
        ..Options::default()
    }
}

// --- work management (work/qsjob/src/panels/work.rs)

/// The first nine active jobs of the WRKACTJOB captures: name, current
/// user, type, function, status, pool, priority, CPU seconds, threads,
/// temporary storage.
const ACTJOBS: &[[&str; 10]] = &[
    ["#SYSLOAD", "QSYS", "SBS", "", "DEQW", "2", "0", "0,0", "2", "7"],
    ["  SYSLOAD", "#SYSLOAD", "ASJ", "DLY-321", "DLYW", "2", "1", "3,9", "1", "13"],
    ["QBATCH", "QSYS", "SBS", "", "DEQW", "2", "0", "82,1", "2", "7"],
    ["  IRESTAPI", "JJMENDES", "BCH", "CMD-QSH", "TIMW", "2", "99", "0,0", "1", "8"],
    ["  MAPEPIRE", "QSECOFR", "BCH", "CMD-STRQSH", "TIMW", "2", "99", "0,3", "1", "8"],
    ["  QP0ZSPWT", "JJMENDES", "BCI", "PGM-node", "SELW", "2", "99", "4,9", "11", "68"],
    ["  QP0ZSPWT", "QSECOFR", "BCI", "PGM-jFromPASE", "EVTW", "2", "99", "0,0", "1", "12"],
    ["  QP0ZSPWT", "QSECOFR", "BCI", "JVM-/QOpenSys/", "THDW", "2", "99", "139,4", "61", "393"],
    ["  QZSHSH", "JJMENDES", "BCI", "PGM-QZSHSH", "EVTW", "2", "99", "0,0", "1", "8"],
];

/// WRKACTJOB's dialog: a view, the CPU figure as shown.
fn wrkactjob_dialog(view: &str, cpu: &str, count: &str) -> MapDialog {
    let rows: Vec<Vec<(&str, &str)>> = ACTJOBS
        .iter()
        .map(|j| {
            let status: &'static str = Box::leak(format!(" {}", j[4]).into_boxed_str());
            vec![
                ("JOB", j[0]),
                ("CURUSER", j[1]),
                ("USER", j[1]),
                ("NUMBER", "NNNNNN"),
                ("TYPE", j[2]),
                ("CPU", cpu),
                ("FUNCTION", j[3]),
                ("STATUS", status),
                ("POOL", j[5]),
                ("PTY", j[6]),
                ("CPUSEC", j[7]),
                ("AUXIO", "0"),
                ("ECPU", cpu),
                ("THREADS", j[8]),
                ("TMPSTG", j[9]),
            ]
        })
        .collect();
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set("SYSCPU", cpu)
        .set("ELAPSED", "HH:MM:SS")
        .set("ACTJOBS", count)
        .set("VIEW", view)
        .set_list("JOBS", partial(&rows));
    d
}

fn wrkactjob_screen(panel: &str, view: usize, more_keys: bool, cpu: &str, count: &str) -> uim::Screen {
    let g = shipped("QWCWRKAJ");
    let d = wrkactjob_dialog(&(view + 1).to_string(), cpu, count);
    let o = Options {
        view,
        more_keys,
        ..opts("PUB400")
    };
    render(&g, panel, &d, &o).expect("render")
}

#[test]
#[ignore = "WRKACTJOB is a display file on the oracle: views 1 and 2 put their keys on a grid R7 does not make"]
fn wrkactjob() {
    compare(&golden("wrkactjob"), &wrkactjob_screen("WRKACTJ1", 0, false, "0,0", "922"));
}

#[test]
#[ignore = "WRKACTJOB is a display file on the oracle: views 1 and 2 put their keys on a grid R7 does not make"]
fn wrkactjob_f11() {
    compare(&golden("wrkactjob-f11"), &wrkactjob_screen("WRKACTJ1", 1, false, "0,0", "922"));
}

#[test]
fn wrkactjob_f11_2() {
    compare(&golden("wrkactjob-f11-2"), &wrkactjob_screen("WRKACTJ1", 2, false, "0,0", "922"));
}

#[test]
fn wrkactjob_f24() {
    compare(&golden("wrkactjob-f24"), &wrkactjob_screen("WRKACTJ1", 2, true, "0,0", "922"));
}

#[test]
fn wrkactjob_f24_keys() {
    compare(&golden("wrkactjob-f24-keys"), &wrkactjob_screen("WRKACTJ1", 0, true, "N.N", "594"));
}

#[test]
#[ignore = "WRKACTJOB is a display file on the oracle: legends and keys sit on a fixed grid R7 does not make"]
fn wrkactjob_f23_options() {
    compare(&golden("wrkactjob-f23-options"), &wrkactjob_screen("WRKACTJ2", 0, false, "N.N", "594"));
}

/// The job the WRKJOB and DSPJOB captures show.
fn job_dialog(change: bool) -> MapDialog {
    let mut d = MapDialog::new();
    d.set("JOB", "QPADEV000R")
        .set("USER", "USERNAME")
        .set("NUMBER", "NNNNNN")
        .set("CANCHG", if change { "Y" } else { "N" });
    d
}

fn job_screen(panel: &str, d: &MapDialog, o: &Options) -> uim::Screen {
    render(&shipped("QUIWRKJOB"), panel, d, o).expect("render")
}

#[test]
fn wrkjob() {
    let d = job_dialog(true);
    compare(&golden("wrkjob"), &job_screen("WRKJOB", &d, &opts("SYSNAME")));
}

#[test]
fn dspjob_menu() {
    let d = job_dialog(false);
    compare(&golden("dspjob-menu"), &job_screen("DSPJOB", &d, &opts("SYSNAME")));
}

#[test]
fn dspjob_status() {
    let mut d = job_dialog(false);
    for (k, v) in [
        ("STATUS", "ACTIVE"),
        ("CURUSER", "USERNAME"),
        ("IDENTITY", "USERNAME"),
        ("SETBY", "*DEFAULT"),
        ("ENTDATE", "MM/DD/YY"),
        ("ENTTIME", "HH:MM:SS"),
        ("STRDATE", "MM/DD/YY"),
        ("STRTIME", "HH:MM:SS"),
        ("SBS", "QINTER2"),
        ("SBSPOOL", "2"),
        ("TYPE", "INTER"),
        ("SPCENV", "*NONE"),
        ("RTNCODE", "0"),
    ] {
        d.set(k, v);
    }
    compare(&golden("dspjob-status"), &job_screen("STSA", &d, &opts("SYSNAME")));
}

#[test]
fn wrkjob_3_status() {
    let mut d = job_dialog(true);
    for (k, v) in [
        ("RUNPTY", "50"),
        ("TIMESLICE", "2000"),
        ("PURGE", "*YES"),
        ("DFTWAIT", "30"),
        ("CPUTIME", "*NOMAX"),
        ("CPUUSED", "10"),
        ("MAXTMP", "*NOMAX"),
        ("TMPUSED", "7"),
        ("TMPPEAK", "7"),
        ("MAXTHD", "*NOMAX"),
        ("THREADS", "1"),
        ("AFFGRP", "*NOGROUP"),
        ("AFFLVL", "*NORMAL"),
        ("RSCAFF", "*NO"),
    ] {
        d.set(k, v);
    }
    compare(&golden("wrkjob-3-status"), &job_screen("RUNA", &d, &opts("SYSNAME")));
}

#[test]
#[ignore = "a display file on the oracle: the second legend row and the headings sit on its own grid"]
fn wrkjob_4_splf() {
    let mut d = job_dialog(true);
    d.set_list("SPLF", list(&[]));
    compare(&golden("wrkjob-4-splf"), &job_screen("SPLF", &d, &opts("SYSNAME")));
}

/// The job log header: the job's names with their leaders.
fn log_header(d: &mut MapDialog) {
    let hdr = format!(
        "Job . . :   {:<10}    User . . :   {:<10}    Number . . . :   {}",
        "QPADEV000R", "USERNAME", "NNNNNN"
    );
    d.set("JOBHDR", &hdr);
}

#[test]
#[ignore = "a display file on the oracle: F24 sits under F10, not where R7 puts it"]
fn wrkjob_10_joblog() {
    let mut d = job_dialog(true);
    log_header(&mut d);
    d.set_list("MSGS", list(&[vec![("MSGLINE", "3>> wrkjob")]]));
    compare(&golden("wrkjob-10-joblog"), &job_screen("JOBLOG", &d, &opts("SYSNAME")));
}

#[test]
#[ignore = "a display file on the oracle: its title is one column left of R1"]
fn dspjoblog() {
    let mut d = job_dialog(false);
    log_header(&mut d);
    d.set_list("MSGS", list(&[vec![("MSGLINE", "3>> dspjoblog")]]));
    compare(&golden("dspjoblog"), &job_screen("JOBLOG", &d, &opts("SYSNAME")));
}

#[test]
#[ignore = "a display file on the oracle: its single key row sits a row above where R6 puts it"]
fn dspjoblog_f10() {
    let mut d = job_dialog(false);
    log_header(&mut d);
    d.set_list("MSGS", list(&[vec![("MSGLINE", "3>> dspjoblog")]]));
    compare(&golden("dspjoblog-f10"), &job_screen("JOBLOGALL", &d, &opts("SYSNAME")));
}

#[test]
#[ignore = "a display file on the oracle: the program and library columns are one apart"]
fn wrkjob_11_callstk() {
    let mut d = job_dialog(true);
    d.set("THREAD", "00000A9D");
    let rows: Vec<Vec<(&str, &str)>> = [
        ("", "QCMD", "/0519"),
        ("", "QUICMENU", "/00C1"),
        ("1", "QUIMNDRV", "/061E"),
        ("2", "QUIMGFLW", "/04D9"),
        ("3", "QUICMD", "/056F"),
    ]
    .iter()
    .map(|(t, p, s)| vec![("RQSLVL", *t), ("PGM", *p), ("PGMLIB", "QSYS"), ("STMT", *s)])
    .collect();
    d.set_list("STACK", list(&rows));
    compare(&golden("wrkjob-11-callstk"), &job_screen("CALLSTK", &d, &opts("SYSNAME")));
}

#[test]
fn wrkjob_13_libl() {
    let mut d = job_dialog(true);
    let libs = [
        ("PUB400SYS", "SYS", "$ PUB400.COM global programs"),
        ("QSYS", "SYS", "System Library"),
        ("QSYS2", "SYS", "System Library for CPI's"),
        ("QUSRSYS", "SYS", "System Library for Users"),
        ("QHLPSYS", "SYS", ""),
        ("USERNAME1", "CUR", "USERNAME (*CURLIB your current library)"),
        ("QGPL", "USR", "General Purpose Library"),
        ("QTEMP", "USR", ""),
        ("GAMES400", "USR", "$ fun library"),
    ];
    let rows: Vec<Vec<(&str, &str)>> = libs
        .iter()
        .map(|(l, t, x)| vec![("LIB", *l), ("LIBTYPE", *t), ("LIBTEXT", *x)])
        .collect();
    d.set_list("LIBL", list(&rows));
    let g = golden("wrkjob-13-libl");
    let copyright = g.lines().nth(23).unwrap_or("").trim().to_string();
    let o = Options {
        show_copyright: true,
        copyright,
        ..opts("SYSNAME")
    };
    compare(&g, &job_screen("LIBL", &d, &o));
}

#[test]
#[ignore = "a display file on the oracle: its columns are one or two apart and the header has its own spacing"]
fn wrkjob_14_openfiles() {
    let mut d = job_dialog(true);
    let hdr = format!(
        "Job . . :   {:<10}     User . . :   {:<10}     Number . . . :   {}",
        "QPADEV000R", "USERNAME", "NNNNNN"
    );
    d.set("JOBHDR", &hdr).set("ODPS", "2");
    let rows: Vec<Vec<(&str, &str)>> = ["QDUI80", "QDDSPOF"]
        .iter()
        .map(|f| {
            vec![
                ("OFILE", *f),
                ("OLIB", "QSYS"),
                ("OMBR", "QPADEV000R"),
                ("OSCOPE", "*ACTGRPDFN"),
                ("OACTGRP", "*DFTACTGRP 0000000000000002"),
            ]
        })
        .collect();
    d.set_list("OPNF", list(&rows));
    compare(&golden("wrkjob-14-openfiles"), &job_screen("OPNF", &d, &opts("SYSNAME")));
}

/// Jobs as WRKUSRJOB and WRKSBSJOB list them: name, user, type,
/// status, function.
fn user_jobs(jobs: &[[&'static str; 5]]) -> Vec<Vec<(&'static str, &'static str)>> {
    jobs.iter()
        .map(|j| {
            vec![
                ("JOB", j[0]),
                ("USER", j[1]),
                ("NUMBER", "NNNNNN"),
                ("TYPE", j[2]),
                ("STATUS", j[3]),
                ("FUNCTION", j[4]),
            ]
        })
        .collect()
}

#[test]
fn wrkusrjob() {
    let rows = user_jobs(&[
        ["QPADEV000R", "USERNAME", "INTER", "ACTIVE", "CMD-WRKUSRJOB"],
        ["QPRTJOB", "USERNAME", "PRINT", "OUTQ", ""],
    ]);
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set_list("JOBS", list(&rows));
    let s = render(&shipped("QUIWRKUSRJ"), "WRKUSRJOB", &d, &opts("PUB400")).expect("render");
    compare(&golden("wrkusrjob"), &s);
}

#[test]
fn wrksbsjob() {
    let rows = user_jobs(&[
        ["QPADEV000G", "AVINASH1", "INTER", "ACTIVE", "CMD-WRKMBRPDM"],
        ["QPADEV000M", "YUVARAJ15", "INTER", "ACTIVE", "MNU-MAIN"],
        ["QPADEV000R", "USERNAME", "INTER", "ACTIVE", "CMD-WRKSBSJOB"],
        ["QPADEV000W", "HIMANSH", "INTER", "ACTIVE", "CMD-WRKMBRPDM"],
        ["QPADEV001L", "SRIPRIYAA", "INTER", "ACTIVE", "CMD-STRSEU"],
        ["QPADEV001S", "IAMSAM", "INTER", "ACTIVE", "MNU-MAIN"],
        ["QPADEV001W", "KIRTIK782", "INTER", "ACTIVE", "CMD-STRSEU"],
        ["QPADEV002L", "SHUBHAM93", "INTER", "ACTIVE", "CMD-STRSEU"],
    ]);
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set("SBS", "QINTER2")
        .set_list("JOBS", partial(&rows));
    let s = render(&shipped("QUIWRKUSRJ"), "WRKSBSJOB", &d, &opts("PUB400")).expect("render");
    compare(&golden("wrksbsjob"), &s);
}

#[test]
#[ignore = "a display file on the oracle: the storage column and the pool group sit off R9's grid"]
fn wrksbs() {
    let names = [
        "#SYSLOAD", "QBATCH", "QBATCH2", "QBATCH3", "QBATCH4", "QBATCH5", "QBATCH6", "QBATCH7",
        "QCMN", "QCTL",
    ];
    let rows: Vec<Vec<(&str, &str)>> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let pool = if i == 0 { ("P2", "2") } else { ("P1", "2") };
            vec![("SBS", *n), ("STORAGE", "0,0"), pool]
        })
        .collect();
    let mut d = MapDialog::new();
    d.set_list("SBS", partial(&rows));
    let s = render(&shipped("QUIWRKSBS"), "WRKSBS", &d, &opts("SYSNAME")).expect("render");
    compare(&golden("wrksbs"), &s);
}

fn sbsd_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    d.set("SBS", "QINTER2")
        .set("SBSLIB", "QSYS")
        .set("STATUS", "ACTIVE");
    d
}

fn sbsd_screen(panel: &str, d: &MapDialog) -> uim::Screen {
    render(&shipped("QUIDSPSBSD"), panel, d, &opts("SYSNAME")).expect("render")
}

#[test]
fn dspsbsd_menu() {
    compare(&golden("dspsbsd-menu"), &sbsd_screen("DSPSBSD", &sbsd_dialog()));
}

#[test]
fn dspsbsd_1() {
    let mut d = sbsd_dialog();
    d.set("MAXJOBS", "*NOMAX")
        .set("SGNDSPF", "QDSIGNON2")
        .set("SGNLIB", "QGPL")
        .set("SYSLIBLE", "*NONE")
        .set("ASPGRP", "*NONE")
        .set("WLGRP", "*NONE");
    compare(&golden("dspsbsd-1"), &sbsd_screen("OPRATR", &d));
}

#[test]
#[ignore = "a display file on the oracle: its headings are centered over the values"]
fn dspsbsd_2_pools() {
    let mut d = sbsd_dialog();
    let rows = vec![
        vec![("POOLID", "1"), ("POOLSIZE", "*BASE")],
        vec![("POOLID", "2"), ("POOLSIZE", "*INTERACT")],
    ];
    d.set_list("POOLS", list(&rows));
    compare(&golden("dspsbsd-2-pools"), &sbsd_screen("POOLS", &d));
}

#[test]
fn dspsbsd_3_aje() {
    let mut d = sbsd_dialog();
    d.set_list("AJES", list(&[]));
    compare(&golden("dspsbsd-3-aje"), &sbsd_screen("AJES", &d));
}

#[test]
#[ignore = "the oracle lays the entries out in four columns across the screen"]
fn dspsbsd_5_wste() {
    let mut d = sbsd_dialog();
    let rows = vec![
        vec![("WSNAME", "*CONS")],
        vec![("WSNAME", "5555")],
        vec![("WSNAME", "*ALL")],
    ];
    d.set_list("WSES", list(&rows));
    compare(&golden("dspsbsd-5-wste"), &sbsd_screen("WSTES", &d));
}

#[test]
#[ignore = "a display file on the oracle: the list starts at column 3 and centers its headings"]
fn dspsbsd_6_jobqe() {
    let mut d = sbsd_dialog();
    let mut rows = Vec::new();
    for (seq, q) in [("10", "QINTER"), ("20", "QS36MRT")] {
        let mut r = vec![("SEQ", seq), ("JOBQ", q), ("JOBQLIB", "QGPL"), ("MAXACT", "*NOMAX")];
        for m in ["M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8", "M9"] {
            r.push((m, "*"));
        }
        rows.push(r);
    }
    d.set_list("JOBQES", list(&rows));
    compare(&golden("dspsbsd-6-jobqe"), &sbsd_screen("JOBQES", &d));
}

#[test]
#[ignore = "a display file on the oracle: its columns sit on their own grid"]
fn dspsbsd_7_rtge() {
    let mut d = sbsd_dialog();
    let rows: Vec<Vec<(&str, &str)>> = [
        ("10", "QCMD", "'QCMDI'", "1"),
        ("15", "QCMD", "'QIGC'", "1"),
        ("20", "QCMD", "'QS36MRT'", "1"),
        ("40", "QARDRIVE", "'525XTEST'", "1"),
        ("700", "QCL", "'QCMD38'", "1"),
        ("9999", "QCMD", "*ANY", ""),
    ]
    .iter()
    .map(|(s, p, c, n)| {
        vec![("RTSEQ", *s), ("PGM", *p), ("PGMLIB", "QSYS"), ("CMPVAL", *c), ("STARTPOS", *n)]
    })
    .collect();
    d.set_list("RTGES", list(&rows));
    compare(&golden("dspsbsd-7-rtge"), &sbsd_screen("RTGES", &d));
}

/// An object list whose first entry is the blank line option 1 types
/// a new name on.
fn objects(var: &'static str, items: &[(&'static str, &'static str)]) -> Vec<Vec<(&'static str, &'static str)>> {
    let mut rows = vec![vec![(var, ""), ("LIB", ""), ("TEXT", "")]];
    rows.extend(
        items
            .iter()
            .map(|(n, t)| vec![(var, *n), ("LIB", "QGPL"), ("TEXT", *t)]),
    );
    rows
}

#[test]
#[ignore = "WRKJOBD centers the first heading row over the second; R9 puts it at the column start"]
fn wrkjobd() {
    let rows = objects(
        "JOBD",
        &[
            ("BKP4I", ""),
            ("CODE4I", ""),
            ("CODE4IB", ""),
            ("CODE4I1", ""),
            ("CODE4I2", ""),
            ("USERNAME", ""),
            ("GANESH4", ""),
            ("JEEVANA1", "jeevana bulla                    ENU"),
            ("LOKE241", "Lokeshwari Cherukuri             ENU"),
            ("MALLI2404", ""),
        ],
    );
    let mut d = MapDialog::new();
    d.set_list("JOBDS", partial(&rows));
    let s = render(&shipped("QUIWRKJOBD"), "WRKJOBD", &d, &opts("SYSNAME")).expect("render");
    compare(&golden("wrkjobd"), &s);
}

fn jobd_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("JOBD", "QDFTJOBD"),
        ("LIB", "QGPL"),
        ("USER", "*RQD"),
        ("SYNTAX", "*NOCHK"),
        ("HOLD", "*NO"),
        ("ENDSEV", "30"),
        ("JOBDATE", "*SYSVAL"),
        ("SWS", "00000000"),
        ("INQMSG", "*SYSRPYL"),
        ("JOBPTY", "5"),
        ("JOBQ", "QBATCH"),
        ("JOBQLIB", "QGPL"),
        ("OUTPTY", "5"),
        ("PRTDEV", "*USRPRF"),
        ("OUTQ", "*USRPRF"),
        ("LOGLVL", "4"),
        ("LOGSEV", "20"),
        ("LOGTXT", "*SECLVL"),
        ("LOGCLPGM", "*YES"),
        ("LOGOUT", "*SYSVAL"),
        ("ACGCDE", "*USRPRF"),
        ("PRTTXT", "*SYSVAL"),
        ("RTGDTA", "QCMDI"),
        ("RQSDTA", "*NONE"),
    ] {
        d.set(k, v);
    }
    d
}

#[test]
fn dspjobd() {
    let s = render(&shipped("QUIWRKJOBD"), "DSPJOBD", &jobd_dialog(), &opts("SYSNAME")).expect("render");
    compare(&golden("dspjobd"), &s);
}

#[test]
fn dspjobd_2() {
    let o = Options {
        page: 1,
        ..opts("SYSNAME")
    };
    let s = render(&shipped("QUIWRKJOBD"), "DSPJOBD", &jobd_dialog(), &o).expect("render");
    compare(&golden("dspjobd-2"), &s);
}

#[test]
fn wrkcls() {
    let rows = objects(
        "CLS",
        &[
            ("BATCHUSR", ""),
            ("QBATCH", "Batch Subsystem Class"),
            ("QBATCH2", "Class for QBATCH2 subsystem"),
            ("QBATCH3", "Class for QBATCH3 subsystem"),
            ("QBATCH4", "Class for QBATCH4 subsystem"),
            ("QBATCH5", "Class for QBATCH5 subsystem"),
            ("QBATCH6", "Class for QBATCH6 subsystem"),
            ("QBATCH7", "Class for QBATCH7 subsystem"),
            ("QCASERVR", "Client Access Server Class"),
            ("QDIALOCAL", "Class for QDIALOCAL Job"),
            ("QFNC", "Finance Subsystem Class"),
        ],
    );
    let mut d = MapDialog::new();
    d.set_list("CLASSES", partial(&rows));
    let s = render(&shipped("QUIWRKCLS"), "WRKCLS", &d, &opts("SYSNAME")).expect("render");
    compare(&golden("wrkcls"), &s);
}

#[test]
#[ignore = "the oracle's text field runs into the last column and on to the next row"]
fn dspcls() {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("CLS", "QINTER"),
        ("LIB", "QGPL"),
        ("RUNPTY", "20"),
        ("TIMESLICE", "2000"),
        ("PURGE", "*YES"),
        ("DFTWAIT", "30"),
        ("CPUTIME", "*NOMAX"),
        ("MAXTMP", "*NOMAX"),
        ("MAXTHD", "*NOMAX"),
        ("TEXT", "Interactive Subsystem Class"),
    ] {
        d.set(k, v);
    }
    let s = render(&shipped("QUIWRKCLS"), "DSPCLS", &d, &opts("SYSNAME")).expect("render");
    compare(&golden("dspcls"), &s);
}

#[test]
#[ignore = "a display file on the oracle: the second legend row and the single key row sit on its own grid"]
fn wrksplf() {
    let files = [
        ("QDFUZZIN", "3"),
        ("QDFUZZOUT", "3"),
        ("QDFUZZ", "178"),
        ("QDFUZZ", "178"),
        ("QORACLE", "8"),
        ("CVTCHAR", "8"),
        ("DATEBAD", "8"),
        ("SUBSTBAD", "8"),
        ("RESUMEG", "8"),
    ];
    let rows: Vec<Vec<(&str, &str)>> = files
        .iter()
        .map(|(f, p)| {
            vec![
                ("FILE", *f),
                ("USER", "USERNAME"),
                ("DEVQ", "USERNAME"),
                ("STS", "RDY"),
                ("PAGES", *p),
                ("COPIES", "1"),
            ]
        })
        .collect();
    let mut d = MapDialog::new();
    d.set_list("FILES", partial(&rows));
    let s = render(&shipped("QUIWRKSPLF"), "WRKSPLF", &d, &opts("SYSNAME")).expect("render");
    compare(&golden("wrksplf"), &s);
}

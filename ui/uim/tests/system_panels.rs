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

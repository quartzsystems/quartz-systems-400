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

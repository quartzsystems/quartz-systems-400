// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security attributes, function usage and journal receiver panels
//! against the primary's captures (`golden/verified/oracle71/d3/`, V7R1M0):
//! each rendered with the capture's values and compared row by row.

mod common;

use std::path::PathBuf;

use common::{compare_in, compare_ours_in, shipped};
use uim::render::{ListState, MapDialog, Options, render};

fn d3() -> PathBuf {
    common::golden_dir().join("../oracle71/d3")
}

fn v71(group: &uim::PanelGroup) -> Options {
    Options {
        system: "SYSNAME".into(),
        show_copyright: true,
        copyright: group.copyright.clone().unwrap_or_default(),
        ..Options::default()
    }
}

fn row(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn dspseca() {
    let g = shipped("QUISECA");
    let mut d = MapDialog::new();
    d.set("SAUID", "109")
        .set("SAGID", "102")
        .set("SASECLVL", "40")
        .set("SAPWDLVL", "0")
        .set("SASYSVAL", "*YES")
        .set("SACERT", "*YES")
        .set("SASTPWD", "*NO");
    compare_in(
        &d3(),
        "dspseca",
        &render(&g, "DSPSECA", &d, &v71(&g)).unwrap(),
    );
}

/// The functions of the capture's first page, by ID and name.
const FIRST_PAGE: &[(&str, &str)] = &[
    (
        "QIBM_DIRSRV_ADMIN",
        "IBM Tivoli Directory Server Administrator",
    ),
    ("QIBM_ACCESS_ALLOBJ_JOBLOG", "Access job log of *ALLOBJ job"),
    ("QIBM_ALLOBJ_TRACE_ANY_USER", "Trace any user"),
    ("QIBM_WATCH_ANY_JOB", "Watch any job"),
    ("QIBM_DB_SQLADM", "Database Administrator"),
    ("QIBM_DB_SYSMON", "Database Information"),
    ("QIBM_QCST_SERVICE_CLUSTADMIN", "Cluster Administration"),
    ("QIBM_QCST_SERVICE_CLUSTOPER", "Cluster Operation"),
    ("QIBM_QYAS_SERVICE_DISKMGMT", "Disk units"),
    ("QIBM_SERVICE_DISK_WATCHER", "DISK WATCHER"),
    ("QIBM_SERVICE_DUMP", "Service dump"),
    ("QIBM_SERVICE_JOB_WATCHER", "JOB WATCHER"),
    ("QIBM_SERVICE_THREAD", "Thread control"),
];

#[test]
fn wrkfcnusg() {
    let g = shipped("QUIFCNUSG");
    let d = MapDialog::new();
    let mut d = d;
    let rows: Vec<_> = FIRST_PAGE
        .iter()
        .map(|(id, name)| row(&[("OPT", ""), ("FCNID", id), ("FCNNAME", name)]))
        .collect();
    d.set_list(
        "FUNCTIONS",
        ListState {
            rows,
            top: 0,
            complete: false,
            ..ListState::default()
        },
    );
    compare_in(
        &d3(),
        "wrkfcnusg-1",
        &render(&g, "WRKFCNUSG", &d, &v71(&g)).unwrap(),
    );
}

#[test]
fn dspfcnusg_with_no_users() {
    let g = shipped("QUIFCNUSG");
    let mut d = MapDialog::new();
    d.set("FCNID", "QIBM_DB_SQLADM")
        .set("FCNNAME", "Database Administrator")
        .set("FCNDESC", "Database Administrator functions")
        .set("FCNPROD", "QIBM_BASE_OPERATING_SYSTEM")
        .set("FCNGRP", "QIBM_DB")
        .set("FCNDFT", "*DENIED")
        .set("FCNALLOBJ", "*NOTUSED");
    d.set_list(
        "USERS",
        ListState {
            rows: Vec::new(),
            top: 0,
            complete: true,
            total: Some(0),
        },
    );
    compare_in(
        &d3(),
        "dspfcnusg-sqladm",
        &render(&g, "DSPFCNUSG", &d, &v71(&g)).unwrap(),
    );
}

#[test]
fn wrkjrnrcv_with_none_found() {
    let g = shipped("QUIJRNRCV");
    let mut d = MapDialog::new();
    d.set_list(
        "RCVS",
        ListState {
            rows: Vec::new(),
            top: 0,
            complete: true,
            total: Some(0),
        },
    );
    // The empty list's sentence is ours, where IBM's is.
    compare_ours_in(
        &d3(),
        "wrkjrnrcv",
        &render(&g, "WRKJRNRCV", &d, &v71(&g)).unwrap(),
        &[(
            "(Cannot find object to match specified name.)",
            "(No journal receivers match the name)",
        )],
    );
}

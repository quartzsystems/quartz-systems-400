// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped system panels against the pub400 goldens: each panel
//! rendered with the golden's visible values and compared row by row
//! (`common::compare`). One test per golden, named after it.

mod common;

use common::{compare, shipped};
use uim::{ListState, MapDialog, Options, render};

/// A list of entries, every one given.
fn list(rows: Vec<Vec<(&str, &str)>>) -> ListState {
    let n = rows.len();
    ListState {
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
            .collect(),
        top: 0,
        total: Some(n),
        complete: true,
    }
}

/// A list of one-column lines.
fn lines(var: &str, rows: &[&str]) -> ListState {
    list(rows.iter().map(|r| vec![(var, *r)]).collect())
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

// --- messages and objects

#[test]
fn dspmsg() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("SYSNM", "SYSNAME")
        .set("MSGQ", "USERNAME")
        .set("MSGQLIB", "QUSRSYS")
        .set("DLVRY", "*BREAK");
    d.set_list("MSGS", lines("LINE", &["  (No messages available)"]));
    compare("dspmsg", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn wrkmsg() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("SYSNM", "SYSNAME")
        .set("MSGQ", "USERNAME")
        .set("MSGQLIB", "QUSRSYS")
        .set("DLVRY", "*BREAK");
    d.set_list("MSGS", lines("LINE", &["  (No messages available)"]));
    compare("wrkmsg", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn additional_message_information() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("FROMUSR", "USERNAME")
        .set("MSGSEV", "00")
        .set("MSGTYPE", "Request")
        .set("SNTDATE", "MM/DD/YY")
        .set("SNTTIME", "HH:MM:SS");
    d.set_list("INFO", lines("LINE", &["Message . . . . :   dspjoblog"]));
    compare(
        "additional-message-information",
        &render(&g, "ADDMSGI", &d, &opts()).unwrap(),
    );
}

#[test]
fn dsplibl() {
    let g = shipped("QUIDSPLIBL");
    let mut d = MapDialog::new();
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
    d.set_list(
        "LIBS",
        list(
            libs.iter()
                .map(|(n, t, x)| vec![("LIBNAME", *n), ("LIBTYPE", *t), ("LIBTEXT", *x)])
                .collect(),
        ),
    );
    compare("dsplibl", &render(&g, "DSPLIBL", &d, &opts()).unwrap());
}

#[test]
fn wrkobj() {
    let g = shipped("QUIWRKOBJ");
    let mut d = MapDialog::new();
    let objs = [
        ("SETUSER", "*PGM", "CLP", "set users to 4G"),
        ("EXCELAFBA", "*SQLPKG", "", ""),
        ("MSACCEAFBA", "*SQLPKG", "", ""),
        ("MSCS001", "*SQLPKG", "PACKAGE", ""),
        ("MSNC001", "*SQLPKG", "PACKAGE", ""),
        ("MSQRY3AFBA", "*SQLPKG", "", ""),
        ("MSRR001", "*SQLPKG", "PACKAGE", ""),
        ("MSRS001", "*SQLPKG", "PACKAGE", ""),
        ("MSUR001", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGC", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGN", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGS", "*SQLPKG", "PACKAGE", ""),
    ];
    let mut l = list(
        objs.iter()
            .map(|(n, t, a, x)| {
                vec![("OBJ", *n), ("OBJTYPE", *t), ("OBJLIB", "QGPL"), ("OBJATR", *a), ("OBJTEXT", *x)]
            })
            .collect(),
    );
    l.total = None;
    l.complete = false;
    d.set_list("OBJS", l);
    compare("wrkobj", &render(&g, "WRKOBJ", &d, &opts()).unwrap());
}

fn objd_header(d: &mut MapDialog) {
    d.set("LIBPOS", &format!("{:>28}", "Library 1 of 1"))
        .set("LIB", "QGPL")
        .set("OBJ", "QDFTJOBD")
        .set("OBJLIB", "QGPL")
        .set("OBJTYPE", "*JOBD")
        .set("OWNER", "QPGMR");
}

#[test]
fn dspobjd_basic() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_header(&mut d);
    let size = format!("{:<10}{:>18}", "", "8192");
    d.set_list(
        "OBJS",
        list(vec![vec![
            ("OBJ", "QDFTJOBD"),
            ("OBJTYPE", "*JOBD"),
            ("ATRSIZE", size.as_str()),
            ("OBJTEXT", "Default job description"),
        ]]),
    );
    compare("dspobjd-basic", &render(&g, "BASIC", &d, &opts()).unwrap());
}

fn objd_full(d: &mut MapDialog) {
    objd_header(d);
    d.set("OBJTEXT", "Default job description")
        .set("CRTDATE", "MM/DD/YY  HH:MM:SS")
        .set("CRTUSER", "*IBM")
        .set("CRTSYS", "00000000")
        .set("DOMAIN", "*SYSTEM")
        .set("CHGDATE", "MM/DD/YY  HH:MM:SS")
        .set("USAGE", "YES")
        .set("LASTUSED", "MM/DD/YY")
        .set("DAYSUSED", "14")
        .set("ALWCHG", "NO")
        .set("AUDIT", "*NOTAVL");
}

#[test]
fn dspobjd_full() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_full(&mut d);
    compare("dspobjd-full", &render(&g, "FULL", &d, &opts()).unwrap());
}

#[test]
fn dspobjd_full_2() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_full(&mut d);
    let o = Options {
        page: 1,
        ..opts()
    };
    compare("dspobjd-full-2", &render(&g, "FULL", &d, &o).unwrap());
}

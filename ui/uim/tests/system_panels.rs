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

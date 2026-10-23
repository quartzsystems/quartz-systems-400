// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debugger's displays against the pub400 goldens: Display Module
//! Source of a program without a source view, each key page, the
//! displays its keys and commands open, and its answers. Messages and
//! help text are Quartz's own, so a golden whose message line or help
//! body shows IBM's text is compared without those rows. Displays
//! pub400 could not show are checked for their title and keys only.

mod common;

use common::{compare, compare_rows};
use dbg::screen::lists::{self, BkptRow, Header, ModRow, ThreadRow};
use dbg::screen::source::{SourceScreen, dspmodsrc};
use dbg::screen::{Screen, eval, help, opm};

/// The golden's library is the account's own, masked but for its last
/// character.
const LIB: &str = "ABCDEFGH1";

fn nosource(program: &str, keys: usize, message: Option<&str>) -> Screen {
    dspmodsrc(&SourceScreen {
        program,
        library: LIB,
        module: program,
        view: None,
        breakpoints: &[],
        keys,
        cmdline: "",
        message,
    })
}

/// Every row but the debug line's two and the message line.
fn but_cmd_and_message() -> Vec<usize> {
    (1..=19).chain(22..=23).collect()
}

#[test]
fn dbg_dspmodsrc() {
    for g in [
        "dbg-dspmodsrc",
        "dbg-dspmodsrc-cmd",
        "dbg-cmd-bottom",
        "dbg-cmd-top",
        "dbg-cmd-equate",
        "dbg-cmd-display-module",
        "dbg-key-f9",
        "dbg-key-f11",
        "dbg-key-f16",
        "dbg-key-f17",
        "dbg-key-f20",
    ] {
        compare(g, &nosource("QSTESTPUB", 0, None));
    }
    compare("dbg-dspmodsrc-rpg", &nosource("QDFUZZ", 0, None));
}

#[test]
fn dbg_dspmodsrc_keys() {
    compare("dbg-dspmodsrc-f24", &nosource("QSTESTPUB", 1, None));
    compare("dbg-dspmodsrc-f24-2", &nosource("QSTESTPUB", 2, None));
    compare("dbg-dspmodsrc-f24-3", &nosource("QSTESTPUB", 3, None));
}

#[test]
fn dbg_dspmodsrc_answers() {
    let rows = but_cmd_and_message();
    compare_rows(
        "dbg-dspmodsrc-rpg-pgdn",
        &nosource("QDFUZZ", 0, Some("x")),
        &rows,
    );
    for g in [
        "dbg-f13-nosource",
        "dbg-key-f5",
        "dbg-key-f6",
        "dbg-key-f15",
        "dbg-key-f19",
        "dbg-key-pgup",
        "dbg-cmd-attr",
        "dbg-cmd-break",
        "dbg-cmd-clear",
        "dbg-cmd-clear-pgm",
        "dbg-cmd-display-bkp",
        "dbg-cmd-display-watch",
        "dbg-cmd-down",
        "dbg-cmd-eval",
        "dbg-cmd-find",
        "dbg-cmd-left",
        "dbg-cmd-next",
        "dbg-cmd-previous",
        "dbg-cmd-qual",
        "dbg-cmd-right",
        "dbg-cmd-set",
        "dbg-cmd-tbreak",
        "dbg-cmd-unknown",
        "dbg-cmd-up",
        "dbg-cmd-watch",
    ] {
        compare_rows(g, &nosource("QSTESTPUB", 0, Some("x")), &rows);
    }
}

#[test]
fn dbg_key_f14() {
    let rows = [
        ModRow {
            name: "QSTESTPUB".into(),
            library: LIB.into(),
            kind: "*PGM".into(),
            selected: false,
        },
        ModRow {
            name: "QSTESTPUB".into(),
            library: String::new(),
            kind: "*MODULE".into(),
            selected: true,
        },
    ];
    compare(
        "dbg-key-f14",
        &lists::wrkmodlst("PUB400", &rows, 0, "", None),
    );
}

#[test]
fn dbg_key_f18() {
    compare("dbg-key-f18", &lists::wrkwatch("PUB400", &[], 0, "", None));
}

#[test]
fn dbg_key_f21() {
    let under = nosource("QSTESTPUB", 0, None);
    compare("dbg-key-f21", &lists::command_window(&under, "", None));
}

#[test]
fn dbg_help() {
    let lines = help::lines();
    let title = "Display Module Source - Help";
    compare_rows(
        "dbg-key-help",
        &help::help(title, &lines, 0, true),
        &[1, 21, 22, 23],
    );
    compare_rows(
        "dbg-cmd-help",
        &help::help(title, &lines, 0, false),
        &[1, 21, 22, 23],
    );
    compare_rows(
        "dbg-cmd-help-f24",
        &help::help(title, &lines, 0, false),
        &[1, 21, 22, 23],
    );
}

#[test]
fn dbg_cmd_display_equate() {
    let mut h = Vec::new();
    for c in ["left", "right", "top"] {
        h.extend(eval::history_entry(c, ""));
    }
    let s = eval::evaluate(&h, 0, "", None);
    compare_rows(
        "dbg-cmd-display-equate",
        &s,
        &[1, 2, 3, 4, 19, 20, 21, 22, 23],
    );
}

#[test]
fn dbg_cmd_thread() {
    let s = lists::threads("00000000", "*NONE", &[], "", None);
    compare_rows("dbg-cmd-thread", &s, &(1..=23).collect::<Vec<_>>());
    compare_rows("dbg-cmd-thread-f24", &s, &(1..=23).collect::<Vec<_>>());
}

/// The displays pub400 does not show this account: title and keys.
#[test]
fn dbg_unverified_titles() {
    let h = Header {
        sysname: "S",
        program: "P",
        library: "QGPL",
        module: "P",
        kind: "*PGM",
    };
    let bk = lists::wrkmodbkp(
        &h,
        &[BkptRow {
            line: 3,
            condition: String::new(),
            thread: None,
        }],
        0,
        "",
        None,
    )
    .text_rows();
    assert!(bk[0].contains("Work with Module Breakpoints"));
    assert!(bk[21].contains("F3=Exit") && bk[21].contains("F12=Cancel"));
    let t = lists::threads(
        "00000001",
        "00000001",
        &[ThreadRow {
            id: "00000001".into(),
            status: "*ENABLED".into(),
        }],
        "",
        None,
    )
    .text_rows();
    assert!(t[9].contains("00000001"));
    assert!(eval::select_view("x", &[], None).text_rows()[0].contains("Select View"));
    assert!(opm::dspbkp(&[], 0, None).text_rows()[0].contains("Display Breakpoints"));
    assert!(
        opm::display_breakpoint(1, "P", "QGPL", "P", 1, None).text_rows()[0]
            .contains("Display Breakpoint")
    );
}

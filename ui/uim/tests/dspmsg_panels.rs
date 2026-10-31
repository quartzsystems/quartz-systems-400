// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUIDSPMSG` against the pub400 DSPMSG captures: Display Messages
//! (intermediate) and Work with Messages (basic), Additional Message
//! Information with the reply field and with the reply. Each panel is
//! rendered with the golden's values and compared row by row, its
//! input fields (row, column, length) with the golden's `.attr` grid.

mod objcommon;

use std::collections::BTreeSet;
use std::fmt::Write as _;

use objcommon::{compare, golden_dir, shipped};
use uim::{ListState, MapDialog, Options, Screen, render};

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

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

/// The input fields of a golden: an attribute byte (`#`) followed by
/// underlined cells, as (row, column, length), 1-based.
fn golden_fields(name: &str) -> BTreeSet<(usize, usize, usize)> {
    let path = golden_dir().join(format!("{name}.attr"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cells: Vec<char> = text
        .replace("\r\n", "\n")
        .lines()
        .flat_map(|l| format!("{l:<80}").chars().collect::<Vec<_>>())
        .collect();
    let input = |c: char| {
        let a = u32::from(c).checked_sub(u32::from('0')).map_or(0, |d| d + 0x20);
        (0x20..=0x3f).contains(&a) && a & 0x04 != 0 && a & 0x07 != 0x07
    };
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i + 1 < cells.len() {
        if cells[i] == '#' && input(cells[i + 1]) {
            let mut n = 0;
            while cells.get(i + 1 + n).copied().is_some_and(input) {
                n += 1;
            }
            out.insert(((i + 1) / 80 + 1, (i + 1) % 80 + 1, n));
            i += n + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The screen's input fields.
fn fields(s: &Screen) -> BTreeSet<(usize, usize, usize)> {
    s.fields
        .iter()
        .filter(|f| f.usage.is_input())
        .map(|f| (f.row, f.col, f.len))
        .collect()
}

/// Rows `first` to `last` (1-based) of golden `name`, each from column
/// `from` (0-based), blanks at the end dropped.
fn golden_rows(name: &str, first: usize, last: usize, from: usize) -> Vec<String> {
    let path = golden_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.lines()
        .skip(first - 1)
        .take(last + 1 - first)
        .map(|l| l.chars().skip(from).collect::<String>().trim_end().to_string())
        .collect()
}


/// Rows and fields against golden `name`.
fn check(name: &str, s: &Screen) {
    compare(name, s);
    assert_eq!(fields(s), golden_fields(name), "{name}: input fields");
}

/// The rows but the message line, and the fields, against `name`.
fn check_no_message(name: &str, s: &Screen) {
    let path = golden_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want: Vec<&str> = text.lines().collect();
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate().take(23) {
        let g = got.get(i).cloned().unwrap_or_default();
        let g = g.replace("12/31/26", "MM/DD/YY").replace("23:59:59", "HH:MM:SS");
        if g.trim_end() != w.trim_end() {
            let _ = writeln!(diff, "row {:>2}:\n  want |{}|\n  got  |{}|", i + 1, w.trim_end(), g.trim_end());
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
    assert_eq!(fields(s), golden_fields(name), "{name}: input fields");
}

/// The Display Messages header of a queue.
fn header(d: &mut MapDialog, queue: &str, lib: &str, sev: &str, dlvry: &str) {
    d.set("SYSNM", "SYSNAME")
        .set("MSGQ", queue)
        .set("MSGQLIB", lib)
        .set("SEV", sev)
        .set("DLVRY", dlvry)
        .set("KSET", "0");
}

/// Display Messages lines: `(text, takes a reply)`.
fn lines(rows: &[(&str, bool)]) -> ListState {
    list(
        rows.iter()
            .map(|(t, f)| vec![("LINE", *t), ("RPYOK", if *f { "Y" } else { "" }), ("RPY", "")])
            .collect(),
    )
}

const FROM: &str = "  From  . . . :   USERNAME       MM/DD/YY   HH:MM:SS";

#[test]
fn dspmsg_inquiry() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    header(&mut d, "USERNAME", "QUSRSYS", "00", "*BREAK");
    d.set_list(
        "MSGS",
        lines(&[(FROM, false), ("  Capture inquiry: answer Y or N.", false), ("    Reply . . .", true)]),
    );
    check("dspmsg-inquiry", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn dspmsg_usrprf() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    header(&mut d, "USERNAME", "QUSRSYS", "00", "*BREAK");
    d.set_list(
        "MSGS",
        lines(&[
            (FROM, false),
            ("  Capture inquiry two: answer Y or N.", false),
            ("    Reply . . :   Y", false),
        ]),
    );
    check("dspmsg-usrprf", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn dspmsg_wrkusr() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    header(&mut d, "QPADEV001T", "QSYS", "00", "*NOTIFY");
    d.set_list(
        "MSGS",
        lines(&[
            (FROM, false),
            ("  Capture inquiry: answer Y or N.", false),
            ("    Reply . . :   N", false),
        ]),
    );
    check("dspmsg-wrkusr", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn dspmsg_more_keys() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    header(&mut d, "QPADEV001T", "QSYS", "00", "*NOTIFY");
    d.set("KSET", "1");
    d.set_list(
        "MSGS",
        lines(&[(FROM, false), ("  Capture inquiry: answer Y or N.", false), ("    Reply . . :", false)]),
    );
    check("dspmsg-f24", &render(&g, "DSPMSG", &d, &opts()).unwrap());
    d.set("KSET", "0");
    check("dspmsg-wrkstn-copy", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn dspmsg_qsysopr() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    header(&mut d, "QSYSOPR", "QSYS", "99", "*HOLD");
    // The operator's messages are the system's words: they come from
    // the golden rather than from this file.
    let texts: Vec<String> = golden_rows("dspmsg-qsysopr", 8, 20, 1);
    let rows: Vec<(&str, bool)> = texts.iter().map(|t| (t.as_str(), false)).collect();
    d.set_list("MSGS", lines(&rows));
    check("dspmsg-qsysopr", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

/// Work with Messages rows: `(text, heading, first row of a message)`.
fn work(rows: &[(&str, &str, bool)]) -> ListState {
    list(
        rows.iter()
            .map(|(t, h, f)| vec![("WLINE", *t), ("WHEAD", *h), ("FIRST", if *f { "Y" } else { "" })])
            .collect(),
    )
}

const NEEDS: &str = "                     Messages needing a reply";
const REST: &str = "                   Messages not needing a reply";
const WFROM: &str = "  From  . . :   USERNAME       MM/DD/YY   HH:MM:SS";

#[test]
fn wrkmsg_basic() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("MSGQ", "USERNAME").set("KSET", "0");
    d.set_list(
        "WMSGS",
        work(&[
            ("", NEEDS, false),
            ("Capture inquiry: answer Y or N.", "", true),
            (WFROM, "", false),
            ("", "", false),
            ("", REST, false),
            ("Capture information message.", "", true),
            (WFROM, "", false),
        ]),
    );
    check("dspmsg-basic", &render(&g, "WRKMSG", &d, &opts()).unwrap());
    d.set("KSET", "1");
    check("dspmsg-basic-f24", &render(&g, "WRKMSG", &d, &opts()).unwrap());
}

#[test]
fn wrkmsg_basic_empty() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("MSGQ", "USERNAME").set("KSET", "0");
    d.set_list(
        "WMSGS",
        work(&[
            ("", NEEDS, false),
            ("(No messages available)", "", false),
            ("", "", false),
            ("", REST, false),
            ("(No messages available)", "", false),
        ]),
    );
    check("dspmsg-basic-empty", &render(&g, "WRKMSG", &d, &opts()).unwrap());
}

#[test]
fn wrkmsg_operator() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("MSGQ", "System Operator");
    // The inquiries' texts come from the golden, as above.
    let texts: Vec<String> = golden_rows("dspmsg-basic-f6", 10, 18, 7);
    let mut rows = vec![("", NEEDS, false)];
    rows.extend(texts.iter().map(|t| (t.as_str(), "", true)));
    rows.push(("", "", false));
    rows.push(("", REST, false));
    rows.push(("Job message queue wrapped.", "", true));
    d.set_list("WMSGS", work(&rows));
    check("dspmsg-basic-f6", &render(&g, "WRKOPR", &d, &opts()).unwrap());
}

/// Additional Message Information's header for the capture's inquiry.
fn info(d: &mut MapDialog) {
    d.set("FROMUSR", "USERNAME")
        .set("MSGSEV", "99")
        .set("MSGTYPE", "Inquiry")
        .set("SNTDATE", "MM/DD/YY")
        .set("SNTTIME", "HH:MM:SS");
    d.set_list(
        "INFO",
        list(vec![vec![("LINE", "Message . . . . :   Capture inquiry: answer Y or N.")]]),
    );
}

#[test]
fn addmsg_reply_field() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    info(&mut d);
    check("dspmsg-basic-5", &render(&g, "ADDMSGR", &d, &opts()).unwrap());
}

#[test]
fn addmsg_replied() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    info(&mut d);
    d.set("REPLY", "N");
    check_no_message("dspmsg-basic-replied", &render(&g, "ADDMSGA", &d, &opts()).unwrap());
}

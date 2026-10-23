// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system displays' panel groups (`work/qsjob/src/sysdsp/`): the
//! help of the System Request menu, Display Program Messages, Display
//! Work Station User and the About panel, against the pub400 goldens
//! where one exists. One test per golden, named after it.

mod common;

use common::{compare, golden_rows, shipped};
use uim::render::{MapDialog, Options, render};

fn sysname() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

/// The help is Quartz's text: only its geometry follows the golden
/// (`Help` and the centered title, headings at 2, text at 6 within
/// column 73, the indicator and the help keys).
#[test]
fn sysreq_help() {
    let g = shipped("QWSSYSRQ");
    let s = uim::render_help(&g, "HLPSYSRQ", &Options::default()).unwrap();
    let want = golden_rows("sysreq-help");
    let got = s.text_rows();
    assert_eq!(got[0].trim_end(), want[0].trim_end(), "the title row");
    let first = |r: &str| r.find(|c: char| c != ' ').map(|i| i + 1);
    let mut headings = 0;
    for (w, o) in want[2..20].iter().zip(&got[2..20]) {
        for row in [w, o] {
            assert!(row.trim_end().len() <= 73, "row passes column 73: {row}");
            assert!(matches!(first(row), None | Some(2 | 6)), "row starts oddly: {row}");
        }
        headings += usize::from(first(o) == Some(2));
    }
    assert!(headings > 0, "the help has a heading on its first page");
    for i in 20..23 {
        assert_eq!(got[i].trim_end(), want[i].trim_end(), "row {}", i + 1);
    }
}

/// A panel without a golden: its title centered on row 1 and its keys
/// on the key rows.
fn title_and_keys(group: &str, panel: &str, d: &MapDialog, title: &str, keys: &[&str]) -> Vec<String> {
    let g = shipped(group);
    let s = render(&g, panel, d, &sysname()).unwrap();
    let rows = s.text_rows();
    let col = 1 + (80 - title.len()) / 2;
    assert_eq!(rows[0].get(col - 1..col - 1 + title.len()), Some(title), "{panel} title");
    assert!(rows[1].contains("System:"), "{panel}: the system name row");
    let key_rows = format!("{}\n{}", rows[21], rows[22]);
    for k in keys {
        assert!(key_rows.contains(k), "{panel}: {k} not on the key rows:\n{key_rows}");
    }
    rows
}

#[test]
fn sysreq_7_wrkstnusr() {
    let g = shipped("QWCDSPWSU");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS")
        .set("USER", "USERNAME")
        .set("USRTEXT", "USERNAME")
        .set("WSNAME", "QPADEV000X")
        .set("WSTEXT", "Device created for PUB400.")
        .set("NBRJOBS", "1")
        .set("CURJOB", "A")
        .set("JOBA", "NNNNNN/USERNAME/QPADEV000X")
        .set("JOBB", "*NONE");
    let opts = Options {
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..sysname()
    };
    let s = render(&g, "DSPWSUSR", &d, &opts).unwrap();
    compare("sysreq-7-wrkstnusr", &s);
}

#[test]
fn tfrgrpjob() {
    let mut d = MapDialog::new();
    d.set("CURGRP", "GRP1");
    let rows = title_and_keys("QWCTFRGJ", "TFRGRPJOB", &d, "Transfer to Group Job", &["F3=Exit", "F12=Cancel"]);
    assert!(rows.iter().any(|r| r.contains("1=Select")));
    assert!(rows.iter().any(|r| r.contains("Group Job")));
}

#[test]
fn about() {
    let mut d = MapDialog::new();
    d.set("RELEASE", "V1R0M0").set("SERIAL", "10A1B2C").set("MODEL", "22A");
    let rows = title_and_keys("QUIABOUT", "ABOUT", &d, "About QS/400", &["F3=Exit", "F12=Cancel"]);
    let all = rows.join("\n");
    assert!(all.contains("V1R0M0") && all.contains("10A1B2C"));
}

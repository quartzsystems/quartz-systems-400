// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped system panels against the pub400 goldens: each panel
//! rendered with the golden's visible values and compared row by row
//! (`common::compare`). One test per golden, named after it.

mod common;

use common::{compare, shipped};
use uim::render::{ListState, MapDialog, Options, render};

// --- system and security (work/qsjob/src/panels/system.rs)

#[test]
fn dspsysval() {
    let g = shipped("QUISYSVAL");
    let mut d = MapDialog::new();
    d.set("SVNAME", "QMODEL")
        .set("SVTEXT", "System model number")
        .set("SVLABEL", "Model number")
        .set("SVVAL1", " 22A");
    let s = render(&g, "DSPSYSVAL", &d, &Options::default()).unwrap();
    compare("dspsysval", &s);
}

/// A basic user profile as pub400 shows its own.
fn usrprf_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("UPNAME", "USERNAME"),
        ("UPPRVSGN", "MM/DD/YY  HH:MM:SS"),
        ("UPSGNINV", "0"),
        ("UPSTATUS", "*ENABLED"),
        ("UPPWDCHG", "MM/DD/YY  HH:MM:SS"),
        ("UPPWDNONE", "*NO"),
        ("UPPWDITV", "*SYSVAL"),
        ("UPPWDEXPD", "MM/DD/YY"),
        ("UPPWDEXP", "*NO"),
        ("UPPWDBLK", "*SYSVAL"),
        ("UPLCLPWD", "*YES"),
        ("UPMAXSGN", "*SYSVAL"),
        ("UPUSRCLS", "*PGMR"),
        ("UPCRTDT", "MM/DD/YY  HH:MM:SS"),
        ("UPCRTBY", "JOBMANAGER"),
        ("UPCHGDT", "MM/DD/YY  HH:MM:SS"),
        ("UPLASTUSE", "MM/DD/YY"),
        ("UPRSTDT", "MM/DD/YY  HH:MM:SS"),
        ("UPEXPDT", "*NONE"),
        ("UPEXPITV", "*NONE"),
        ("UPEXPACN", "*NONE"),
        ("UPSPCAUT", "*NONE"),
        ("UPGRPPRF", "*NONE"),
        ("UPOWNER", "*USRPRF"),
        ("UPGRPAUT", "*NONE"),
        ("UPGRPTYP", "*PRIVATE"),
        ("UPSUPGRP", "*NONE"),
        ("UPASTLVL", "*INTERMED"),
        ("UPCURLIB", "USERNAME1"),
        ("UPINLPGM", "SETLANG"),
        ("UPINLPGML", "PUB400SYS"),
    ] {
        d.set(k, v);
    }
    d
}

#[test]
fn dspusrprf() {
    let g = shipped("QUIUSRPRF");
    let opts = Options {
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..Options::default()
    };
    let s = render(&g, "DSPUSRPRF", &usrprf_dialog(), &opts).unwrap();
    compare("dspusrprf", &s);
}

#[test]
fn dspusrprf_2() {
    let g = shipped("QUIUSRPRF");
    let opts = Options {
        page: 1,
        ..Options::default()
    };
    let s = render(&g, "DSPUSRPRF", &usrprf_dialog(), &opts).unwrap();
    compare("dspusrprf-2", &s);
}

/// pub400's network attributes, as far as the goldens show them.
fn neta_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("SYSNAME", "PUB400"),
        ("LCLNETID", "APPN"),
        ("LCLCPNAME", "C20AA83V"),
        ("LCLLOCNAME", "C20AA83V"),
        ("DFTMODE", "BLANK"),
        ("NODETYPE", "*ENDNODE"),
        ("DTACPR", "*NONE"),
        ("DTACPRINM", "*NONE"),
        ("MAXINTSSN", "200"),
        ("RAR", "128"),
        ("NETSRVID", "*LCLNETID"),
        ("NETSRVCP", "*ANY"),
        ("ALRSTS", "*OFF"),
        ("ALRLOGSTS", "*NONE"),
        ("ALRPRIFP", "*NO"),
        ("ALRDFTFP", "*NO"),
        ("ALRBCKNET", "*NONE"),
        ("ALRRQSNET", "*NONE"),
        ("ALRCTLD", "*NONE"),
        ("ALRHLDCNT", "0"),
        ("ALRFTR", "*NONE"),
        ("MSGQ", "QSYSOPR"),
        ("MSGQLIB", "QSYS"),
        ("OUTQ", "QPRINT"),
        ("OUTQLIB", "QGPL"),
        ("JOBACN", "*FILE"),
    ] {
        d.set(k, v);
    }
    d
}

fn sysname() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

#[test]
fn dspneta() {
    let g = shipped("QUIDSPNETA");
    let s = render(&g, "DSPNETA", &neta_dialog(), &sysname()).unwrap();
    compare("dspneta", &s);
}

#[test]
fn dspneta_2() {
    let g = shipped("QUIDSPNETA");
    let opts = Options {
        page: 1,
        ..sysname()
    };
    let s = render(&g, "DSPNETA", &neta_dialog(), &opts).unwrap();
    compare("dspneta-2", &s);
}

fn row(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn dspsfwrsc() {
    let g = shipped("QUISFWRSC");
    let mut rows = vec![row(&[
        ("RSCID", "5770999"),
        ("RSCOPT", "*BASE"),
        ("RSCFTR", "5050"),
        ("RSCDESC", "Licensed Internal Code"),
    ])];
    for f in [
        "5050", "2902", "2903", "2904", "2905", "2909", "2912", "2913", "2914", "2922", "2923",
        "2924", "2925", "2926",
    ] {
        rows.push(row(&[
            ("RSCID", "5770SS1"),
            ("RSCOPT", "*BASE"),
            ("RSCFTR", f),
            ("RSCDESC", "IBM i"),
        ]));
    }
    let mut d = MapDialog::new();
    d.set_list(
        "RESOURCES",
        ListState {
            total: Some(rows.len()),
            rows,
            top: 0,
            complete: true,
        },
    );
    let s = render(&g, "DSPSFWRSC", &d, &sysname()).unwrap();
    compare("dspsfwrsc", &s);
}

#[test]
#[ignore = "the golden keeps two key rows for one row of keys (row 22, row 23 blank)"]
fn dspautusr() {
    let g = shipped("QUIAUTUSR");
    let rows: Vec<_> = [
        ("USERNAME", "Wellman,Cody"),
        ("ENGMTZ", "Mohammed,Moataz"),
        ("SANDEP981", "Anonym,sandeep"),
        ("VENSUJA", "Prakash,Suja"),
    ]
    .iter()
    .map(|(u, t)| row(&[("AUUSER", u), ("AUPWDCHG", "MM/DD/YY"), ("AUTEXT", t)]))
    .collect();
    let mut d = MapDialog::new();
    d.set_list(
        "USERS",
        ListState {
            total: Some(rows.len()),
            rows,
            top: 0,
            complete: true,
        },
    );
    let opts = Options {
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..Options::default()
    };
    let s = render(&g, "DSPAUTUSR", &d, &opts).unwrap();
    compare("dspautusr", &s);
}

#[test]
fn information_assistant() {
    let g = shipped("QUIINFO");
    let opts = Options {
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..sysname()
    };
    let s = render(&g, "INFO", &MapDialog::new(), &opts).unwrap();
    compare("information-assistant", &s);
}

/// The main menu's help is Quartz's text: only its geometry follows the
/// golden (`Help` and the centered title, headings at column 2, text
/// at column 6 within column 73, the indicator and the help keys).
#[test]
fn main_menu_help() {
    let main = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../menus/sys/MAIN.pnlgrp");
    let g = common::group_from(&main);
    let s = uim::render_help(&g, "HLPMAIN", &Options::default()).unwrap();
    let want = common::golden_rows("main-menu-help");
    let got = s.text_rows();
    let first = |r: &str| r.find(|c: char| c != ' ').map(|i| i + 1);
    // Row 1: `Help` at column 2, the title centered.
    assert_eq!(got[0].get(1..5), Some("Help"));
    let title = got[0][5..].trim();
    assert_eq!(first(&got[0][5..]).map(|c| c + 5), Some(1 + (80 - title.len()) / 2));
    assert!(want[0].starts_with(" Help "));
    // Rows 3 to 20: text at column 6, headings at 2, nothing past 73.
    assert_eq!(first(&got[2]), Some(6));
    let mut headings = 0;
    for (w, o) in want[2..20].iter().zip(&got[2..20]) {
        for row in [w, o] {
            assert!(row.trim_end().len() <= 73, "row passes column 73: {row}");
            assert!(matches!(first(row), None | Some(2 | 6)), "row starts oddly: {row}");
        }
        headings += usize::from(first(o) == Some(2));
    }
    assert!(headings > 0, "the help has a heading on its first page");
    // The indicator and the keys, as the golden has them.
    for i in 20..23 {
        assert_eq!(got[i].trim_end(), want[i].trim_end(), "row {}", i + 1);
    }
}

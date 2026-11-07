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
    assert_eq!(
        first(&got[0][5..]).map(|c| c + 5),
        Some(1 + (80 - title.len()) / 2)
    );
    assert!(want[0].starts_with(" Help "));
    // Rows 3 to 20: text at column 6, headings at 2, nothing past 73.
    assert_eq!(first(&got[2]), Some(6));
    let mut headings = 0;
    for (w, o) in want[2..20].iter().zip(&got[2..20]) {
        for row in [w, o] {
            assert!(row.trim_end().len() <= 73, "row passes column 73: {row}");
            assert!(
                matches!(first(row), None | Some(2 | 6)),
                "row starts oddly: {row}"
            );
        }
        headings += usize::from(first(o) == Some(2));
    }
    assert!(headings > 0, "the help has a heading on its first page");
    // The indicator and the keys, as the golden has them.
    for i in 20..23 {
        assert_eq!(got[i].trim_end(), want[i].trim_end(), "row {}", i + 1);
    }
}

// --- the V7R1M0 captures (golden/verified/oracle71/panels)

/// The copyright line shown, the system name as the capture has it.
fn v71(group: &uim::PanelGroup, system: &str) -> Options {
    Options {
        system: system.into(),
        show_copyright: true,
        copyright: group.copyright.clone().unwrap_or_default(),
        ..Options::default()
    }
}

/// A list of entries, `complete` when the capture says `Bottom`.
fn entries(rows: Vec<Vec<(String, String)>>, complete: bool) -> ListState {
    ListState {
        total: complete.then_some(rows.len()),
        rows,
        top: 0,
        complete,
    }
}

#[test]
fn wrksysval() {
    let g = shipped("QUISYSVAL");
    let rows = [
        ("QABNORMSW", "*SYSCTL", "Previous end of system indicator"),
        ("QACGLVL", "*MSG", "Accounting level"),
        ("QACTJOB", "*ALC", "Initial number of active jobs"),
        ("QADLACTJ", "*ALC", "Additional number of active jobs"),
        (
            "QADLSPLA",
            "*ALC",
            "Spooling control block additional storage",
        ),
        ("QADLTOTJ", "*ALC", "Additional number of total jobs"),
        ("QALWJOBITP", "*SYSCTL", "Allow jobs to be interrupted"),
        ("QALWOBJRST", "*SEC", "Allow object restore option"),
    ]
    .iter()
    .map(|(n, t, d)| row(&[("SVNAME", n), ("SVGROUP", t), ("SVDESC", d)]))
    .collect();
    let mut d = MapDialog::new();
    d.set_list("SYSVALS", entries(rows, false));
    let o = Options {
        show_copyright: false,
        ..v71(&g, "SYSNAME")
    };
    common::compare71("wrksysval", &render(&g, "WRKSYSVAL", &d, &o).unwrap());
}

#[test]
fn wrkusrprf() {
    let g = shipped("QUIUSRPRF");
    let rows = [
        ("GRPA", ""),
        ("OWNA", ""),
        ("QANZAGENT", "Trace Analyzer Agent Server"),
        ("QAUTPROF", "IBM-supplied User Profile"),
        ("QBRMS", "IBM-supplied User Profile"),
        ("QCLUMGT", "IBM-supplied User Profile"),
        ("QCLUSTER", "IBM-supplied User Profile"),
        ("QCOLSRV", "IBM-supplied User Profile"),
        ("QDBSHR", "Internal Data Base User Profile"),
    ]
    .iter()
    .map(|(n, t)| row(&[("WUPNAME", n), ("WUPTEXT", t)]))
    .collect();
    let mut d = MapDialog::new();
    d.set_list("USERS", entries(rows, false));
    let o = Options {
        show_copyright: false,
        ..v71(&g, "SYSNAME")
    };
    common::compare71("wrkusrprf", &render(&g, "WRKUSRPRF", &d, &o).unwrap());
}

#[test]
fn wrkcfgsts() {
    let g = shipped("QUICFG");
    let job = |j: &str, u: &str| format!("{j:<12}{u:<12}NNNNNN");
    let rows = [
        ("DSP01", "VARY ON PENDING", String::new()),
        ("ETHLITCP", "ACTIVE", job("QTCPWRK", "QSYS")),
        ("OPTVRT01", "ACTIVE", String::new()),
        ("OPT01", "ACTIVE", String::new()),
        ("QCONSOLE", "VARIED OFF", String::new()),
        ("QESPAP", "VARIED OFF", String::new()),
        ("QIADSP", "VARIED OFF", String::new()),
        ("QIAPRT", "VARIED OFF", String::new()),
        ("QPADEV0001", "ACTIVE", job("QPADEV0001", "USERNAM")),
    ]
    .iter()
    .map(|(n, s, j)| {
        row(&[
            ("CFNAME", n),
            ("CFOBJ", n),
            ("CFSTS", s),
            ("CFJOB", j),
            ("CFTYPE", "*DEV"),
        ])
    })
    .collect();
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS");
    d.set_list("CFGS", entries(rows, false));
    let o = Options {
        show_copyright: false,
        ..v71(&g, "S062B175")
    };
    common::compare71("wrkcfgsts", &render(&g, "WRKCFGSTS", &d, &o).unwrap());
}

#[test]
fn dspdevd() {
    let g = shipped("QUICFG");
    let mut d = MapDialog::new();
    for (k, v) in [
        ("*DATETIME", "MM/DD/YY  HH:MM:SS"),
        ("DVNAME", "QPADEV0003"),
        ("DVCAT", "*DSP"),
        ("DVCLS", "*VRT"),
        ("DVTYPE", "V100"),
        ("DVMODEL", "*ASCII"),
        ("DVEMLDEV", "3196A2"),
        ("DVONLINE", "*NO"),
        ("DVCTL", "QPACTL01"),
        ("DVKBD", "USB"),
        ("DVPRTDEV", "*SYSVAL"),
        ("DVOUTQ", "*DEV"),
        ("DVPRTF", "QSYSPRT"),
        ("DVPRTFL", "*LIBL"),
        ("DVLOCNAM", "*NONE"),
    ] {
        d.set(k, v);
    }
    let o = Options {
        show_copyright: false,
        ..v71(&g, "S062B175")
    };
    common::compare71("dspdevd", &render(&g, "DSPDEVD", &d, &o).unwrap());
}

fn autl_users() -> ListState {
    entries(
        vec![
            row(&[("ALUSER", "*PUBLIC"), ("ALAUT", "*USE"), ("ALMGT", "")]),
            row(&[("ALUSER", "QSYS"), ("ALAUT", "*ALL"), ("ALMGT", " X")]),
        ],
        true,
    )
}

#[test]
fn dspautl() {
    let g = shipped("QUIAUTL");
    let mut d = MapDialog::new();
    d.set("ALNAME", "QPWFSERVER").set("ALOWNER", "QSYS");
    d.set_list("USERS", autl_users());
    common::compare71(
        "dspautl",
        &render(&g, "DSPAUTL", &d, &v71(&g, "SYSNAME")).unwrap(),
    );
}

#[test]
fn wrkautl() {
    let g = shipped("QUIAUTL");
    let rows = [
        ("QIWSADM", "IBM i Access Administrators"),
        ("QLWISVR", ""),
        ("QOPTSEC", "Default Optical Authorization List"),
        ("QPMCCDATA", ""),
        ("QPMCCFCN", ""),
        ("QPWFSERVER", ""),
        ("QSYLMTJAVA", ""),
        ("QZSRVHTTP", ""),
        ("SECORAL", ""),
    ]
    .iter()
    .map(|(n, t)| row(&[("ALNAME", n), ("ALTEXT", t)]))
    .collect();
    let mut d = MapDialog::new();
    d.set_list("LISTS", entries(rows, true));
    let o = Options {
        show_copyright: false,
        ..v71(&g, "SYSNAME")
    };
    common::compare71("wrkautl", &render(&g, "WRKAUTL", &d, &o).unwrap());
}

#[test]
fn dspsecaud() {
    let g = shipped("QUISECAUD");
    let mut d = MapDialog::new();
    d.set("SAEXIST", "NO")
        .set("SARCV", "*NONE")
        .set("SACTL", "*NONE")
        .set("SALVL", "*NONE")
        .set("SALVL2", "*NONE");
    common::compare71(
        "dspsecaud",
        &render(&g, "DSPSECAUD", &d, &v71(&g, "SYSNAME")).unwrap(),
    );
}

/// DSPAUTUSR after F11: the password levels (V7R1M0).
#[test]
fn dspautusr_f11() {
    let g = shipped("QUIAUTUSR");
    let users = [
        ("GRPA", "", "*NO", "*NO", "*NO"),
        ("OWNA", "GRPA", "*YES", "*YES", "*YES"),
        ("QANZAGENT", "", "*NO", "*NO", "*NO"),
        ("QAUTPROF", "", "*NO", "*NO", "*NO"),
        ("QBRMS", "", "*NO", "*NO", "*NO"),
        ("QCLUMGT", "", "*NO", "*NO", "*NO"),
        ("QCLUSTER", "", "*NO", "*NO", "*NO"),
        ("QCOLSRV", "", "*NO", "*NO", "*NO"),
        ("QDBSHR", "", "*NO", "*NO", "*NO"),
        ("QDBSHRDO", "", "*NO", "*NO", "*NO"),
        ("QDFTOWN", "", "*NO", "*NO", "*NO"),
        ("QDIRSRV", "", "*NO", "*NO", "*NO"),
        ("QDLFM", "", "*NO", "*NO", "*NO"),
        ("QDOC", "", "*NO", "*NO", "*NO"),
    ];
    let pad = |v: &str| format!("  {v}");
    let rows = users
        .iter()
        .map(|(u, gr, a, b, c)| {
            row(&[
                ("AUUSER", u),
                ("AUGROUP", gr),
                ("AUPWDCHG", "MM/DD/YY"),
                ("AULVL01", &pad(a)),
                ("AULVL23", &pad(b)),
                ("AUNETSVR", &pad(c)),
                ("AULCLPWD", "*YES"),
            ])
        })
        .collect();
    let mut d = MapDialog::new();
    d.set("AUVIEW", "2");
    d.set_list("USERS", entries(rows, false));
    let o = Options {
        view: 1,
        ..sysname()
    };
    common::compare71("dspautusr-f11", &render(&g, "DSPAUTUSR", &d, &o).unwrap());
}

/// DSPDEVD after F11: each value after its keyword (V7R1M0).
#[test]
fn dspdevd_f11() {
    let g = shipped("QUICFG");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS").set("DVVIEW", "2");
    for (k, kw, v) in [
        ("DVNAME", "DEVD", "QPADEV0003"),
        ("DVOPTION", "OPTION", "*BASIC"),
        ("DVCAT", "", "*DSP"),
        ("DVCLS", "DEVCLS", "*VRT"),
        ("DVTYPE", "TYPE", "V100"),
        ("DVMODEL", "MODEL", "*ASCII"),
        ("DVEMLDEV", "EMLDEV", "3196A2"),
        ("DVONLINE", "ONLINE", "*NO"),
        ("DVCTL", "CTL", "QPACTL01"),
        ("DVKBD", "KBDTYPE", "USB"),
        ("DVPRTDEV", "PRTDEV", "*SYSVAL"),
        ("DVOUTQ", "OUTQ", "*DEV"),
        ("DVPRTF", "PRTFILE", "QSYSPRT"),
        ("DVPRTFL", "", "*LIBL"),
        ("DVLOCNAM", "DEPLOCNAME", "*NONE"),
    ] {
        d.set(k, &format!("{kw:<12}{v}"));
    }
    let o = Options {
        show_copyright: false,
        ..v71(&g, "S062B175")
    };
    common::compare71("dspdevd-f11", &render(&g, "DSPDEVD", &d, &o).unwrap());
}

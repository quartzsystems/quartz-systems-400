// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications panels (`QUILIND`, `QUICFG`'s `WRKCFGSTS`)
//! against the pub400 captures of `tools/conformance/golden/phase10/
//! cmn`: each rendered with the capture's values, the way the job's
//! drivers fill them, and compared row by row.

mod common;

use std::path::{Path, PathBuf};

use common::{compare_in, shipped};
use uim::render::{ListState, MapDialog, Options, render};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase10/cmn")
}

fn opts(system: &str) -> Options {
    Options {
        system: system.into(),
        ..Options::default()
    }
}

fn list(rows: &[Vec<(&str, String)>]) -> ListState {
    let mut l = ListState {
        complete: true,
        ..ListState::default()
    };
    for r in rows {
        l.rows.push(
            r.iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        );
    }
    l.total = Some(l.rows.len());
    l
}

/// A value after its keyword, as the DSPLIND driver shows it.
fn kv(kwd: &str, value: &str) -> String {
    format!("{kwd:<12}{value}")
}

#[test]
fn wrklind() {
    let g = shipped("QUILIND");
    let mut d = MapDialog::new();
    d.set_list(
        "LINES",
        list(&[vec![
            ("LIND", "LINETH".into()),
            ("LINTYPE", "*ELAN".into()),
            ("LINTEXT", String::new()),
        ]]),
    );
    compare_in(
        &dir(),
        "wrklind",
        &render(&g, "WRKLIND", &d, &opts("SYSNAME")).unwrap(),
    );
}

fn dsplind_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC");
    for (var, kwd, value) in [
        ("DLIND", "LIND", "LINETH"),
        ("DOPTION", "OPTION", "*BASIC"),
        ("DCAT", "", "*ELAN"),
        ("DRSRC", "RSRCNAME", "CMN04"),
        ("DBRIDGE", "BRIDGE", "*NONE"),
        ("DONLINE", "ONLINE", "*YES"),
        ("DVRYWAIT", "VRYWAIT", "*NOWAIT"),
        ("DNETCTL", "NETCTL", "LINETNET02"),
        ("DADPTADR", "ADPTADR", "26A46507FE0A"),
        ("DEXCHID", "EXCHID", "056D610F"),
        ("DETHSTD", "ETHSTD", "*ETHV2"),
        ("DSPEED", "LINESPEED", "*AUTO"),
        ("DCURSPD", "", "*AUTO"),
        ("DDUPLEX", "DUPLEX", "*AUTO"),
        ("DCURDPX", "", "*AUTO"),
        ("DSRVOPT", "SRVOPT", "*NONE"),
        ("DMAXFRM", "MAXFRAME", "1496"),
        ("DMAXCTL", "MAXCTL", "40"),
        ("DTHRESH", "THRESHOLD", "*OFF"),
        ("DGENTST", "GENTSTFRM", "*YES"),
        ("DMSGQ", "MSGQ", "*SYSVAL"),
        ("DCURMSGQ", "", "QSYSOPR"),
        ("DCURMSGL", "", "QSYS"),
        ("DTEXT", "TEXT", "*BLANK"),
    ] {
        d.set(var, &kv(kwd, value));
    }
    d
}

#[test]
fn dsplind() {
    let g = shipped("QUILIND");
    let s = render(&g, "DSPLIND", &dsplind_dialog(), &opts("PUB400")).unwrap();
    compare_in(&dir(), "dsplind", &s);
}

#[test]
fn dsplind_page_2() {
    let g = shipped("QUILIND");
    let o = Options {
        page: 1,
        ..opts("PUB400")
    };
    let s = render(&g, "DSPLIND", &dsplind_dialog(), &o).unwrap();
    compare_in(&dir(), "dsplind-2", &s);
}

/// A `WRKCFGSTS` entry as the driver makes it: the name indented two
/// columns a level, the job as name, user and number.
fn cfg(
    depth: usize,
    name: &str,
    status: &str,
    ty: &str,
    job: Option<(&str, &str, &str)>,
) -> Vec<(&'static str, String)> {
    vec![
        ("CFNAME", format!("{}{name}", "  ".repeat(depth))),
        ("CFOBJ", name.to_string()),
        ("CFSTS", status.to_string()),
        (
            "CFJOB",
            job.map(|(j, u, n)| format!("{j:<12}{u:<12}{n}"))
                .unwrap_or_default(),
        ),
        ("CFTYPE", ty.to_string()),
    ]
}

fn lin_view(view: usize, golden: &str) {
    let g = shipped("QUICFG");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC");
    d.set_list(
        "CFGS",
        list(&[
            cfg(0, "LINETH", "ACTIVE", "*LIN", None),
            cfg(1, "LINETNET02", "ACTIVE", "*CTL", None),
            cfg(
                2,
                "LINETTCP00",
                "ACTIVE",
                "*DEV",
                Some(("QTCPWRK", "QSYS", "090579")),
            ),
        ]),
    );
    let o = Options {
        view,
        ..opts("PUB400")
    };
    compare_in(&dir(), golden, &render(&g, "WRKCFGSTS", &d, &o).unwrap());
}

#[test]
fn wrkcfgsts_lin() {
    lin_view(0, "wrkcfgsts-lin");
}

#[test]
fn wrkcfgsts_lin_f11() {
    lin_view(1, "wrkcfgsts-lin-f11");
}

#[test]
fn wrkcfgsts_ctl() {
    let g = shipped("QUICFG");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC");
    let tcp = Some(("QTCPWRK", "QSYS", "090579"));
    let mut l = list(&[
        cfg(0, "CTL01", "ACTIVE", "*CTL", None),
        cfg(1, "DSP01", "SIGNON DISPLAY", "*DEV", None),
        cfg(0, "LINETNET", "VARIED OFF", "*CTL", None),
        cfg(0, "LINETNET00", "VARIED OFF", "*CTL", None),
        cfg(1, "LINETTCP", "VARIED OFF", "*DEV", None),
        cfg(0, "LINETNET01", "VARIED OFF", "*CTL", None),
        cfg(0, "LINETNET02", "ACTIVE", "*CTL", None),
        cfg(1, "LINETTCP00", "ACTIVE", "*DEV", tcp),
        cfg(0, "QCTL", "VARIED OFF", "*CTL", None),
        cfg(0, "QPACTL01", "ACTIVE", "*CTL", None),
    ]);
    l.total = None;
    l.complete = false;
    d.set_list("CFGS", l);
    compare_in(
        &dir(),
        "wrkcfgsts-ctl",
        &render(&g, "WRKCFGSTS", &d, &opts("PUB400")).unwrap(),
    );
}

#[test]
fn wrkcfgsts_dev() {
    let g = shipped("QUICFG");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC");
    let st = [
        "SIGNON DISPLAY",
        "VARIED OFF",
        "SIGNON DISPLAY",
        "SIGNON DISPLAY",
        "VARIED OFF",
        "VARY ON PENDING",
        "VARY ON PENDING",
        "SIGNON DISPLAY",
        "VARIED OFF",
        "VARIED OFF",
    ];
    let rows: Vec<_> = st
        .iter()
        .map(|s| cfg(0, "QPADEVNNNN", s, "*DEV", None))
        .collect();
    let mut l = list(&rows);
    l.total = None;
    l.complete = false;
    d.set_list("CFGS", l);
    compare_in(&dir(), "wrkcfgsts-dev", &render(&g, "WRKCFGSTS", &d, &opts("PUB400")).unwrap());
}

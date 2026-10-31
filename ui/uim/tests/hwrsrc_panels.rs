// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUIHDWRSC` (WRKHDWRSC, DSPHDWRSC) against the pub400 goldens of
//! `golden/phase10/cmn` and `golden/phase11/hwrsrc`: each panel rendered
//! with the golden's visible values and compared row by row. An empty
//! list's text is Quartz's own, so that row is compared for its place
//! only.

mod common;

use std::path::{Path, PathBuf};

use common::{compare_in, golden_rows_in, shipped};
use uim::render::{ListState, MapDialog, Options, render};

fn dir(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden").join(sub)
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        show_copyright: false,
        ..Options::default()
    }
}

/// A complete list from rows of `(variable, value)` pairs.
fn list(rows: &[Vec<(&str, String)>], top: usize) -> ListState {
    let mut l = ListState {
        complete: true,
        top,
        ..ListState::default()
    };
    for r in rows {
        l.rows.push(r.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect());
    }
    l.total = Some(top + l.rows.len());
    l
}

/// The `RSCS` list of the type-column panels.
fn type_rows(rows: &[(&str, &str, &str, &str)]) -> ListState {
    let r: Vec<Vec<(&str, String)>> = rows
        .iter()
        .map(|(n, t, s, x)| {
            vec![
                ("RSCNAME", (*n).to_string()),
                ("RSCTYPE", (*t).to_string()),
                ("RSCSTS", (*s).to_string()),
                ("RSCTEXT", (*x).to_string()),
            ]
        })
        .collect();
    list(&r, 0)
}

/// The `RSCTMS` list of the type-model panels.
fn tm_rows(rows: &[(&str, &str, &str, &str)]) -> ListState {
    let r: Vec<Vec<(&str, String)>> = rows
        .iter()
        .map(|(n, t, s, x)| {
            vec![
                ("RSCNAME", (*n).to_string()),
                ("RSCTM", format!(" {t}")),
                ("RSCSTS", (*s).to_string()),
                ("RSCTXT22", (*x).to_string()),
            ]
        })
        .collect();
    list(&r, 0)
}

fn kind(word: &str) -> MapDialog {
    let mut d = MapDialog::new();
    d.set("RSCKIND", word);
    d
}

/// Every row but `skip` against the golden; `skip` only for where its
/// text starts.
fn compare_but(sub: &str, golden: &str, s: &uim::Screen, skip: usize) {
    let want = golden_rows_in(&dir(sub), golden);
    let got = s.text_rows();
    for (i, w) in want.iter().enumerate() {
        let ours = got.get(i).map_or("", String::as_str);
        if i + 1 == skip {
            let at = |t: &str| t.find(|c: char| c != ' ');
            assert_eq!(at(w), at(ours), "{golden} row {skip}: {ours}");
            continue;
        }
        assert_eq!(w.trim_end(), ours.trim_end(), "{golden} row {}", i + 1);
    }
}

#[test]
fn wrkhdwrsc_cmn() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Communication");
    d.set_list(
        "RSCS",
        type_rows(&[
            ("CMB09", "6B03", "Operational", "Comm Processor"),
            ("  LIN03", "6B03", "Operational", "Comm Adapter"),
            ("    CMN01", "6B03", "Operational", "Comm Port"),
            ("CMB10", "6B03", "Operational", "Comm Processor"),
            ("  LIN01", "6B03", "Operational", "Comm Adapter"),
            ("    CMN02", "6B03", "Operational", "Comm Port"),
            ("CMB11", "268C", "Not detected", "Comm Processor"),
            ("  LIN02", "268C", "Not detected", "LAN Adapter"),
            ("    CMN03", "268C", "Not detected", "Ethernet Port"),
            ("CMB12", "268C", "Operational", "Combined function IOP"),
            ("  LIN04", "6B26", "Operational", "Comm Adapter"),
            ("CMB17", "268C", "Operational", "Comm Processor"),
            ("  LIN05", "268C", "Operational", "LAN Adapter"),
            ("    CMN04", "268C", "Operational", "Ethernet Port"),
        ]),
    );
    compare_in(&dir("phase10/cmn"), "wrkhdwrsc-cmn", &render(&g, "HWCMN", &d, &opts()).unwrap());
}

#[test]
fn wrkhdwrsc_stg() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Storage");
    d.set_list(
        "RSCTMS",
        tm_rows(&[
            ("CMB01", "290A-001", "Not detected", "Storage Controller"),
            ("  DC01", "290A-001", "Not detected", "Storage Controller"),
            ("CMB02", "290A-001", "Not detected", "Storage Controller"),
            ("  DC02", "290A-001", "Not detected", "Storage Controller"),
            ("CMB03", "298A-001", "Operational", "Storage Controller"),
            ("  DC03", "298A-001", "Operational", "Storage Controller"),
            ("CMB04", "6B25-001", "Not detected", "Storage Controller"),
            ("  DC04", "6B25-001", "Not detected", "Storage Controller"),
            ("  TAPMLB01", "3573-020", "Not detected", "Tape Library"),
            ("CMB05", "6B25-001", "Operational", "Storage Controller"),
            ("  DC05", "6B25-001", "Operational", "Storage Controller"),
            ("CMB06", "6B25-001", "Operational", "Storage Controller"),
            ("  DC06", "6B25-001", "Operational", "Storage Controller"),
            ("  TAPMLB02", "3573-020", "Not detected", "Tape Library"),
            ("  TAPMLB03", "3573-020", "Not detected", "Tape Library"),
            ("CMB07", "6B25-001", "Operational", "Storage Controller"),
        ]),
    );
    compare_in(&dir("phase11/hwrsrc"), "wrkhdwrsc-stg", &render(&g, "HWSTG", &d, &opts()).unwrap());
}

#[test]
fn wrkhdwrsc_prc() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Processor");
    let mut rows = vec![
        ("CEC01", "9824-22A".to_string(), "Operational", "Main Card Enclosure"),
        ("PN01", "6B5D".into(), "Not detected", "System Control Panel"),
        ("PN02", "6B5C".into(), "Not detected", "System Control Panel"),
        ("PN03", "6B86".into(), "Operational", "System Control Panel"),
        ("PN04", "6C85".into(), "Operational", "System Control Panel"),
    ];
    for i in 1..=8 {
        rows.push(("MP0X", "5C3F".into(), "Not detected", "System Processor Card"));
        let _ = i;
    }
    let names = ["MP01", "MP02", "MP03", "MP04", "MP05", "MP06", "MP07", "MP08"];
    for (r, n) in rows.iter_mut().skip(5).zip(names) {
        r.0 = n;
    }
    rows.push(("MP09", "5CD1".into(), "Operational", "System Processor Card"));
    rows.push(("MP10", "5CD1".into(), "Operational", "System Processor Card"));
    rows.push(("MP11", "5CD1".into(), "Operational", "System Processor Card"));
    let r: Vec<(&str, &str, &str, &str)> = rows.iter().map(|(a, b, c, e)| (*a, b.as_str(), *c, *e)).collect();
    d.set_list("RSCTMS", tm_rows(&r));
    compare_in(&dir("phase11/hwrsrc"), "wrkhdwrsc-prc", &render(&g, "HWPRC", &d, &opts()).unwrap());
}

#[test]
fn wrkhdwrsc_lws() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Local Work Station");
    d.set_list(
        "RSCS",
        type_rows(&[
            ("CMB12", "268C", "Operational", "Combined function IOP"),
            ("  CTL01", "6A59", "Operational", "Work Station Controller"),
        ]),
    );
    compare_in(&dir("phase11/hwrsrc"), "wrkhdwrsc-lws", &render(&g, "HWLWS", &d, &opts()).unwrap());
}

#[test]
fn wrkhdwrsc_crp_and_csa_empty() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Cryptographic");
    d.set_list("RSCS", type_rows(&[]));
    compare_but("phase11/hwrsrc", "wrkhdwrsc-crp", &render(&g, "HWCRP", &d, &opts()).unwrap(), 8);
    let mut d = kind("Coupled");
    d.set_list("RSCCSA", list(&[], 0));
    compare_but("phase11/hwrsrc", "wrkhdwrsc-csa", &render(&g, "HWCSA", &d, &opts()).unwrap(), 8);
}

/// The detail of the oracle's `CMB09`: the location, then the logical
/// address lines in the scrolled area.
fn detail(name: &str, text: &str, tm: &str, loc: &str, lines: &[String]) -> MapDialog {
    let mut d = MapDialog::new();
    d.set("DRSC", name);
    d.set("DTEXT", text);
    d.set("DTM", tm);
    d.set("DSERIAL", "00-00000");
    d.set("DPART", "");
    d.set("DLOC", loc);
    let r: Vec<Vec<(&str, String)>> = lines.iter().map(|l| vec![("DLINE", l.clone())]).collect();
    d.set_list("DLINES", list(&r, 0));
    d
}

fn spd() -> Vec<String> {
    let item = |l: &str, v: &str| format!("  {l:<33}{v}");
    vec![
        String::new(),
        "Logical address:".into(),
        " SPD bus:".into(),
        item("System bus", "255"),
        item("System board", "128"),
        item("System card", "1"),
    ]
}

#[test]
fn detail_pages() {
    let g = shipped("QUIHDWRSC");
    let d = detail("CMB09", "Comm Processor", "6B03-001", "U9824.22A.78EA841-V22-C1", &spd());
    compare_in(&dir("phase10/cmn"), "wrkhdwrsc-cmn-7", &render(&g, "DETAIL", &d, &opts()).unwrap());
    let mut d2 = detail("CMB09", "Comm Processor", "6B03-001", "U9824.22A.78EA841-V22-C1", &[]);
    let rest: Vec<Vec<(&str, String)>> = spd()[5..].iter().map(|l| vec![("DLINE", l.clone())]).collect();
    d2.set_list("DLINES", list(&rest, 5));
    compare_in(&dir("phase10/cmn"), "wrkhdwrsc-cmn-7-2", &render(&g, "DETAIL", &d2, &opts()).unwrap());
    let d = detail("CMB01", "Storage Controller", "290A-001", "U9824.22A.78EA841-V22-C4", &spd());
    compare_in(&dir("phase11/hwrsrc"), "wrkhdwrsc-stg-7", &render(&g, "DETAIL", &d, &opts()).unwrap());
}

#[test]
fn configuration_descriptions() {
    let g = shipped("QUIHDWRSC");
    let mut d = MapDialog::new();
    d.set("DRSC", "CMN04");
    d.set("DTEXT", "Ethernet Port");
    let r = vec![
        vec![("CFGD", String::new())],
        vec![("CFGD", "LINETH".to_string())],
    ];
    d.set_list("CFGDS", list(&r, 0));
    compare_in(&dir("phase10/cmn"), "wrkhdwrsc-cmn04-5", &render(&g, "CFGD", &d, &opts()).unwrap());
}

/// DSPHDWRSC has no golden (not authorized to the pub400 account): its
/// panels are the lists' without options.
#[test]
fn dsphdwrsc_has_the_list_without_options() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Hardware");
    d.set_list("RSCTMS", tm_rows(&[("CMB01", "QB01-001", "Operational", "PCI Bridge")]));
    let s = render(&g, "DSPTM", &d, &opts()).unwrap();
    let rows = s.text_rows();
    assert!(rows[0].contains("Display Hardware Resources"));
    assert!(!rows.iter().any(|r| r.contains("Type options")));
    assert!(rows.iter().any(|r| r.contains("CMB01            QB01-001   Operational")), "{rows:#?}");
    assert!(rows[22].contains("F3=Exit") && rows[22].contains("F12=Cancel"));
}

#[test]
fn work_with_a_storage_controller() {
    let g = shipped("QUIHDWRSC");
    let mut d = kind("Storage Controller");
    d.set_list("RSCTMS", tm_rows(&[("DC03", "298A-001", "Operational", "Storage Controller")]));
    compare_in(&dir("phase11/hwrsrc"), "wrkhdwrsc-stg-dc03-9", &render(&g, "HWSUB", &d, &opts()).unwrap());
}

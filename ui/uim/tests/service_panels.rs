// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service panels (`QGSXPNLS`: the problem log and `ANZPRB`;
//! `QESCNTINF`: contact information, service providers, service
//! status) against goldens of our own rendering under
//! `tests/golden/service/` (`QS400_UPDATE_GOLDEN=1` rewrites them).
//! pub400 refuses these commands, so there is no IBM capture.

use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name}:\n{}", c.listing);
    c.group.expect("a group")
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/service")
}

fn list(rows: &[&[(&str, &str)]]) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|r| r.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect())
            .collect(),
        top: 0,
        total: Some(rows.len()),
        complete: true,
    }
}

/// Render, check the frame, compare with (or write) the golden.
fn golden(grp: &str, panel: &str, d: &MapDialog) -> Vec<String> {
    let opts = Options {
        system: "QS400".into(),
        ..Options::default()
    };
    let s = render(&group(grp), panel, d, &opts).unwrap();
    let rows = s.text_rows();
    assert_eq!(rows.len(), 24);
    for r in &rows {
        assert!(r.chars().count() <= 80, "{r:?}");
    }
    let text = rows.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n") + "\n";
    let path = dir().join(format!("{panel}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    assert_eq!(text, want, "{panel} differs from {}", path.display());
    rows
}

fn has(rows: &[String], text: &str) {
    assert!(rows.iter().any(|r| r.contains(text)), "no {text:?} in\n{}", rows.join("\n"));
}

fn problem(d: &mut MapDialog) {
    d.set("PRBID", "2630300001")
        .set("PRBSTS", "READY")
        .set("PRBTYPE", "1")
        .set("PRBSEV", "2")
        .set("PRBTEXT", "Dump of QGPL/ORDERS *FILE requested")
        .set("PRBREF", "DUMP")
        .set("PRBRSRC", "*NONE")
        .set("PRBSYS", "QS400")
        .set("PRBODATE", "10/30/26")
        .set("PRBOTIME", "14:02:11")
        .set("PRBCDATE", "10/30/26")
        .set("PRBCTIME", "14:05:40")
        .set("PRBASGN", "*NONE")
        .set("PRBRPT", "*NO")
        .set("PRBSYMP", "DUMP/QGPL/ORDERS *FILE");
}

#[test]
fn wrkprb_lists_with_every_option() {
    let mut d = MapDialog::new();
    d.set_list(
        "PRBS",
        list(&[
            &[("PRBID", "2630300002"), ("PRBSTS", "OPENED"), ("PRBTEXT", "Damaged object found")],
            &[("PRBID", "2630300001"), ("PRBSTS", "READY"), ("PRBTEXT", "Dump of QGPL/ORDERS *FILE requested")],
        ]),
    );
    let rows = golden("QGSXPNLS", "WRKPRB", &d);
    has(&rows, "Work with Problems");
    has(&rows, "8=Work with problem");
    has(&rows, "12=Enter text");
    has(&rows, "F16=Report prepared problems");
}

#[test]
fn problem_details_work_with_and_text() {
    let mut d = MapDialog::new();
    problem(&mut d);
    d.set_list("NOTES", list(&[&[("NOTE", "occurred again at 1793376131")]]));
    let rows = golden("QGSXPNLS", "DSPPRB", &d);
    has(&rows, "Display Problem Details");
    has(&rows, "DUMP/QGPL/ORDERS");
    let rows = golden("QGSXPNLS", "WRKPRBW", &d);
    has(&rows, "2. Report problem");
    let rows = golden("QGSXPNLS", "PRBNOTE", &d);
    has(&rows, "Enter Text About the Problem");
}

#[test]
fn guided_analysis() {
    let mut d = MapDialog::new();
    let rows = golden("QGSXPNLS", "ANZPRB1", &d);
    has(&rows, "Analyze a New Problem");
    has(&rows, "5. Performance");
    d.set("ANZSTOP", "N");
    golden("QGSXPNLS", "ANZPRB2", &d);
    problem(&mut d);
    let rows = golden("QGSXPNLS", "ANZPRB3", &d);
    has(&rows, "Problem Analysis Complete");
}

#[test]
fn contact_providers_and_status() {
    let mut d = MapDialog::new();
    let rows = golden("QESCNTINF", "WRKCNTINF", &d);
    has(&rows, "Work with Support Contact Information");
    d.set("CNT01", "Quartz Systems").set("CNT02", "Cody").set("CNT11", "2924").set("CNT12", "*AUTOMATIC");
    golden("QESCNTINF", "DSPCNTINF", &d);
    d.set_list(
        "PVDS",
        list(&[&[("CPNAME", "QSERVICE"), ("NETID", "*NETATR"), ("CNNTYPE", "*NONE"), ("PVDTEXT", "Quartz Systems service")]]),
    );
    let rows = golden("QESCNTINF", "WRKSRVPVD", &d);
    has(&rows, "9=Send test request");
    d.set("SRV01", "000123/QSECOFR/QPADEV0001")
        .set("SRV02", "*NONE")
        .set("SRV03", "*YES")
        .set("SRV04", "T1");
    golden("QESCNTINF", "DSPSRVSTS", &d);
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The GO LICPGM panels (`QSZLICPGM`) against goldens of our own
//! rendering under `tests/golden/licpgm/` (`QS400_UPDATE_GOLDEN=1`
//! rewrites them). The pub400 captures for the IBM comparison are
//! `tools/conformance/golden/phase11/`, taken separately.

use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QSZLICPGM").expect("QSZLICPGM is shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QSZLICPGM:\n{}", c.listing);
    c.group.expect("a group")
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/licpgm")
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
fn golden(panel: &str, d: &MapDialog) -> Vec<String> {
    let opts = Options {
        system: "QS400".into(),
        ..Options::default()
    };
    let s = render(&group(), panel, d, &opts).unwrap();
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

fn installed() -> ListState {
    list(&[
        &[("LPID", "QS00SS1"), ("LPOPT", "*BASE"), ("STATUS", "*COMPATIBLE"), ("LPTEXT", "Quartz Systems/400 Operating System"), ("LPRLS", "V1R0M0")],
        &[("LPID", "QS00SS1"), ("LPOPT", "30"), ("STATUS", "*COMPATIBLE"), ("LPTEXT", "Qshell"), ("LPRLS", "V1R0M0")],
        &[("LPID", "QS00WDS"), ("LPOPT", "31"), ("STATUS", "*ERROR"), ("LPTEXT", "ILE RPG compiler"), ("LPRLS", "V1R0M0")],
    ])
}

fn has(rows: &[String], text: &str) {
    assert!(rows.iter().any(|r| r.contains(text)), "no {text:?} in\n{}", rows.join("\n"));
}

#[test]
fn installed_programs_and_their_releases() {
    let mut d = MapDialog::new();
    d.set_list("INSTALLED", installed());
    let rows = golden("LICPGM10", &d);
    has(&rows, "Display Installed Licensed Programs");
    has(&rows, "*COMPATIBLE");
    has(&rows, "F11=Display release");
    let rows = golden("LICPGM10R", &d);
    has(&rows, "Release");
    has(&rows, "V1R0M0");
}

#[test]
fn install_delete_and_save_lists() {
    let mut d = MapDialog::new();
    d.set_list("PROGRAMS", installed());
    has(&golden("LICPGM11", &d), "1=Install");
    has(&golden("LICPGM12", &d), "4=Delete");
    has(&golden("LICPGM13", &d), "1=Save");
}

#[test]
fn install_options_and_the_agreement() {
    let mut d = MapDialog::new();
    d.set("INSDEV", "QGPL/WDSLOAD");
    d.set("INSOBJ", "1");
    d.set("INSIPL", "N");
    let rows = golden("LICINSOPT", &d);
    has(&rows, "Installation device");
    let mut d = MapDialog::new();
    d.set("LPID", "QS00WDS");
    d.set("LPRLS", "V1R0M0");
    d.set("LPTEXT", "Quartz Systems/400 Application Development");
    let text = licpgm_text();
    let lines: Vec<Vec<(String, String)>> = text.lines().map(|l| vec![("AGRLINE".to_string(), l.to_string())]).collect();
    d.set_list(
        "AGRTEXT",
        ListState {
            total: Some(lines.len()),
            rows: lines,
            top: 0,
            complete: true,
        },
    );
    let rows = golden("LICAGREE", &d);
    has(&rows, "F14=Accept");
    has(&rows, "F15=Decline");
    has(&rows, "MIT license");
}

fn licpgm_text() -> &'static str {
    "Quartz Systems/400 Application Development - Software Agreement\n\n1. This software is licensed under the MIT license. You may use, copy,\n   change and pass it on, as that license says."
}

#[test]
fn languages_agreements_and_licenses() {
    let mut d = MapDialog::new();
    d.set("PRILNG", "2924");
    d.set("PRILNGT", "English");
    d.set_list("LANGS", list(&[]));
    has(&golden("LICLNG20", &d), "No secondary languages are installed");
    golden("LICLNG21", &d);
    golden("LICLNG22", &d);
    let mut d = MapDialog::new();
    d.set_list(
        "AGRS",
        list(&[&[("LPID", "QS00SS1"), ("LPRLS", "V1R0M0"), ("AGRSTS", "*ACCEPTED"), ("LPTEXT", "Quartz Systems/400 Operating System")]]),
    );
    has(&golden("WRKSFWAGR", &d), "5=Display");
    let mut d = MapDialog::new();
    d.set_list(
        "LICS",
        list(&[&[
            ("LPID", "QS00WDS"),
            ("LICTRM", "V1"),
            ("LICFTR", "5102"),
            ("LICCMP", "*WARNING"),
            ("LICLMT", "*NOMAX"),
            ("LICCNT", "0"),
        ]]),
    );
    has(&golden("WRKLICINF", &d), "Compliance");
}

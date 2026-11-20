// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSI server's displays (`QSQSICSL`, docs/QSID.md section 12):
//! STRQSIPAIR's password and WRKQSICSL's consoles. IBM i has none, so
//! they are against goldens of our own rendering under
//! `tests/golden/qsi/` (`QS400_UPDATE_GOLDEN=1` rewrites them).

use std::path::Path;

use uim::render::{ListState, MapDialog, Options, Screen, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QSQSICSL").expect("shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QSQSICSL:\n{}", c.listing);
    c.group.expect("a group")
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

fn ours(name: &str, s: &Screen) {
    let rows = s.text_rows();
    assert_eq!(rows.len(), 24, "{name}: 24 rows");
    let text = rows
        .iter()
        .map(|r| r.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/qsi/{name}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_GOLDEN=1 writes it)", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(text, want, "{name} differs from its golden");
}

const FP: &str = "3f1c9a0b7e2d4c6a8b0e1f2a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e";

fn fingerprint(d: &mut MapDialog) {
    d.set("FPRINT1", &FP[..32]);
    d.set("FPRINT2", &FP[32..]);
}

#[test]
fn the_pairing_password() {
    let g = group();
    let mut d = MapDialog::new();
    fingerprint(&mut d);
    d.set("PASSWORD", "7K3Q-9VNM-2HXD-PW4T");
    d.set("UNTIL", "11/20/26  17:45:00");
    d.set("ATTEMPTS", "5");
    ours("strqsipair", &render(&g, "QSIPAIR", &d, &opts()).unwrap());
}

#[test]
fn the_consoles_paired_and_none() {
    let g = group();
    let mut d = MapDialog::new();
    fingerprint(&mut d);
    let row = |name: &str, paired: &str, last: &str, fp: &str| {
        vec![
            ("OPT".to_string(), String::new()),
            ("CNAME".to_string(), name.to_string()),
            ("PAIRED".to_string(), paired.to_string()),
            ("LASTSES".to_string(), last.to_string()),
            ("FPSHORT".to_string(), fp[..16].to_string()),
            ("FPFULL".to_string(), fp.to_string()),
        ]
    };
    d.set_list(
        "CONSOLES",
        ListState {
            total: Some(2),
            rows: vec![
                row("QS/MC on mc01", "11/20/26 17:31", "11/20/26 17:40", FP),
                row(
                    "QS/MC on mc02",
                    "11/20/26 17:35",
                    "",
                    &FP.chars().rev().collect::<String>(),
                ),
            ],
            top: 0,
            ..ListState::default()
        },
    );
    ours("wrkqsicsl", &render(&g, "QSICSL", &d, &opts()).unwrap());
    d.set_list("CONSOLES", ListState::default());
    ours(
        "wrkqsicsl-none",
        &render(&g, "QSICSL", &d, &opts()).unwrap(),
    );
}

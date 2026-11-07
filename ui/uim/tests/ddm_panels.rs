// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKDDMF` against the primary's capture
//! (`tools/conformance/golden/phase12/oracle/ddm-wscst-screens.txt`):
//! every row the oracle shows, at its place, blank rows dropped as the
//! capture drops them.

use std::path::Path;

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, compile};

/// The capture's `Work with DDM Files` screen, its non-blank rows.
fn oracle() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase12/oracle/ddm-wscst-screens.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    let mut screens = text.split("--- screen");
    let screen = screens
        .find(|s| s.contains("Work with DDM Files"))
        .expect("the capture has WRKDDMF");
    screen
        .lines()
        .filter_map(|l| l.strip_prefix('|').and_then(|l| l.strip_suffix('|')))
        .map(|l| l.trim_end().to_string())
        .collect()
}

#[test]
fn wrkddmf() {
    let text = uim::shipped::source("QUIWRKDDMF").expect("shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{}", c.listing);
    let g = c.group.expect("a group");
    let mut d = MapDialog::new();
    d.set_list(
        "FILES",
        ListState {
            rows: vec![vec![
                ("LCLFILE".into(), "QSTEST/QSDDMT".into()),
                ("RMTFILE".into(), "QSTEST/QSDDMR".into()),
                ("RMTLOC".into(), "127.0.0.1".into()),
            ]],
            complete: true,
            ..ListState::default()
        },
    );
    let s = render(&g, "WRKDDMF", &d, &Options::default()).unwrap();
    let ours: Vec<String> = s
        .text_rows()
        .iter()
        .map(|r| r.trim_end().to_string())
        .filter(|r| !r.is_empty())
        .collect();
    assert_eq!(ours, oracle(), "\n{}", s.text_rows().join("\n"));
}

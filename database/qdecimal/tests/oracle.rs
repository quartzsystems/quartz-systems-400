// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The committed oracle kit against the machine's results, when present.

use std::path::{Path, PathBuf};

use qdecimal::fuzz;

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/oracle/qdecimal")
}

#[test]
fn kit_is_current() {
    let dir = oracle_dir();
    let cases = std::fs::read_to_string(dir.join("cases.csv")).expect("cases.csv is committed");
    let cases = fuzz::read_cases(&cases).expect("cases.csv parses");
    assert!(!cases.is_empty());
    let rpg = std::fs::read_to_string(dir.join("QDFUZZ.RPGLE"))
        .expect("QDFUZZ.RPGLE is committed")
        .replace("\r\n", "\n");
    assert_eq!(
        rpg,
        fuzz::rpg::rpg_source(),
        "QDFUZZ.RPGLE is stale; run qdfuzz kit"
    );
    let expected = std::fs::read_to_string(dir.join("expected.csv"))
        .expect("expected.csv")
        .replace("\r\n", "\n");
    let rows: Vec<_> = cases.iter().map(fuzz::expected).collect();
    assert_eq!(
        expected,
        fuzz::write_rows(&rows).replace("\r\n", "\n"),
        "expected.csv is stale; run qdfuzz kit"
    );
}

#[test]
fn machine_results_match() {
    let dir = oracle_dir();
    let results = dir.join("results.csv");
    let Ok(text) = std::fs::read_to_string(&results) else {
        eprintln!(
            "skipped: {} is not present; see {}",
            results.display(),
            dir.join("README.md").display()
        );
        return;
    };
    let cases = fuzz::read_cases(&std::fs::read_to_string(dir.join("cases.csv")).unwrap()).unwrap();
    let rows = fuzz::read_rows(&text).expect("results.csv parses");
    let report = fuzz::diff(&cases, &rows);
    assert!(report.mismatches.is_empty(), "{report}");
    assert!(report.missing.is_empty(), "{report}");
}

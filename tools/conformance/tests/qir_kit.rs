// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IR kit in memory: every case runs, its outcome goes under
//! `target/conformance-qir/ours/` and is compared with `expected/`.
//! `QS400_UPDATE_QIR=1` rewrites the expected files after a deliberate
//! change.

use std::fs;

use conformance::db::compare;
use conformance::qir::{Kit, run};

#[test]
fn every_case_runs_and_matches() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the kit");
    let ours = Kit::ours_dir();
    let _ = fs::remove_dir_all(&ours);
    fs::create_dir_all(&ours).unwrap();
    let update = std::env::var_os("QS400_UPDATE_QIR").is_some();
    let mut failures = Vec::new();
    for case in &cases {
        match run(case) {
            Ok(text) => {
                fs::write(ours.join(format!("{}.out", case.name)), &text).unwrap();
                if update {
                    fs::create_dir_all(kit.expected_dir()).unwrap();
                    fs::write(kit.expected_dir().join(format!("{}.out", case.name)), &text)
                        .unwrap();
                }
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "cases that did not run:\n{}",
        failures.join("\n")
    );
    let c = compare(&ours, &kit.expected_dir());
    eprintln!("{}", c.render());
    assert!(
        c.mismatched.is_empty(),
        "outcomes differ from the expected files:\n{}",
        c.render()
    );
    assert!(
        c.missing.is_empty(),
        "cases without an expected file: {:?}; QS400_UPDATE_QIR=1 cargo test -p conformance --test qir_kit",
        c.missing
    );
}

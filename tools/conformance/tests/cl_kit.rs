// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CL corpus in memory: every program compiles and runs with the
//! doubles, its outcome and listing go under `target/conformance-cl/ours/`
//! and are compared with `expected/`. `QS400_UPDATE_CL=1` rewrites the
//! expected files after a deliberate change.

use std::fs;

use conformance::cl::{Kit, run};
use conformance::db::compare;

#[test]
fn every_program_runs_and_matches() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the corpus");
    assert!(cases.len() >= 40, "the corpus holds {} programs", cases.len());
    let ours = Kit::ours_dir();
    let _ = fs::remove_dir_all(&ours);
    fs::create_dir_all(&ours).unwrap();
    let update = std::env::var_os("QS400_UPDATE_CL").is_some();
    let mut failures = Vec::new();
    for case in &cases {
        match run(case) {
            Ok((text, listing)) => {
                let lst = listing.join("\n") + "\n";
                fs::write(ours.join(format!("{}.out", case.name)), &text).unwrap();
                fs::write(ours.join(format!("{}.lst", case.name)), &lst).unwrap();
                if update {
                    fs::create_dir_all(kit.expected_dir()).unwrap();
                    fs::write(kit.expected_dir().join(format!("{}.out", case.name)), &text).unwrap();
                    fs::write(kit.expected_dir().join(format!("{}.lst", case.name)), &lst).unwrap();
                }
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(failures.is_empty(), "programs that did not run:\n{}", failures.join("\n"));
    let c = compare(&ours, &kit.expected_dir());
    eprintln!("{}", c.render());
    assert!(c.mismatched.is_empty(), "outcomes differ from the expected files:\n{}", c.render());
    assert!(
        c.missing.is_empty(),
        "programs without an expected file: {:?}; QS400_UPDATE_CL=1 cargo test -p conformance --test cl_kit",
        c.missing
    );
}

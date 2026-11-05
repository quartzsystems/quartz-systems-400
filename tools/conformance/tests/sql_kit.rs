// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL kit in memory: every case under `tools/conformance/sql/cases/`
//! run here, written under `target/conformance-sql/ours/` and compared
//! with its capture in `expected/`. `QS400_SQL_ONLY=area/NAME` runs some;
//! the report is `target/conformance-sql/report.txt`. A difference not
//! listed in `known.txt` fails, as does a listed case that agrees now.

use std::fs;

use conformance::sql;

#[test]
fn cases_agree_with_their_captures() {
    let only = std::env::var("QS400_SQL_ONLY").ok();
    let report = sql::run_all(only.as_deref()).expect("cases");
    let text = report.text();
    let out = sql::ours_dir().join("../report.txt");
    fs::write(&out, &text).expect("report");
    println!("{text}");
    let problems = report.problems();
    if only.is_none() {
        assert!(problems.is_empty(), "{}\n{text}", problems.join("\n"));
    }
}

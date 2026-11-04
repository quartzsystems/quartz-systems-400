// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every saved crash, and a slice of every corpus, runs without a panic:
//! a fixed crash stays fixed.

use std::path::Path;

use fuzz::engine::run_one;
use fuzz::targets::TARGETS;

#[test]
fn saved_inputs_do_not_panic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bad = Vec::new();
    for t in TARGETS {
        for (sub, limit) in [("crashes", usize::MAX), ("corpus", 200)] {
            let Ok(rd) = std::fs::read_dir(dir.join(sub).join(t.name)) else {
                continue;
            };
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            for f in files.into_iter().take(limit) {
                let data = std::fs::read(&f).unwrap();
                if std::env::var_os("QS400_FUZZ_TRACE").is_some() {
                    eprintln!("{}", f.display());
                }
                if let Some(p) = run_one(t, &data) {
                    bad.push(format!("{}: {p}", f.display()));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

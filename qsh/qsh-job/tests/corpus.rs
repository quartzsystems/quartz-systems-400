// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The utilities' text corpus (`qsh-utils/tests/corpus/text-*.sht` and
//! `misc-*.sht`) run over [`JobSys`] and a real IFS (`LocalIfs` over the
//! in-memory backend), the files seeded as the `MemSys` corpus seeds
//! them. Every case's standard output, standard error and status must
//! match what the utilities do over `MemSys`. A case naming something
//! only `MemSys` has (its clock, its process table) is listed in `SKIP`.

mod common;

use std::path::{Path, PathBuf};

use common::{Case, parse, run_script, side};

/// Cases that read what only the `MemSys` seed has (its objects, its
/// clock) or that depend on how its file system sets FIFO modes.
const SKIP: &[&str] = &[
    "file qsys",
    "locale a objects",
    "mkfifo makes",
    "mkfifo umask",
    "mkfifo mode plus",
    "mkfifo open",
    "pr stdin header",
    "pr two pages",
    "pr page break",
    "pr length",
    "pr start page",
    "pr merge header",
    "test program newer",
];

fn files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../qsh-utils/tests/corpus");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("the utilities' corpus")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            std::path::Path::new(n).extension().is_some_and(|x| x.eq_ignore_ascii_case("sht"))
                && (n.starts_with("text-") || n.starts_with("misc-"))
        })
        .collect();
    v.sort();
    v
}

#[test]
fn text_and_misc_corpus_over_the_job_sys() {
    let mut failed = Vec::new();
    let mut ran = 0;
    for f in files() {
        let text = std::fs::read_to_string(&f).expect("readable");
        for c in parse(&text) {
            if SKIP.contains(&c.name.as_str()) {
                continue;
            }
            ran += 1;
            let mut s = side();
            let got = run_script(&mut s, &c.script, &c.stdin);
            let want = Case {
                name: c.name.clone(),
                ..got.clone()
            };
            if (got.out.as_str(), got.err.as_str(), got.status)
                != (c.out.as_str(), c.err.as_str(), c.status)
            {
                failed.push(format!(
                    "{}: {}\n  want out {:?} err {:?} status {}\n  got  out {:?} err {:?} status {}",
                    f.file_name().unwrap().to_string_lossy(),
                    c.name,
                    c.out,
                    c.err,
                    c.status,
                    want.out,
                    want.err,
                    want.status
                ));
            }
        }
    }
    assert!(ran > 100, "only {ran} cases");
    assert!(
        failed.is_empty(),
        "{} of {ran} cases differ:\n{}",
        failed.len(),
        failed.join("\n")
    );
}

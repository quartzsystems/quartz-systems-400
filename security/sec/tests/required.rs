// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every operation the code checks is in `required.toml`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The crates whose sources call the guard.
const ROOTS: &[&str] = &[
    "storage/store/src",
    "database/db/src",
    "database/jrn/src",
    "database/sql/src",
    "security/sec/src",
    "work/wm/src",
    "work/qsjob/src",
    "work/stg/src",
    "runtime/mi/src",
    "runtime/sysapi/src",
    "runtime/ile/src",
    "runtime/msgf/src",
    "lic/slic/src",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The string literal right after `name(txn, ` (and its variants).
fn named_ops(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for call in [
        "check_id(txn, \"",
        "check(txn, \"",
        "check(t, \"",
        "require_op(\"",
        "check_on(txn, \"",
        "check_in_library(txn, \"",
    ] {
        let mut rest = text;
        while let Some(i) = rest.find(call) {
            rest = &rest[i + call.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out
}

#[test]
fn every_named_operation_is_in_the_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let table: BTreeSet<String> = sec::auth::required::names()
        .into_iter()
        .map(|n| n.split('@').next().unwrap().to_string())
        .collect();
    let mut files = Vec::new();
    for r in ROOTS {
        sources(&root.join(r), &mut files);
    }
    let mut named = BTreeSet::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for op in named_ops(&text) {
            assert!(
                table.contains(&op),
                "{}: {op} is not in required.toml",
                f.display()
            );
            named.insert(op);
        }
    }
    assert!(named.len() >= 30, "only {} operations found", named.len());
    for op in [
        "obj.delete",
        "lib.create-in",
        "file.open-input",
        "dtaq.send",
        "jrn.read",
    ] {
        assert!(named.contains(op), "{op} is never checked");
    }
}

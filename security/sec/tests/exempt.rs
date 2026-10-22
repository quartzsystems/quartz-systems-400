// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every store, database, journal and work management function that
//! takes a transaction checks authority, or is listed with a reason in
//! `exempt.toml`; and the runtime's ways into another program check the
//! program's state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The crates whose `pub fn`s taking a `Txn` are entry points.
const ROOTS: &[&str] = &[
    "storage/store/src",
    "database/db/src",
    "database/jrn/src",
    "work/wm/src",
];

/// What counts as checking.
const MARKERS: &[&str] = &[
    "check(",
    "check_id(",
    "check_open(",
    "check_in_library(",
    "require_op(",
    "allowed(",
    "guard::",
    "auth::check",
];

/// The runtime's entry points into another program, and what they check.
const MI: &[(&str, &str, &str)] = &[
    ("runtime/mi/src/job.rs", "call_program", "Mch6801"),
    ("runtime/mi/src/act.rs", "load_program", "Mch6801"),
];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// Each `pub fn` of a source, before its tests: name, signature, body.
fn functions(text: &str) -> Vec<(String, String, String)> {
    let text = text.split("#[cfg(test)]").next().unwrap_or("");
    let starts: Vec<usize> = text
        .match_indices("pub fn ")
        .map(|(i, _)| i)
        .filter(|&i| text[..i].rsplit('\n').next().is_some_and(|l| l.trim().is_empty()))
        .collect();
    let mut out = Vec::new();
    for (n, &i) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(text.len());
        let body = &text[i..end];
        let name: String = body["pub fn ".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let sig = body.split('{').next().unwrap_or("").to_string();
        out.push((name, sig, body.to_string()));
    }
    out
}

fn relative(p: &Path, root: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn entry_points_check_or_are_exempt() {
    let root = workspace();
    let text = std::fs::read_to_string(root.join("security/sec/exempt.toml")).unwrap();
    let table: toml::Table = toml::from_str(&text).unwrap();
    let mut exempt: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (file, v) in table["file"].as_table().unwrap() {
        let reason = v["reason"].as_str().unwrap_or("");
        assert!(!reason.trim().is_empty(), "{file} has no reason");
        let fns = v["fns"].as_array().unwrap();
        exempt.insert(
            file.clone(),
            fns.iter().map(|f| f.as_str().unwrap().to_string()).collect(),
        );
    }

    let mut unchecked: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut checked = 0;
    for dir in ROOTS {
        let mut files = Vec::new();
        sources(&root.join(dir), &mut files);
        for f in files {
            let rel = relative(&f, &root);
            let src = std::fs::read_to_string(&f).unwrap();
            for (name, sig, body) in functions(&src) {
                if !sig.contains("Txn<") {
                    continue;
                }
                if MARKERS.iter().any(|m| body.contains(m)) {
                    checked += 1;
                } else {
                    unchecked.entry(rel.clone()).or_default().insert(name);
                }
            }
        }
    }
    assert!(checked > 50, "the scan found only {checked} checked functions");

    let mut missing = Vec::new();
    for (file, names) in &unchecked {
        for n in names {
            if !exempt.get(file).is_some_and(|s| s.contains(n)) {
                missing.push(format!("{file}: {n}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "entry points that check nothing and are not in exempt.toml:\n  {}",
        missing.join("\n  ")
    );
    let mut stale = Vec::new();
    for (file, names) in &exempt {
        for n in names {
            if !unchecked.get(file).is_some_and(|s| s.contains(n)) {
                stale.push(format!("{file}: {n}"));
            }
        }
    }
    assert!(
        stale.is_empty(),
        "exempt.toml lists functions that check or no longer exist:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn runtime_entry_points_check_state() {
    let root = workspace();
    for (file, name, marker) in MI {
        let src = std::fs::read_to_string(root.join(file)).unwrap();
        let body = src
            .split(&format!("pub fn {name}("))
            .nth(1)
            .unwrap_or_else(|| panic!("{file}: no {name}"));
        let body = body.split("\n    pub fn ").next().unwrap_or("");
        assert!(body.contains(marker), "{file}: {name} does not check ({marker})");
    }
}

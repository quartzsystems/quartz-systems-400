// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 8 hygiene: no oracle credential anywhere in the tracked tree,
//! and no line of a captured golden (IBM's text, test data only)
//! reproduced in a source file outside the golden directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// Every file git tracks, plus untracked files it does not ignore.
fn tracked_files(root: &Path) -> Vec<PathBuf> {
    let out = std::process::Command::new("git")
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    out.stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| root.join(String::from_utf8_lossy(p).into_owned()))
        .collect()
}

fn env_values(root: &Path) -> Vec<(String, String)> {
    let Ok(text) = std::fs::read_to_string(root.join("tools/conformance/oracle/.env")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(_, v)| v.len() >= 4)
        .collect()
}

#[test]
fn no_credential_in_the_tree() {
    let root = repo_root();
    let values = env_values(&root);
    if values.is_empty() {
        eprintln!("no .env: the credential check has nothing to look for");
        return;
    }
    // The host names are fine to mention; the user names and passwords
    // are not. pub400's user name is a person's name and appears only
    // in the .env.
    let secrets: Vec<&(String, String)> = values
        .iter()
        .filter(|(k, _)| k.ends_with("_USER") || k.ends_with("_PASS"))
        .filter(|(k, v)| !(k.ends_with("_USER") && v == "QSECOFR"))
        .collect();
    // Files of earlier phases that name the pub400 account as the
    // owner of its libraries; they predate the rule and hold no
    // password.
    let historical = [
        "tools/conformance/db/expected/dspjrn.csv",
        "tools/conformance/db/oracle/pub400/README.md",
        "tools/conformance/db/oracle/pub400/gen.py",
        "tools/conformance/oracle/ile/README.md",
    ];
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if f.ends_with(".env") {
            continue;
        }
        let rel = f
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let old = historical.contains(&rel.as_str());
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for (k, v) in &secrets {
            if text.contains(v.as_str()) && !(old && k.ends_with("_USER")) {
                hits.push(format!("{} holds the value of {k}", f.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "credentials in the tree:\n  {}",
        hits.join("\n  ")
    );
}

#[test]
fn golden_text_stays_in_the_golden_directory() {
    let root = repo_root();
    let golden = root.join("tools/conformance/golden/verified");
    let mut lines: HashSet<String> = HashSet::new();
    if golden.is_dir() {
        for entry in walk(&golden) {
            if entry.extension().is_some_and(|e| e == "txt")
                && let Ok(text) = std::fs::read_to_string(&entry)
            {
                for l in text.lines() {
                    let t = l.trim();
                    // Screen furniture every system shares is not IBM's text.
                    if t.len() > 20 && !t.starts_with("F3=") && !t.contains("===>") {
                        lines.insert(t.to_string());
                    }
                }
            }
        }
    }
    if lines.is_empty() {
        return;
    }
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if f.starts_with(root.join("tools/conformance/golden")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for l in text.lines() {
            let t = l.trim();
            if lines.contains(t) {
                hits.push(format!("{}: {t}", f.display()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "golden text outside the goldens:\n  {}",
        hits.join("\n  ")
    );
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

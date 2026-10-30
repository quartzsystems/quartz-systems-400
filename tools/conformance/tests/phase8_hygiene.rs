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
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if f.ends_with(".env") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        // In any case: a probe once typed the account name in lower case.
        let text = String::from_utf8_lossy(&bytes).to_lowercase();
        for (k, v) in &secrets {
            if text.contains(&v.to_lowercase()) {
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
                    // A column ruler (`....+....1`) is a scale, not text.
                    let ruler = t.chars().all(|c| c == '.' || c == '+' || c.is_ascii_digit());
                    if t.len() > 20 && !t.starts_with("F3=") && !t.contains("===>") && !ruler {
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
        // The prompter's own goldens (Phase 7) are goldens too: the
        // prompt display is the compatibility interface.
        // The oracle kits are our own data on the oracle: the SEU
        // captures edit one of their members, so its records are on
        // screen.
        if f.starts_with(root.join("tools/conformance/golden"))
            || f.starts_with(root.join("command/cmd/tests/golden"))
            || f.starts_with(root.join("command/sys-cpps/tests/golden"))
            || f.starts_with(root.join("tools/conformance/oracle/qdecimal"))
        {
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

/// The Phase 7 denylist (IBM's own phrasings) over the Phase 8 sources
/// that hold screen text: panel groups, shipped display files, menus
/// and the developer tools.
#[test]
fn no_ibm_phrasing_in_screens() {
    let root = repo_root();
    let denied: Vec<String> = std::fs::read_to_string(root.join("command/sys-cpps/denylist.txt"))
        .expect("the denylist")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect();
    let mut hits = Vec::new();
    for dir in ["ui/uim/pnlgrp", "ui/dspf/sys", "ui/menus", "ui/uim/src", "devtools"] {
        for f in walk(&root.join(dir)) {
            // Tests and the oracle's own inventories are test data.
            if f.components().any(|c| c.as_os_str() == "tests" || c.as_os_str() == "inventory") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let lower = text.to_ascii_lowercase();
            for d in &denied {
                if has_phrase(&lower, d) {
                    hits.push(format!("{}: {d:?}", f.display()));
                }
            }
        }
    }
    assert!(hits.is_empty(), "IBM phrasings in screen sources:\n  {}", hits.join("\n  "));
}

/// `phrase` in `text` as whole words: "ibm i" is not "ibm inventories".
fn has_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
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

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 9 hygiene: the message texts in the oracle's compile listings
//! (IBM's words, captured as test data) appear nowhere in the tree but
//! the RPG corpus, and the compiler's own texts are not IBM's.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

fn tracked_files(root: &Path) -> Vec<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard", "-z"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|s| root.join(s))
        .collect()
}

/// The message texts of the listings' summaries: ` *RNF2318 00   1 text`.
fn ibm_texts(root: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    let Ok(rd) = std::fs::read_dir(root.join("tools/conformance/rpg/expected")) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "lst") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else { continue };
        for l in String::from_utf8_lossy(&bytes).lines() {
            let Some(rest) = l.strip_prefix(" *RNF") else { continue };
            // ID digits, severity, count, then the text.
            let Some(mut tail) = rest.get(4..).map(str::trim_start) else { continue };
            for _ in 0..2 {
                tail = tail.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start();
            }
            let t = tail.trim();
            if t.len() > 24 {
                out.insert(t.to_string());
            }
        }
    }
    out
}

#[test]
fn listing_text_stays_in_the_corpus() {
    let root = repo_root();
    let texts = ibm_texts(&root);
    if texts.is_empty() {
        return;
    }
    let corpus = root.join("tools/conformance/rpg");
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if f.starts_with(&corpus) || f.starts_with(root.join("target")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for t in &texts {
            if text.contains(t.as_str()) {
                hits.push(format!("{}: {t}", f.display()));
            }
        }
    }
    assert!(hits.is_empty(), "IBM listing text outside the corpus:\n  {}", hits.join("\n  "));
}

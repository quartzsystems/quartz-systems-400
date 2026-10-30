// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 hygiene: IBM's words captured from the oracles (message
//! texts, FTP replies, `JTOpen` outputs) stay under `tools/conformance`,
//! and the string literals and data files of the Phase 10 crates hold
//! none of IBM's phrasings. The credential and account-name check is
//! `phase8_hygiene::no_credential_in_the_tree`, over the whole tree.
//!
//! The rule (SPEC "Acceptance" 5): IBM formats that clients parse are
//! the interface and are used as-is; IBM prose is not. [`INTERFACE`]
//! lists the IBM output lines kept for that reason:
//!
//! - `qsh-utils`' `db2` prints `DB20000I  THE SQL COMMAND COMPLETED
//!   SUCCESSFULLY.`, `  n RECORD(S) SELECTED.` and the `**** CLI ERROR
//!   *****` block exactly as IBM's `db2` utility does: scripts grep for
//!   the message ID, count the rows from the trailer and detect failure
//!   from the error banner. Each is a fixed token, not explanatory text,
//!   so they are interface.
//! - The FTP server's `SYST` reply names `OS/400`: FTP clients (`JTOpen`'s
//!   `AS400FTP`, ACS, the usual command-line clients) look for it to
//!   switch to the IBM i listing and naming formats.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// IBM output kept because clients and scripts parse it (see above).
const INTERFACE: &[&str] = &[
    "DB20000I  THE SQL COMMAND COMPLETED SUCCESSFULLY.",
    "RECORD(S) SELECTED.",
    "**** CLI ERROR *****",
    "OS/400 is the remote operating system.",
];

/// The Phase 10 crates.
const PHASE10: &[&str] = &["ifs", "net", "qsh", "devtools/edtf"];

/// Captured IBM text shorter than this is too generic to be telling
/// ("File not found.").
const MIN_TEXT: usize = 30;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// Every file git tracks, plus untracked files it does not ignore.
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
        .filter(|p| p.is_file())
        .collect()
}

fn read(p: &Path) -> String {
    std::fs::read(p)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// Message texts: the last `|` field of the captured message files
/// (`MESSAGE_ID|...|MESSAGE_TEXT`, `CPE3001|text`).
fn message_texts(root: &Path, out: &mut BTreeSet<String>) {
    let mut files = vec![root.join("tools/conformance/oracle/ifs/qcpfmsg-ifs.txt")];
    let net = root.join("tools/conformance/oracle/net/pub400");
    if let Ok(rd) = std::fs::read_dir(&net) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("messages-") {
                files.push(e.path());
            }
        }
    }
    for f in files {
        for l in read(&f).lines() {
            if l.starts_with("--") || l.starts_with("MESSAGE_") || !l.contains('|') {
                continue;
            }
            if let Some(t) = l.rsplit('|').next() {
                out.insert(t.trim().to_string());
            }
        }
    }
}

/// FTP reply texts: `S: 250 text` and `S: 250-text`, without the code.
fn ftp_replies(root: &Path, out: &mut BTreeSet<String>) {
    let Ok(rd) = std::fs::read_dir(root.join("tools/conformance/ftp/transcripts")) else {
        return;
    };
    for e in rd.flatten() {
        for l in read(&e.path()).lines() {
            let Some(r) = l.strip_prefix("S: ") else { continue };
            if r.len() > 4 && r.as_bytes()[..3].iter().all(u8::is_ascii_digit) {
                out.insert(r[4..].trim().to_string());
            }
        }
    }
}

/// Message texts in the `JTOpen` outputs: `msg CPF9801 ... text=...`.
fn jtopen_texts(root: &Path, out: &mut BTreeSet<String>) {
    let Ok(rd) = std::fs::read_dir(root.join("tools/conformance/jtopen/expected/oracle")) else {
        return;
    };
    for e in rd.flatten() {
        for l in read(&e.path()).lines() {
            if let Some((_, t)) = l.split_once(" text=") {
                out.insert(t.trim().to_string());
            }
        }
    }
}

/// Every captured IBM text long enough to be telling, with
/// placeholders (`{user}`) and interface lines left out.
fn ibm_texts(root: &Path) -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    message_texts(root, &mut all);
    ftp_replies(root, &mut all);
    jtopen_texts(root, &mut all);
    all.into_iter()
        .filter(|t| t.len() >= MIN_TEXT && !t.contains('{') && is_sentence(t))
        .filter(|t| !INTERFACE.iter().any(|i| t.contains(i)))
        .collect()
}

/// A sentence of prose: four words or more, ending in a period. Field
/// labels and panel titles ("Coded character set identifier") are the
/// prompter's and panels' interface, which the Phase 7 and 8 goldens
/// keep.
fn is_sentence(t: &str) -> bool {
    let words = t
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphabetic))
        .count();
    words >= 4 && t.ends_with('.')
}

/// Known hits in crates other agents are still changing; each is to be
/// rewritten in Quartz words and dropped from here.
const PENDING: &[(&str, &str)] = &[
    ("qsh/qsh/src/sys/errno.rs", "An input or output error occurred."),
];

/// The first bytes of each text, to find candidates in one pass.
const KEY: usize = 12;

/// Every text of `texts` in `hay`.
fn found<'a>(
    hay: &str,
    index: &std::collections::HashMap<&[u8], Vec<&'a str>>,
) -> Vec<&'a str> {
    let b = hay.as_bytes();
    let mut out = Vec::new();
    if b.len() < KEY {
        return out;
    }
    let mut first = [false; 256];
    for k in index.keys() {
        first[usize::from(k[0])] = true;
    }
    for i in 0..=b.len() - KEY {
        if !first[usize::from(b[i])] {
            continue;
        }
        if let Some(list) = index.get(&b[i..i + KEY]) {
            for t in list {
                if b[i..].starts_with(t.as_bytes()) && !out.contains(t) {
                    out.push(*t);
                }
            }
        }
    }
    out
}

#[test]
fn captured_ibm_text_stays_in_conformance() {
    let root = repo_root();
    let texts = ibm_texts(&root);
    assert!(texts.len() > 100, "the captures hold IBM texts ({} found)", texts.len());
    let mut index: std::collections::HashMap<&[u8], Vec<&str>> = std::collections::HashMap::new();
    for t in &texts {
        index.entry(&t.as_bytes()[..KEY]).or_default().push(t.as_str());
    }
    let conformance = root.join("tools/conformance");
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if f.starts_with(&conformance) {
            continue;
        }
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        for t in found(&read(&f), &index) {
            if !PENDING.contains(&(rel.as_str(), t)) {
                hits.push(format!("{rel}: {t}"));
            }
        }
    }
    assert!(hits.is_empty(), "IBM text outside tools/conformance:\n  {}", hits.join("\n  "));
}

/// The string literals of a Rust source: comments are not shipped.
fn rust_strings(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'\'' => {
                // A char literal ('"', '\'') or a lifetime.
                if b.get(i + 1) == Some(&b'\\') {
                    i += 4;
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            b'r' if (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#'))
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) =>
            {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                let start = j + 1;
                let end = src[start..].find(&close).map_or(b.len(), |e| start + e);
                out.push_str(&src[start..end]);
                out.push('\n');
                i = end + close.len();
            }
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    if b[j] == b'\\' {
                        // A line continuation joins with one space.
                        if b.get(j + 1) == Some(&b'\n') || b.get(j + 1) == Some(&b'\r') {
                            out.push(' ');
                            j += 2;
                            while j < b.len() && b[j].is_ascii_whitespace() {
                                j += 1;
                            }
                            continue;
                        }
                        j += 1;
                    }
                    if j < b.len() {
                        let c = src[j..].chars().next().unwrap_or(' ');
                        out.push(c);
                        j += c.len_utf8();
                    }
                }
                out.push('\n');
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// `phrase` in `text` as whole words: "ibm i" is not "ibm inventories".
fn has_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// The shipped text of a Phase 10 file: a Rust source's string
/// literals, a data file whole; `None` for what ships no text (tests,
/// docs, scripts).
fn shipped_text(f: &Path) -> Option<String> {
    if f.components().any(|c| matches!(c.as_os_str().to_str(), Some("tests" | "benches" | "examples"))) {
        return None;
    }
    let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("");
    let text = read(f);
    let shipped = match ext {
        "rs" => rust_strings(cut_tests(&text)),
        "toml" if f.file_name().is_some_and(|n| n == "Cargo.toml") => return None,
        "toml" | "txt" | "pnlgrp" | "dds" | "cmd" | "msgf" => text,
        _ => return None,
    };
    let mut s = shipped.to_ascii_lowercase();
    for i in INTERFACE {
        s = s.replace(&i.to_ascii_lowercase(), " ");
    }
    Some(s)
}

/// A source without its trailing `#[cfg(test)]` module.
fn cut_tests(src: &str) -> &str {
    src.find("#[cfg(test)]\nmod tests")
        .or_else(|| src.find("#[cfg(test)]\r\nmod tests"))
        .map_or(src, |at| &src[..at])
}

#[test]
fn no_ibm_phrasing_in_phase10_crates() {
    let root = repo_root();
    let denied: Vec<String> = read(&root.join("command/sys-cpps/denylist.txt"))
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect();
    assert!(!denied.is_empty(), "the denylist");
    let dirs: Vec<PathBuf> = PHASE10.iter().map(|d| root.join(d)).collect();
    let mut hits = Vec::new();
    for f in tracked_files(&root) {
        if !dirs.iter().any(|d| f.starts_with(d)) {
            continue;
        }
        let Some(text) = shipped_text(&f) else { continue };
        for d in &denied {
            if has_phrase(&text, d) {
                hits.push(format!("{}: {d:?}", f.display()));
            }
        }
    }
    assert!(hits.is_empty(), "IBM phrasings in Phase 10 sources:\n  {}", hits.join("\n  "));
}

#[test]
fn interface_lines_are_used_where_documented() {
    // The db2 lines live in the db2 utility only.
    let root = repo_root();
    let db2 = root.join("qsh/qsh-utils/src/ibmi/db2.rs");
    if !db2.is_file() {
        return;
    }
    let text = rust_strings(&read(&db2));
    for line in &INTERFACE[..3] {
        assert!(text.contains(line), "db2.rs no longer prints {line:?}: update INTERFACE");
    }
}

#[test]
fn string_literals_are_found() {
    let s = rust_strings("let a = \"one\"; // \"not\"\nlet b = r#\"two \"q\"\"#; let c = '\"'; /* \"no\" */ f(\"a\\\n    b\");");
    assert_eq!(s, "one\ntwo \"q\"\na b\n");
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The utilities corpus: `tests/corpus/*.sht`, each a list of cases
//!
//! ```text
//! ### name
//! script
//! --- in          standard input (optional)
//! --- out         expected standard output (`--- out -n`: no final newline)
//! --- err         expected standard error
//! --- status      expected exit status (default 0)
//! ```
//!
//! run against the seeded `MemSys`. `QSH_UPDATE_CORPUS=1` rewrites the
//! expectations from what the shell does.

mod common;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
struct Case {
    name: String,
    script: String,
    stdin: String,
    out: String,
    err: String,
    status: i32,
}

fn body(lines: &[&str], no_newline: bool) -> String {
    let mut s = lines.join("\n");
    if !lines.is_empty() && !no_newline {
        s.push('\n');
    }
    s
}

fn parse(text: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut cur: Option<Case> = None;
    let mut section = String::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut no_nl = false;
    let flush = |cur: &mut Option<Case>, section: &str, lines: &mut Vec<&str>, no_nl: bool| {
        if let Some(c) = cur.as_mut() {
            let b = body(lines, no_nl);
            match section {
                "script" => c.script = b,
                "in" => c.stdin = b,
                "out" => c.out = b,
                "err" => c.err = b,
                "status" => c.status = b.trim().parse().expect("status"),
                other => panic!("unknown section {other}"),
            }
        }
        lines.clear();
    };
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("### ") {
            // The blank line between cases is not part of the last section.
            if lines.last() == Some(&"") {
                lines.pop();
            }
            flush(&mut cur, &section, &mut lines, no_nl);
            if let Some(c) = cur.take() {
                cases.push(c);
            }
            cur = Some(Case {
                name: name.trim().to_string(),
                ..Case::default()
            });
            section = "script".to_string();
            no_nl = false;
        } else if let Some(h) = line.strip_prefix("--- ") {
            flush(&mut cur, &section, &mut lines, no_nl);
            let mut it = h.split_whitespace();
            section = it.next().unwrap_or_default().to_string();
            no_nl = it.next() == Some("-n");
        } else if cur.is_some() {
            lines.push(line);
        }
    }
    flush(&mut cur, &section, &mut lines, no_nl);
    if let Some(c) = cur.take() {
        cases.push(c);
    }
    cases
}

fn section(out: &mut String, name: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if text.ends_with('\n') {
        let _ = writeln!(out, "--- {name}");
        out.push_str(text);
    } else {
        let _ = writeln!(out, "--- {name} -n");
        out.push_str(text);
        out.push('\n');
    }
}

fn render(cases: &[Case]) -> String {
    let mut out = String::new();
    for (i, c) in cases.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "### {}", c.name);
        out.push_str(&c.script);
        section(&mut out, "in", &c.stdin);
        section(&mut out, "out", &c.out);
        section(&mut out, "err", &c.err);
        if c.status != 0 {
            let _ = writeln!(out, "--- status\n{}", c.status);
        }
    }
    out
}

fn files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "sht"))
        .collect();
    v.sort();
    v
}

#[test]
fn corpus() {
    let update = std::env::var_os("QSH_UPDATE_CORPUS").is_some();
    let only = std::env::var("QSH_CORPUS_ONLY").ok();
    let mut failures = Vec::new();
    let mut total = 0;
    for path in files() {
        let text = std::fs::read_to_string(&path)
            .expect("read")
            .replace("\r\n", "\n");
        let mut cases = parse(&text);
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for c in &mut cases {
            if only.as_ref().is_some_and(|o| !c.name.contains(o.as_str())) {
                continue;
            }
            total += 1;
            let sys = common::sys();
            let o = common::run_on(&sys, &c.script, &c.stdin);
            if update {
                c.out = o.out;
                c.err = o.err;
                c.status = o.status;
                continue;
            }
            if o.out != c.out || o.err != c.err || o.status != c.status {
                failures.push(format!(
                    "{file}: {}\n  script: {:?}\n  out:    {:?}\n  want:   {:?}\n  err:    {:?}\n  want:   {:?}\n  status: {} want {}",
                    c.name, c.script, o.out, c.out, o.err, c.err, o.status, c.status
                ));
            }
        }
        if update && only.is_none() {
            std::fs::write(&path, render(&cases)).expect("write");
        }
    }
    assert!(total > 0, "no corpus cases");
    assert!(
        failures.is_empty(),
        "{} of {total} corpus cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

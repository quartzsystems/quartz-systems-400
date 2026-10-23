// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An SDA screen against a pub400 golden, row by row: the golden's masks
//! copied over our side, the copyright line matching where it starts.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::Screen;

/// The masks the captures put over values that differ by system.
const MASKS: &[&str] = &[
    "SYSNAME",
    "USERNAME",
    "NNNNNN",
    "QPADEV000X",
    "MM/DD/YY",
    "HH:MM:SS",
];

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// A golden's rows.
pub fn golden_rows(name: &str) -> Vec<String> {
    let path = golden_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

fn masked(golden: &str, ours: &str) -> String {
    let mark = "(C) COPYRIGHT";
    if golden
        .find(mark)
        .is_some_and(|i| ours.find(mark) == Some(i))
    {
        return golden.to_string();
    }
    let mut out: Vec<char> = ours.chars().collect();
    let g: Vec<char> = golden.chars().collect();
    for m in MASKS {
        let mc: Vec<char> = m.chars().collect();
        let mut i = 0;
        while i + mc.len() <= g.len() {
            if g[i..i + mc.len()] == mc[..] {
                if out.len() < i + mc.len() {
                    out.resize(i + mc.len(), ' ');
                }
                out[i..i + mc.len()].copy_from_slice(&mc);
                i += mc.len();
            } else {
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

/// The screen's characters against golden `name`.
pub fn compare(name: &str, screen: &Screen) {
    let want = golden_rows(name);
    let got = screen.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let ours = masked(w, got.get(i).map_or("", String::as_str));
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(
                diff,
                "row {}:\n  want |{}|\n  got  |{}|",
                i + 1,
                w.trim_end(),
                ours.trim_end()
            );
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

/// Rows of text (the design image) against golden `name`, rows
/// `from..=to`.
pub fn compare_lines(name: &str, got: &[String], from: usize, to: usize) {
    let want = golden_rows(name);
    let mut diff = String::new();
    for r in from..=to {
        let w = want.get(r - 1).map_or("", String::as_str);
        let ours = masked(w, got.get(r - 1).map_or("", String::as_str));
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(
                diff,
                "row {r}:\n  want |{}|\n  got  |{}|",
                w.trim_end(),
                ours.trim_end()
            );
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

/// Only the rows given: the title and the keys, for screens whose
/// values cannot be reproduced.
pub fn compare_rows(name: &str, screen: &Screen, rows: &[usize]) {
    let want = golden_rows(name);
    let got = screen.text_rows();
    for r in rows {
        let w = &want[r - 1];
        let ours = masked(w, &got[r - 1]);
        assert_eq!(w.trim_end(), ours.trim_end(), "{name} row {r}");
    }
}

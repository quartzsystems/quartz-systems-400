// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared by the system panel tests: a shipped group compiled, and a
//! rendered screen compared with a pub400 golden row by row.

#![allow(dead_code)] // each test binary uses what it needs

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, Screen, compile};

/// The pub400 goldens.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// A shipped panel group (`pnlgrp/NAME.pnlgrp`), compiled clean.
pub fn shipped(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name} must compile:\n{}", c.listing);
    c.group.expect("group")
}

/// The golden's masks applied to one of our rows: dates become
/// `MM/DD/YY`, times `HH:MM:SS`.
fn masked(row: &str) -> String {
    let mut c: Vec<char> = row.chars().collect();
    let digit = |c: &[char], i: usize| c.get(i).is_some_and(char::is_ascii_digit);
    let mut i = 0;
    while i + 8 <= c.len() {
        let pair = |k: usize| digit(&c, i + k) && digit(&c, i + k + 1);
        let shape = |sep: char| pair(0) && c[i + 2] == sep && pair(3) && c[i + 5] == sep && pair(6);
        let edge = (i == 0 || !c[i - 1].is_ascii_alphanumeric())
            && c.get(i + 8).is_none_or(|ch| !ch.is_ascii_alphanumeric());
        if edge && shape('/') {
            c.splice(i..i + 8, "MM/DD/YY".chars());
            i += 8;
        } else if edge && shape(':') {
            c.splice(i..i + 8, "HH:MM:SS".chars());
            i += 8;
        } else {
            i += 1;
        }
    }
    c.into_iter().collect()
}

/// The golden's number mask: where it shows `N`, a run of digits on
/// our row becomes `N` and blanks of the same length.
fn numbers(golden: &str, row: &str) -> String {
    let w: Vec<char> = golden.chars().collect();
    let mut c: Vec<char> = row.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let starts = w.get(i) == Some(&'N')
            && c[i].is_ascii_digit()
            && (i == 0 || !c[i - 1].is_ascii_alphanumeric());
        if starts {
            c[i] = 'N';
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                c[i] = ' ';
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    c.into_iter().collect()
}

/// Compare a screen's character grid with golden `NAME.txt`. The
/// message line is skipped when the golden shows IBM's copyright
/// there (ours is Quartz's).
pub fn compare(golden: &str, screen: &Screen) {
    let path = golden_dir().join(format!("{golden}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want: Vec<String> = text.lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
    let got = screen.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        if w.trim_start().starts_with("(C) COPYRIGHT") {
            continue;
        }
        let g = got.get(i).map(|r| numbers(w, &masked(r))).unwrap_or_default();
        if g.trim_end() != w.trim_end() {
            let _ = writeln!(diff, "row {:>2}:\n  want |{}|\n  got  |{}|", i + 1, w.trim_end(), g.trim_end());
        }
    }
    assert!(diff.is_empty(), "{golden} differs from the golden:\n{diff}");
}

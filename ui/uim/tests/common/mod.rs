// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared by the system panel tests: the shipped groups, the goldens
//! and the row-by-row comparison with the golden's masks applied to
//! our side.

#![allow(dead_code)] // each test binary uses part of it

use std::path::{Path, PathBuf};

use uim::render::Screen;
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

/// The golden directory of the pub400 captures.
#[must_use]
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// A golden's character grid.
#[must_use]
pub fn golden(name: &str) -> String {
    let path = golden_dir().join(format!("{name}.txt"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A shipped panel group, compiled.
#[must_use]
pub fn shipped(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name}:\n{}", c.listing);
    c.group.expect("a group")
}

/// Replace `dd?dd?dd` (with `/`, `.`, `-` or `:` as given) by a token.
fn mask_triplet(row: &mut [char], seps: &[char], token: &str) {
    let tok: Vec<char> = token.chars().collect();
    let mut i = 0;
    while i + 8 <= row.len() {
        let w = &row[i..i + 8];
        let digit = |k: usize| w[k].is_ascii_digit();
        let edge_ok = (i == 0 || !row[i - 1].is_ascii_alphanumeric())
            && (i + 8 == row.len() || !row[i + 8].is_ascii_alphanumeric());
        if edge_ok
            && digit(0)
            && digit(1)
            && seps.contains(&w[2])
            && digit(3)
            && digit(4)
            && w[5] == w[2]
            && digit(6)
            && digit(7)
        {
            row[i..i + 8].copy_from_slice(&tok);
            i += 8;
        } else {
            i += 1;
        }
    }
}

/// The default masks of the capture tool on one of our rows: dates,
/// times and the name after `System:`.
fn mask(row: &str) -> String {
    let mut chars: Vec<char> = row.chars().collect();
    mask_triplet(&mut chars, &['/', '.', '-'], "MM/DD/YY");
    mask_triplet(&mut chars, &[':'], "HH:MM:SS");
    let text: String = chars.iter().collect();
    if let Some(at) = text.find("System:") {
        let start = at + "System:".len();
        let rest = &text[start..];
        let blanks = rest.len() - rest.trim_start().len();
        let name_len = rest.trim_start().find(' ').unwrap_or(rest.trim_start().len());
        if name_len > 0 {
            let from = start + blanks;
            let mut tok: String = "SYSNAME".chars().take(name_len).collect();
            while tok.len() < name_len {
                tok.push(' ');
            }
            return format!("{}{tok}{}", &text[..from], &text[from + name_len..]);
        }
    }
    text
}

/// Compare a screen with a golden's grid row by row, our rows masked
/// as the capture tool masks; panics with every row that differs.
pub fn compare(golden: &str, screen: &Screen) {
    let ours = screen.text_rows();
    let theirs: Vec<String> = golden
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    let mut diffs = Vec::new();
    for (i, row) in ours.iter().enumerate() {
        let want = theirs.get(i).map_or("", String::as_str);
        let got = mask(row);
        if got.trim_end() != want.trim_end() {
            diffs.push(format!(
                "row {:>2}\n  golden |{want}|\n  ours   |{got}|",
                i + 1
            ));
        }
    }
    assert!(diffs.is_empty(), "{} row(s) differ:\n{}", diffs.len(), diffs.join("\n"));
}

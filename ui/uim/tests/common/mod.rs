// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared by the panel tests: the shipped groups compiled, and a
//! rendered screen compared with a pub400 golden row by row, the
//! golden's masks applied to our side.

#![allow(dead_code)] // each test binary uses its share

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, Screen, compile};

/// The masks the captures put over values that differ by system.
pub const MASKS: &[&str] = &[
    "SYSNAME",
    "USERNAME",
    "NNNNNN",
    "QPADEV000X",
    "MM/DD/YY",
    "HH:MM:SS",
];

/// The pub400 golden directory.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// A golden's character rows, line ends dropped.
pub fn golden_rows(name: &str) -> Vec<String> {
    golden_rows_in(&golden_dir(), name)
}

/// A golden's character rows from `dir`.
pub fn golden_rows_in(dir: &Path, name: &str) -> Vec<String> {
    let path = dir.join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A panel group from a source file, compiled.
pub fn group_from(path: &Path) -> PanelGroup {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let c = compile(&text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{} must compile:\n{}", path.display(), c.listing);
    c.group.expect("group")
}

/// A shipped system panel group (`ui/uim/pnlgrp/NAME.pnlgrp`).
pub fn shipped(name: &str) -> PanelGroup {
    group_from(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("pnlgrp")
            .join(format!("{name}.pnlgrp")),
    )
}

/// Our row with every mask the golden row shows copied over the
/// same columns.
fn masked(golden: &str, ours: &str) -> String {
    // A copyright line is the vendor's own: only where it starts counts.
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

/// The screen's character grid against golden `name`, row by row.
pub fn compare(golden: &str, screen: &Screen) {
    compare_in(&golden_dir(), golden, screen);
}

/// [`compare`] against a golden of `dir`.
pub fn compare_in(dir: &Path, golden: &str, screen: &Screen) {
    let want = golden_rows_in(dir, golden);
    let got = screen.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let ours = got.get(i).map_or("", String::as_str);
        let ours = masked(w, ours);
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
    assert!(diff.is_empty(), "{golden} differs:\n{diff}");
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared by the TCP/IP panel tests: the Phase 10 goldens (pub400,
//! V7R5M0) and the row-by-row comparison, the golden's masks and
//! copyright line taken as `common` takes them.

#![allow(dead_code)] // each test binary uses part of it

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::render::Screen;
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

/// The Phase 10 golden directory.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase10/tcpcfg")
}

/// A golden's rows.
pub fn golden_rows(name: &str) -> Vec<String> {
    let path = golden_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.replace("\r\n", "\n").lines().map(str::to_string).collect()
}

/// A shipped panel group, compiled.
pub fn shipped(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name}:\n{}", c.listing);
    c.group.expect("a group")
}

/// Our row with the golden's `SYSNAME` mask copied over the same
/// columns; a vendor copyright line counts by where it starts.
fn masked(golden: &str, ours: &str) -> String {
    let mark = "(C) COPYRIGHT";
    if golden.find(mark).is_some_and(|i| ours.find(mark) == Some(i)) {
        return golden.to_string();
    }
    let mut out: Vec<char> = ours.chars().collect();
    let g: Vec<char> = golden.chars().collect();
    let m: Vec<char> = "SYSNAME".chars().collect();
    let mut i = 0;
    while i + m.len() <= g.len() {
        if g[i..i + m.len()] == m[..] {
            if out.len() < i + m.len() {
                out.resize(i + m.len(), ' ');
            }
            out[i..i + m.len()].copy_from_slice(&m);
            i += m.len();
        } else {
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// The screen against golden `name`, row by row.
pub fn compare(name: &str, screen: &Screen) {
    let want = golden_rows(name);
    let got = screen.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let ours = masked(w, got.get(i).map_or("", String::as_str));
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(diff, "row {}:\n  want |{}|\n  got  |{}|", i + 1, w.trim_end(), ours.trim_end());
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

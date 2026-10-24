// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The grammar fuzzer's programs through the compiler and the runtime:
//! each is created (no internal error, nothing above severity 10) and
//! runs to its end. `QS400_FUZZ_N` sets how many (2000 in a release
//! build, 300 in a debug one);
//! `QS400_FUZZ_WRITE=N` also writes the first N as corpus members for
//! the oracle under `tools/conformance/rpg/b14fuzz/`.

use conformance::rpg::{self, fuzz};
use rpg_oracle::case::Case;
use rpg_oracle::outcome::Run;

#[test]
fn fuzzed_programs_compile_and_run() {
    let n: u64 = std::env::var("QS400_FUZZ_N").ok().and_then(|v| v.parse().ok()).unwrap_or(if cfg!(debug_assertions) { 300 } else { 2000 });
    let write: u64 = std::env::var("QS400_FUZZ_WRITE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rpg/b14fuzz");
    let mut bad = Vec::new();
    for seed in 0..n {
        let name = format!("FUZ{seed:04}");
        let text = fuzz::program(seed);
        if seed < write {
            std::fs::create_dir_all(&dir).expect("the fuzz folder");
            std::fs::write(dir.join(format!("{name}.rpgle")), &text).expect("a fuzz member");
        }
        let case = Case::parse(&name, &text);
        match rpg::run(&case) {
            Ok((o, _, notes)) => {
                let worst = o.diags.iter().map(|d| d.severity).max().unwrap_or(0);
                if !o.created || worst > 10 {
                    let ids: Vec<&str> = o.diags.iter().map(|d| d.msgid.as_str()).collect();
                    bad.push(format!("{name}: not created, severity {worst} {ids:?} {notes}"));
                } else if let Run::Escape(m) = &o.run {
                    bad.push(format!("{name}: escape {m}"));
                }
            }
            Err(e) => bad.push(format!("{name}: {e}")),
        }
    }
    assert!(bad.is_empty(), "{} of {n} fuzzed programs failed:\n  {}", bad.len(), bad.join("\n  "));
}

/// The members `rpg-oracle fuzz` writes, 20 programs to a member: each
/// is created and runs to its end, as its programs do alone.
#[test]
fn fuzz_bundles_compile_and_run() {
    let n: u64 = if cfg!(debug_assertions) { 100 } else { 2000 };
    let mut bad = Vec::new();
    for first in (0..n).step_by(20) {
        let name = format!("FZB{:03}", first / 20);
        let case = Case::parse(&name, &fuzz::bundle(first, 20));
        match rpg::run(&case) {
            Ok((o, _, notes)) => {
                let worst = o.diags.iter().map(|d| d.severity).max().unwrap_or(0);
                if !o.created || worst > 10 {
                    let ids: Vec<&str> = o.diags.iter().map(|d| d.msgid.as_str()).collect();
                    bad.push(format!("{name}: not created, severity {worst} {ids:?} {notes}"));
                } else if let Run::Escape(m) = &o.run {
                    bad.push(format!("{name}: escape {m}"));
                }
            }
            Err(e) => bad.push(format!("{name}: {e}")),
        }
    }
    assert!(bad.is_empty(), "{} bundles failed:\n  {}", bad.len(), bad.join("\n  "));
}

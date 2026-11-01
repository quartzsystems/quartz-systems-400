// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Our start-up response record against captures of the same exchange
//! with another system (`tests/fixtures/startup/`, see its README): the
//! same bytes except the system and device name values.

use std::path::PathBuf;

use tn5250::startup;

/// The record in a capture: hex text (whitespace ignored) or raw bytes;
/// a trailing `IAC EOR` and doubled `IAC`s removed.
fn record_of(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let hex: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut b = if !hex.is_empty() && hex.len().is_multiple_of(2) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    } else {
        raw.to_vec()
    };
    if b.ends_with(&[0xFF, 0xEF]) {
        b.truncate(b.len() - 2);
    }
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        out.push(b[i]);
        i += if b[i] == 0xFF && b.get(i + 1) == Some(&0xFF) { 2 } else { 1 };
    }
    out
}

#[test]
fn captures_match_ours() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/startup");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        if !matches!(p.extension().and_then(|x| x.to_str()), Some("bin" | "hex")) {
            continue;
        }
        let theirs = record_of(&std::fs::read(&p).unwrap());
        let got = startup::decode(&theirs).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let mut ours = startup::encode(&got.code, &got.system, &got.device);
        // The name values are each system's own.
        let mut theirs_masked = theirs.clone();
        for r in [20..28, 28..38] {
            ours[r.clone()].fill(0);
            theirs_masked[r].fill(0);
        }
        assert_eq!(ours, theirs_masked, "{} (code {})", p.display(), got.code);
        seen += 1;
    }
    if seen == 0 {
        eprintln!("no captures in {}: nothing compared", dir.display());
    }
}

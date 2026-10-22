// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The analyzer kit: every string of `conformance/cmd/strings.txt`
//! analyzed, the outcomes compared line by line with `expected.txt`.
//! `QS400_UPDATE_CMD=1` rewrites the expected file after a deliberate
//! change.

use conformance::cmdkit::{root, run, strings};

#[test]
fn every_string_matches() {
    let strings = strings().expect("the kit's strings");
    assert!(strings.len() >= 300, "the kit holds {} strings", strings.len());
    let got = run(&strings);
    let path = root().join("expected.txt");
    let mut rendered = String::new();
    for (s, o) in strings.iter().zip(&got) {
        rendered.push_str(s);
        rendered.push_str("\n  ");
        rendered.push_str(o);
        rendered.push('\n');
    }
    if std::env::var_os("QS400_UPDATE_CMD").is_some() {
        std::fs::write(&path, &rendered).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_CMD=1 writes it)", path.display()))
        .replace("\r\n", "\n");
    let mut diffs = Vec::new();
    for (i, (a, b)) in want.lines().zip(rendered.lines()).enumerate() {
        if a != b {
            diffs.push(format!("line {}: expected {a:?}, got {b:?}", i + 1));
        }
    }
    if want.lines().count() != rendered.lines().count() {
        diffs.push(format!(
            "{} expected lines, {} produced",
            want.lines().count(),
            rendered.lines().count()
        ));
    }
    assert!(diffs.is_empty(), "\n{}", diffs.join("\n"));
}

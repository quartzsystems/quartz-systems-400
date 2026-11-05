// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every SQLCODE the engine can raise has its message in QSQLMSG
//! (`runtime/msgf/msgs/qsqlmsg.toml`): a tripwire over the sources.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// SQLCODEs written in the sources: `=> -204,` arms and `sqlcode: N`.
fn codes(dir: &Path, out: &mut BTreeSet<i32>) {
    for e in fs::read_dir(dir).expect("source dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            codes(&p, out);
            continue;
        }
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let text = fs::read_to_string(&p).expect("source");
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with("//") {
                continue;
            }
            for key in ["=> ", "sqlcode: ", "sqlcode = "] {
                let mut rest = t;
                while let Some(i) = rest.find(key) {
                    rest = &rest[i + key.len()..];
                    let num: String = rest
                        .chars()
                        .take_while(|c| *c == '-' || c.is_ascii_digit())
                        .collect();
                    let after = rest[num.len()..].chars().next();
                    // A match arm counts only when negative (an error code);
                    // positive ones are CCSIDs, lengths and the like.
                    let arm_ok = key != "=> " || num.starts_with('-');
                    if arm_ok
                        && num.len() > 1
                        && matches!(after, Some(',' | ';' | ' ') | None)
                        && let Ok(n) = num.parse::<i32>()
                    {
                        out.insert(n);
                    }
                }
            }
        }
    }
}

#[test]
fn every_sqlcode_has_a_message() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = BTreeSet::new();
    codes(&root.join("src"), &mut found);
    found.retain(|c| *c != 0 && (*c < 0 || *c >= 100));
    let msgs = fs::read_to_string(root.join("../../runtime/msgf/msgs/qsqlmsg.toml")).expect("QSQLMSG");
    let missing: Vec<String> = found
        .iter()
        .map(|c| format!("SQL{:04}", c.unsigned_abs()))
        .filter(|id| !msgs.contains(&format!("id = \"{id}\"")))
        .collect();
    assert!(found.contains(&-204) && found.contains(&-84), "the scan finds the error table");
    assert!(missing.is_empty(), "SQLCODEs without a QSQLMSG message: {missing:?}");
}

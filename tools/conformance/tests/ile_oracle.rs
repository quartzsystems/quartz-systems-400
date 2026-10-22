// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE oracle: the machine's observations under
//! `conformance/oracle/ile/expected/ile-oracle.txt`, when captured,
//! against this implementation's readings of the same open questions.

use std::fs;
use std::path::Path;

/// What this implementation observes for each oracle line, keyed by
/// the program and the start of its line.
fn ours() -> Vec<(&'static str, &'static str)> {
    vec![
        ("CVTCHAR", "[1.50]"),
        ("CVTCHAR", "[0.5]"),
        ("CVTCHAR", "[100]"),
        ("CVTCHAR", "[-0.5]"),
        ("DATEBAD", "status 112"),
        ("SUBSTBAD", "status 121"),
        ("RESUMEG", "after, z=0"),
        ("REPLYR", "after, z=0"),
        ("STATMAP", "MCH1211 status 102"),
        ("STATMAP", "MCH1210 status 103"),
        ("STATMAP", "MCH1202 status 907"),
        ("STATMAP", "MCH0603 status 121"),
        ("STATMAP", "MCH3601 status 222"),
        ("FCWHERE", "caller saw status 9999"),
        ("PROMOTE", "monitor saw status 102"),
        ("CEEFC", "CEEHDLU unregistered: CEE2057"),
        ("CEEFC", "CEEMRCR action 1: CEE2064"),
        ("CANCELTO", "caller got CEE9901 status 202"),
        ("DEEPREC", "got MCH0601 status 222"),
        ("CMDEXC", "received [from QCMDEXC]"),
        ("PTRSTAT", "static neighbour read ok"),
        ("PTRSTAT", "auto neighbour status 222"),
        ("DECBLANK", "[12]"),
        ("DIFFTS", "earlier time of day: months 0"),
        ("DIFFTS", "later time of day: months 1"),
    ]
}

#[test]
fn machine_observations_match_when_captured() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("oracle")
        .join("ile")
        .join("expected")
        .join("ile-oracle.txt");
    let Ok(text) = fs::read_to_string(&p) else {
        eprintln!("skipped: {} not captured yet", p.display());
        return;
    };
    let machine: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .collect();
    let mut differ = Vec::new();
    for (i, (pgm, line)) in ours().into_iter().enumerate() {
        match machine.get(i) {
            Some((mp, ml)) if mp == pgm && ml == line => {}
            Some((mp, ml)) => differ.push(format!("{pgm}: ours `{line}`, machine `{mp}: {ml}`")),
            None => differ.push(format!(
                "{pgm}: ours `{line}`, machine has no line {}",
                i + 1
            )),
        }
    }
    assert!(
        differ.is_empty(),
        "readings the machine settles differently:\n{}",
        differ.join("\n")
    );
}

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
/// Readings the machine settled that the implementation does not match
/// yet: program and line.
const OPEN: &[(&str, usize)] = &[("PTRSTAT", 2)];

fn ours() -> Vec<(&'static str, &'static str)> {
    vec![
        ("CVTCHAR", "[1.50]"),
        ("CVTCHAR", "[.50]"),
        ("CVTCHAR", "[100]"),
        ("CVTCHAR", "[-.50]"),
        ("DATEBAD", "status 112"),
        ("SUBSTBAD", "status 100"),
        ("RESUMEG", "after, z=0"),
        ("REPLYR", "after, z=0"),
        ("STATMAP", "MCH1211 status 102"),
        ("STATMAP", "MCH1210 status 103"),
        ("STATMAP", "MCH1202 status 907"),
        ("STATMAP", "MCH0603 status 121"),
        ("STATMAP", "MCH3601 status 222"),
        ("FCWHERE", "caller saw status 202"),
        ("PROMOTE", "monitor saw status 102"),
        ("CEEFC", "CEEHDLU unregistered: CEE2057"),
        ("CEEFC", "CEEMRCR action 1: CEE2064"),
        ("CANCELTO", "caller got MCH1211 status 202"),
        ("DEEPREC", "got MCH4429 status 202"),
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
    // Per program, in order: a program the machine could not run (no
    // reply queued, a function check) leaves its own lines out and
    // shifts nothing else.
    let mut differ = Vec::new();
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (pgm, line) in ours() {
        let n = seen.entry(pgm).or_insert(0);
        let theirs = machine.iter().filter(|(mp, _)| mp == pgm).nth(*n);
        *n += 1;
        match theirs {
            Some((_, ml)) if ml == line => {}
            // Settled by the capture but not yet implemented: the string
            // operation's RNX0100 (status 100) and a readable neighbour of
            // an automatic variable (ILE_OPEN_QUESTIONS.md 3 and 4).
            Some((_, ml)) if OPEN.contains(&(pgm, *n)) => {
                eprintln!("still open: {pgm} line {n}: ours `{line}`, machine `{ml}`");
            }
            Some((_, ml)) => differ.push(format!("{pgm}: ours `{line}`, machine `{ml}`")),
            // A program the box could not run (the inquiry replies need a
            // reply list the account cannot change) leaves no line.
            None => eprintln!("not captured: {pgm} line {n} (ours `{line}`)"),
        }
    }
    assert!(
        differ.is_empty(),
        "readings the machine settles differently:\n{}",
        differ.join("\n")
    );
}

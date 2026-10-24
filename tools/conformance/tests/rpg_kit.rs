// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG corpus in memory: every member compiles and runs, its
//! outcome and listing go under `target/conformance-rpg/ours/`, and the
//! outcomes are compared with the oracle's in `expected/` (provenance
//! lines aside). Members the oracle has not run yet are listed, and so
//! are those `known.txt` names (a member, or `batch/*`) as known to
//! differ.

use std::fs;

use conformance::rpg::{Kit, run};
use rpg_oracle::outcome::comparable;

#[test]
fn every_member_matches_the_oracle() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the corpus");
    let ours = Kit::ours_dir();
    let _ = fs::remove_dir_all(&ours);
    fs::create_dir_all(&ours).unwrap();
    let mut failures = Vec::new();
    let mut missing = Vec::new();
    let mut passed = 0;
    let known_text = fs::read_to_string(kit.root.join("known.txt")).unwrap_or_default();
    let known: Vec<&str> = known_text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let mut known_hit = Vec::new();
    // QS400_RPG_ONLY=PREFIX runs only the members whose names start so.
    let only = std::env::var("QS400_RPG_ONLY").unwrap_or_default();
    for (batch, case) in cases.iter().filter(|(_, c)| c.name.starts_with(&only)) {
        let (o, listing, notes) = match run(case) {
            Ok(x) => x,
            Err(e) => {
                failures.push(format!("{}: {e}", case.name));
                continue;
            }
        };
        let text = o.render();
        fs::write(ours.join(format!("{}.out", case.name)), &text).unwrap();
        fs::write(ours.join(format!("{}.lst", case.name)), listing.join("\n") + "\n").unwrap();
        fs::write(ours.join(format!("{}.qir", case.name)), notes).unwrap();
        let exp = kit.expected_dir().join(format!("{}.out", case.name));
        match fs::read_to_string(&exp) {
            Ok(e) if comparable(&e) == comparable(&text) => passed += 1,
            Ok(_) if known.contains(&case.name.as_str()) || known.contains(&format!("{batch}/*").as_str()) => {
                known_hit.push(case.name.clone());
            }
            Ok(_) => failures.push(format!("{batch}/{}: differs (diff target/conformance-rpg/ours/{0}.out against expected)", case.name)),
            Err(_) => missing.push(case.name.clone()),
        }
    }
    eprintln!(
        "rpg kit: {passed} match, {} differ, {} known to differ, {} without an oracle outcome",
        failures.len(),
        known_hit.len(),
        missing.len()
    );
    if !known_hit.is_empty() {
        eprintln!("known to differ: {}", known_hit.join(" "));
    }
    if !missing.is_empty() {
        eprintln!("not yet run on the oracle: {}", missing.join(" "));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

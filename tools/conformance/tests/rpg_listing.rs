// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG corpus's compile listings against the oracle's: every
//! member with an `expected/NAME.lst` is compiled (not run), both
//! listings are normalized (`conformance::rpg::lstcmp`) and compared
//! line for line. The normalized pair goes under
//! `target/conformance-rpg/lst/` (ours unnormalized under `raw/`) for a diff. Members `known_lst.txt`
//! names (a member, or `batch/*`) are reported apart; any other
//! difference fails. `QS400_LST=A,B` compares only those members.

use std::fs;

use conformance::rpg::{Kit, compile, files, lstcmp};

#[test]
fn every_listing_matches_the_oracle() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the corpus");
    let dir = Kit::ours_dir().join("..").join("lst");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("ours")).unwrap();
    fs::create_dir_all(dir.join("ibm")).unwrap();
    fs::create_dir_all(dir.join("raw")).unwrap();
    let known_text = fs::read_to_string(kit.root.join("known_lst.txt")).unwrap_or_default();
    let known: Vec<&str> = known_text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let only = std::env::var("QS400_LST").unwrap_or_default();
    let mut passed = 0;
    let mut differ = Vec::new();
    let mut known_hit = Vec::new();
    let mut missing = 0;
    for (batch, case) in &cases {
        if !only.is_empty() && !only.split(',').any(|n| n == case.name) {
            continue;
        }
        let Ok(exp) = fs::read_to_string(kit.expected_dir().join(format!("{}.lst", case.name))) else {
            missing += 1;
            continue;
        };
        let ibm: Vec<String> = exp.lines().map(str::to_string).collect();
        let verb = case.cmd.trim_start().to_ascii_uppercase();
        let ours: Vec<String> = if verb.starts_with("CRTPGM") {
            Vec::new()
        } else {
            let db = if case.db.is_empty() {
                None
            } else {
                match files::KitDb::build(&case.db) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        differ.push(format!("{}: {e}", case.name));
                        continue;
                    }
                }
            };
            compile(case, db.as_ref()).listing
        };
        let ours_n = lstcmp::normalize(&ours, true);
        let ibm_n = lstcmp::normalize(&ibm, false);
        fs::write(dir.join("raw").join(format!("{}.lst", case.name)), ours.join("\n") + "\n").unwrap();
        fs::write(dir.join("ours").join(format!("{}.lst", case.name)), ours_n.join("\n") + "\n").unwrap();
        fs::write(dir.join("ibm").join(format!("{}.lst", case.name)), ibm_n.join("\n") + "\n").unwrap();
        match lstcmp::first_difference(&ours_n, &ibm_n) {
            None => passed += 1,
            Some(_) if known.contains(&case.name.as_str()) || known.contains(&format!("{batch}/*").as_str()) => {
                known_hit.push(case.name.clone());
            }
            Some((n, ours_l, ibm_l)) => differ.push(format!("{}: line {n}\n    ours: {ours_l}\n    ibm:  {ibm_l}", case.name)),
        }
    }
    eprintln!(
        "rpg listings: {passed} match, {} differ, {} known to differ, {missing} without an oracle listing",
        differ.len(),
        known_hit.len()
    );
    if !known_hit.is_empty() {
        eprintln!("known to differ: {}", known_hit.join(" "));
    }
    for d in &differ {
        eprintln!("{d}");
    }
    assert!(differ.is_empty(), "{} listings differ (diff target/conformance-rpg/lst/ours against ibm)", differ.len());
}

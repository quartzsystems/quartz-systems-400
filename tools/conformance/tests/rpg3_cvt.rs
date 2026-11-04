// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CVTRPGSRC` against IBM's: every RPG III member of the corpus is
//! converted and compared, record for record, with the member pub400's
//! `CVTRPGSRC` wrote (`expected/NAME.cvt`). Ours go to
//! `target/conformance-rpg/ours/NAME.cvt`.

use std::fs;

use conformance::rpg::Kit;

#[test]
fn conversions_match_the_oracle() {
    let kit = Kit::locate();
    let ours = Kit::ours_dir();
    fs::create_dir_all(&ours).unwrap();
    let (mut compared, mut failures) = (0, Vec::new());
    for (_, case) in kit.cases().unwrap().iter().filter(|(_, c)| c.rpg3) {
        let conv = rpg3::convert(&case.lines);
        let text = conv.text();
        fs::write(ours.join(format!("{}.cvt", case.name)), text.join("\n") + "\n").unwrap();
        let Ok(e) = fs::read_to_string(kit.expected_dir().join(format!("{}.cvt", case.name))) else { continue };
        compared += 1;
        let ibm: Vec<&str> = e.lines().filter(|l| !l.starts_with("# ")).collect();
        let first = (0..ibm.len().max(text.len()))
            .find(|&i| ibm.get(i).map(|l| l.trim_end()) != text.get(i).map(|l| l.trim_end()));
        if let Some(i) = first {
            failures.push(format!(
                "{} record {}:\n  ibm:  {:?}\n  ours: {:?}",
                case.name,
                i + 1,
                ibm.get(i).unwrap_or(&""),
                text.get(i).map_or("", String::as_str)
            ));
        }
    }
    eprintln!("CVTRPGSRC: {} of {compared} conversions match", compared - failures.len());
    // The reports, blank lines aside, with the oracle's masks.
    let mut reports = 0;
    for (_, case) in kit.cases().unwrap().iter().filter(|(_, c)| c.rpg3) {
        let Ok(e) = fs::read_to_string(kit.expected_dir().join(format!("{}.rpt", case.name))) else { continue };
        if !e.contains("CVTRPGSRC") {
            continue;
        }
        reports += 1;
        let conv = rpg3::convert(&case.lines);
        let p = rpg3::report::Params {
            user: "{user}".into(),
            from: ("{lib}".into(), "QRPGSRC".into(), case.name.clone()),
            to: ("{lib}".into(), "QRPGLESRC".into(), "*FROMMBR".into()),
            ..rpg3::report::Params::default()
        };
        let ours = rpg_oracle::listing::mask(&rpg3::report::report(&conv, &p), &rpg_oracle::listing::Mask::default());
        fs::write(Kit::ours_dir().join(format!("{}.rpt", case.name)), ours.join("\n") + "\n").unwrap();
        let keep = |l: &&str| l.len() > 1;
        let a: Vec<&str> = e.lines().filter(keep).collect();
        let b: Vec<&str> = ours.iter().map(String::as_str).filter(keep).collect();
        if a != b {
            let i = (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i)).unwrap_or(0);
            failures.push(format!("{} report line {}:\n  ibm:  {:?}\n  ours: {:?}", case.name, i + 1, a.get(i), b.get(i)));
        }
    }
    eprintln!("CVTRPGSRC: {reports} reports compared");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

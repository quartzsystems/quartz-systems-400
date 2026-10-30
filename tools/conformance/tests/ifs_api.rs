// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS API corpus in memory: every program runs against a fresh
//! `/home/QSTEST`, its console goes under `target/conformance-ifsapi/ours/`
//! and is compared with `expected/`. `QS400_UPDATE_IFSAPI=1` rewrites
//! the expected files after a deliberate change.

use std::fs;

use conformance::db::compare;
use conformance::ifsapi::{Kit, run};

#[test]
fn every_program_runs_and_matches() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the corpus");
    assert!(!cases.is_empty(), "no programs");
    let protos = kit.protos().expect("protos.qir");
    let ours = Kit::ours_dir();
    let _ = fs::remove_dir_all(&ours);
    fs::create_dir_all(&ours).unwrap();
    let update = std::env::var_os("QS400_UPDATE_IFSAPI").is_some();
    let mut failures = Vec::new();
    for case in &cases {
        match run(case, &protos) {
            Ok(text) => {
                fs::write(ours.join(format!("{}.out", case.name)), &text).unwrap();
                if update {
                    fs::create_dir_all(kit.expected_dir()).unwrap();
                    fs::write(kit.expected_dir().join(format!("{}.out", case.name)), &text).unwrap();
                }
            }
            Err(e) => failures.push(e),
        }
    }
    assert!(failures.is_empty(), "programs that did not run:\n{}", failures.join("\n"));
    let c = compare(&ours, &kit.expected_dir());
    eprintln!("{}", c.render());
    assert!(c.mismatched.is_empty(), "outcomes differ from the expected files:\n{}", c.render());
    assert!(
        c.missing.is_empty(),
        "programs without an expected file: {:?}; QS400_UPDATE_IFSAPI=1 cargo test -p conformance --test ifs_api",
        c.missing
    );
}

/// Every RPG probe has its IR program, and the other way round.
#[test]
fn every_program_has_its_rpg_probe() {
    let kit = Kit::locate();
    let ir: Vec<String> = kit.cases().unwrap().into_iter().map(|c| c.name).collect();
    let mut rpg: Vec<String> = fs::read_dir(kit.root.join("rpg"))
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "rpgle").then(|| p.file_stem().unwrap().to_string_lossy().to_uppercase())
        })
        .collect();
    rpg.sort();
    assert_eq!(ir, rpg);
}

/// Printed or shown lines as both sides compare them: control
/// characters as blanks, trailing blanks and empty lines left out.
fn normal(lines: impl Iterator<Item = String>) -> Vec<String> {
    lines
        .map(|l| l.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// A spooled line's text without its `page/line ` prefix.
fn text_of(l: &str) -> String {
    let Some((_, rest)) = l.split_once('/') else { return String::new() };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    rest.get(digits.max(3) + 1..).unwrap_or("").to_string()
}

/// Every RPG probe compiles with QS/400's compiler and, run over the
/// same IFS in memory, prints what its IR program shows: the probes
/// the oracle runs ask what the IR corpus asks.
#[test]
fn the_rpg_probes_print_what_the_ir_shows() {
    use std::rc::Rc;
    let kit = Kit::locate();
    let mut failures = Vec::new();
    for case in kit.cases().unwrap() {
        let path = kit.root.join("rpg").join(format!("{}.rpgle", case.name));
        let rc = rpg_oracle::Case::load(&path).unwrap();
        conformance::rpg::set_job_setup(Some(Rc::new(|j: &mut mi::Job| {
            let sys = sysapi::ifs::memory::MemIfs::new();
            sysapi::ifs::configure(j, Rc::new(sys.client("QSTEST", 1, 37)), 37);
        })));
        let r = conformance::rpg::run(&rc);
        conformance::rpg::set_job_setup(None);
        let (o, _, notes) = match r {
            Ok(x) => x,
            Err(e) => {
                failures.push(format!("{}: {e}", case.name));
                continue;
            }
        };
        if !o.created {
            let why: Vec<&str> = notes.lines().filter(|l| l.starts_with(';')).collect();
            failures.push(format!("{}: not created: {} {:?}\n{}", case.name, o.cause, o.diags, why.join("\n")));
            continue;
        }
        let printed = normal(
            o.spool
                .iter()
                .filter(|s| s.file == "QSYSPRT")
                .flat_map(|s| s.lines.iter())
                .map(|l| text_of(l)),
        );
        let expected = fs::read_to_string(kit.expected_dir().join(format!("{}.out", case.name))).unwrap();
        let shown = normal(
            expected
                .lines()
                .take_while(|l| !l.starts_with("descriptors left open:"))
                // perror writes to standard error, not QSYSPRT.
                .filter(|l| !l.starts_with(&format!("{}: ", case.name)))
                .map(str::to_string),
        );
        if printed != shown {
            let at = printed.iter().zip(&shown).position(|(a, b)| a != b).unwrap_or(printed.len().min(shown.len()));
            failures.push(format!(
                "{}: line {at}: printed {:?}, the IR shows {:?} ({:?})",
                case.name,
                printed.get(at..(at + 3).min(printed.len())),
                shown.get(at..(at + 3).min(shown.len())),
                o.run
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

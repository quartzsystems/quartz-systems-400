// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security oracle: the machine's observations under
//! `conformance/oracle/sec/expected/sec-oracle.txt`, when captured,
//! against this implementation's readings of the same open questions.

use std::fs;
use std::path::Path;

/// What this implementation observes, keyed by the question tag and
/// the start of the line; whitespace runs fold before comparing.
fn ours() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Q1", "QSYSOPR *JOBCTL *SAVSYS"),
        ("Q1", "QPGMR *JOBCTL *SAVSYS"),
        ("Q1", "QSRV *JOBCTL *SAVSYS *SERVICE"),
        ("Q1", "QTCP *JOBCTL *IOSYSCFG"),
        ("Q2", "adopt through owner's group: refused"),
        ("Q3", "owner reduced, group *ALL: refused"),
        ("Q4", "CHGPWD rule failure: CPF4AA7"),
        ("Q5", "CRTUSRPRF without *SECADM: CPF2222"),
        ("Q6", "new profile owner QSECOFR"),
        ("Q7", "CHGJOB RUNPTY(10) with PTYLMT(5): 50"),
        ("Q9", "ADDPFM with *OBJALTER only: refused"),
        ("Q11", "QSYCHGPR: absent"),
        ("Q12", "DLTUSRPRF owner: CPF2204"),
        ("Q12", "ADDAUTLE duplicate: CPF2272"),
        ("Q12", "RMVAUTLE absent: CPF2270"),
        ("Q12", "GRTOBJAUT beyond own: CPF2227"),
    ]
}

fn fold(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn machine_observations_match_when_captured() {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("oracle")
        .join("sec")
        .join("expected")
        .join("sec-oracle.txt");
    let Ok(text) = fs::read_to_string(&p) else {
        eprintln!("skipped: {} not captured yet", p.display());
        return;
    };
    let machine: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(a, b)| (a.trim().to_string(), fold(b)))
        .collect();
    let mut differ = Vec::new();
    for (tag, line) in ours() {
        // The machine's line for the same question and subject.
        let subject = line.split(':').next().unwrap_or(line);
        let subject = fold(subject.split_whitespace().next().unwrap_or(subject));
        let found = machine
            .iter()
            .find(|(t, l)| t == tag && l.starts_with(&subject));
        match found {
            Some((_, l)) if *l == fold(line) => {}
            Some((_, l)) => differ.push(format!("{tag}: ours `{line}`, machine `{l}`")),
            None => differ.push(format!("{tag}: ours `{line}`, machine has no such line")),
        }
    }
    assert!(
        differ.is_empty(),
        "readings the machine settles differently:\n{}",
        differ.join("\n")
    );
}

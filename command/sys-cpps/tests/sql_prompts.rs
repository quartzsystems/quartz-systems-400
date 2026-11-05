// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL tools' command prompts against pub400's captures under
//! `tools/conformance/golden/sql/prompts/` (V7R5M0,
//! `tools/conformance/probes/sql/prompts*.tn`), every page with F10's
//! parameters. Rows 5 to 20 are compared: each row's prompt text and
//! every value the capture shows; the title, keys and columns are the
//! prompter's own (as `rpg3_prompts.rs`).

use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_rows(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/sql/prompts")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The parameter rows of a display: rows 5 to 20, blank rows, list
/// markers, the `...` of a long value and the Additional Parameters
/// title aside (where the prompter spaces them is its own).
fn rows(v: &[String]) -> Vec<String> {
    v.iter()
        .take(20)
        .skip(4)
        .map(|r| r.trim_end().to_string())
        .filter(|r| {
            let t = r.trim();
            !t.is_empty() && !t.starts_with('+') && t != "..." && t != "Additional Parameters"
        })
        .collect()
}

/// A row's prompt text: to column 34, dot leaders aside.
fn label(r: &str) -> String {
    r.chars()
        .take(34)
        .collect::<String>()
        .trim_end_matches(['.', ' ', ':'])
        .trim_start()
        .to_string()
}

/// The parameter rows of the prompt of `text`, the screens after each
/// key set in order, against the captures' rows in order: where a
/// page breaks is the prompter's own.
fn check(names: &[&str], text: &str, keys: &[&[u8]]) {
    let mut src = source();
    let mut ours: Vec<String> = Vec::new();
    for ks in keys {
        let r = {
            let mut env = Env::new(&mut src);
            env.setting.interactive = true;
            analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
        };
        let mut p = Prompter::new(&r);
        for k in *ks {
            let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
            let mut env = Env::new(&mut src);
            env.setting.interactive = true;
            assert_eq!(p.key(*k, &fields, &mut env), Step::Show, "{text}: key {k}");
        }
        for r in rows(&p.screen().rows) {
            if !ours.contains(&r) {
                ours.push(r);
            }
        }
    }
    let mut want: Vec<String> = Vec::new();
    for n in names {
        for r in rows(&golden_rows(n)) {
            if !want.contains(&r) {
                want.push(r);
            }
        }
    }
    let mut diffs = Vec::new();
    for (i, w) in want.iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let values: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| *t != ">" && !t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            .collect();
        let tail = got.get(34..).unwrap_or("").to_ascii_uppercase();
        if label(got) != label(w) || !values.iter().all(|v| tail.contains(&v.to_ascii_uppercase())) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{text}:\n{}", diffs.join("\n"));
}

#[test]
fn runsql() {
    check(&["prompt-runsql"], "?RUNSQL", &[&[]]);
    check(
        &["prompt-runsql-v-f10", "prompt-runsql-v-2", "prompt-runsql-v-3"],
        "?RUNSQL SQL('values 1')",
        &[&[10], &[10, 8], &[10, 8, 8]],
    );
}

#[test]
fn runsqlstm() {
    check(&["prompt-runsqlstm"], "?RUNSQLSTM", &[&[]]);
    check(
        &["prompt-runsqlstm-f10", "prompt-runsqlstm-f10-2", "prompt-runsqlstm-f10-3"],
        "?RUNSQLSTM",
        &[&[10], &[10, 8], &[10, 8, 8]],
    );
}

#[test]
fn crtsqlrpgi() {
    check(&["prompt-crtsqlrpgi"], "?CRTSQLRPGI", &[&[]]);
    check(
        &[
            "prompt-crtsqlrpgi-v-f10",
            "prompt-crtsqlrpgi-v-2",
            "prompt-crtsqlrpgi-v-3",
            "prompt-crtsqlrpgi-v-4",
            "prompt-crtsqlrpgi-v-5",
        ],
        "?CRTSQLRPGI OBJ(QTEMP/X)",
        &[&[10], &[10, 8], &[10, 8, 8], &[10, 8, 8, 8], &[10, 8, 8, 8, 8]],
    );
}

#[test]
fn strsql() {
    check(&["prompt-strsql"], "?STRSQL", &[&[]]);
}

// BLOCKED(sql): the definitions of these commands come with their
// processing (SQL-11: STRDBMON, ENDDBMON, PRTSQLINF; SQL-13:
// DLTSQLPKG); the captures are here for them.
#[test]
#[ignore = "BLOCKED(sql): STRDBMON is defined with the database monitor"]
fn strdbmon() {
    check(
        &["prompt-strdbmon-v", "prompt-strdbmon-v-2", "prompt-strdbmon-v-3", "prompt-strdbmon-v-4"],
        "?STRDBMON OUTFILE(QTEMP/X)",
        &[&[], &[8], &[8, 8], &[8, 8, 8]],
    );
}

#[test]
#[ignore = "BLOCKED(sql): ENDDBMON is defined with the database monitor"]
fn enddbmon() {
    check(&["prompt-enddbmon"], "?ENDDBMON", &[&[]]);
}

#[test]
#[ignore = "BLOCKED(sql): PRTSQLINF is defined with its engine"]
fn prtsqlinf() {
    check(&["prompt-prtsqlinf"], "?PRTSQLINF", &[&[]]);
}

#[test]
#[ignore = "BLOCKED(sql): DLTSQLPKG is defined with *SQLPKG"]
fn dltsqlpkg() {
    check(&["prompt-dltsqlpkg"], "?DLTSQLPKG", &[&[]]);
}

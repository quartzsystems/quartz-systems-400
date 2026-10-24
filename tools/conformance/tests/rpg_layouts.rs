// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The status data structures in the code must match the tables in
//! `docs/RPG.md` ("Status data structures").

use std::path::Path;

/// A table row: position from 1, length (`None` for variable), name, word.
#[derive(Debug, PartialEq)]
struct Row {
    pos: usize,
    len: Option<usize>,
    name: String,
    word: String,
}

/// The rows of the table under the heading naming `title`.
fn table(md: &str, title: &str) -> Vec<Row> {
    let heading = format!("### `{title}`");
    let mut lines = md.lines().skip_while(|l| *l != heading).skip(1);
    let mut rows = Vec::new();
    for l in lines.by_ref().skip_while(|l| !l.starts_with("| Position")).skip(2) {
        if !l.starts_with('|') {
            break;
        }
        let c: Vec<&str> = l.split('|').map(str::trim).collect();
        rows.push(Row {
            pos: c[1].parse().expect("position"),
            len: c[2].parse().ok(),
            name: c[3].to_string(),
            word: c[4].trim_matches('`').to_string(),
        });
    }
    assert!(!rows.is_empty(), "no table for {title}");
    rows
}

fn doc() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/RPG.md");
    std::fs::read_to_string(p).expect("docs/RPG.md")
}

/// The named rows as `(name, offset from 0, length)`.
fn named(rows: &[Row]) -> Vec<(String, usize, usize)> {
    rows.iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| (r.name.clone(), r.pos - 1, r.len.unwrap_or(0)))
        .collect()
}

/// The rows with a special word as `(word, offset from 0, length)`.
fn words(rows: &[Row]) -> Vec<(String, u32, u32)> {
    rows.iter()
        .filter(|r| !r.word.is_empty())
        .map(|r| {
            let len = r.len.expect("a word's field has a length");
            (r.word.clone(), u32::try_from(r.pos - 1).unwrap(), u32::try_from(len).unwrap())
        })
        .collect()
}

#[test]
fn infds_matches_the_spec() {
    let rows = table(&doc(), "INFDS");
    let code: Vec<_> = mi::io::infds::FIELDS.iter().map(|(n, (o, l))| ((*n).to_string(), *o, *l)).collect();
    assert_eq!(named(&rows), code, "mi::io::infds against docs/RPG.md");
    let code: Vec<_> = rpgc::extfiles::INFDS_WORDS.iter().map(|(w, o, l, _)| ((*w).to_string(), *o, *l)).collect();
    assert_eq!(words(&rows), code, "INFDS_WORDS against docs/RPG.md");
}

#[test]
fn psds_matches_the_spec() {
    let rows = table(&doc(), "PSDS");
    let code: Vec<_> = qrpgrt::exc::PSDS.iter().map(|(n, (p, l))| ((*n).to_string(), p - 1, *l)).collect();
    assert_eq!(named(&rows), code, "qrpgrt::exc::PSDS against docs/RPG.md");
    let code: Vec<_> = rpgc::extfiles::PSDS_WORDS.iter().map(|(w, o, l, _)| ((*w).to_string(), *o, *l)).collect();
    assert_eq!(words(&rows), code, "PSDS_WORDS against docs/RPG.md");
}

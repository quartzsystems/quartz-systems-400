// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `docs/DDM.md` against the code: every table after a
//! `<!-- consts: KEY -->` marker lists exactly the constants of KEY, and
//! every table after `<!-- layout: KEY -->` the fields of that layout at
//! the same offsets and sizes.

use std::collections::BTreeMap;
use std::path::Path;

use ddm::cp;
use ddm::fdoca::{desc, types};
use ddm::file::{feedback, ufcb};
use ddm::msg::pkg;
use ddm::wire::dss;

type Consts = Vec<(String, u64)>;

fn consts() -> BTreeMap<&'static str, Consts> {
    let w = |t: &[(&str, u16)]| {
        t.iter()
            .map(|(n, v)| ((*n).to_string(), u64::from(*v)))
            .collect::<Vec<_>>()
    };
    BTreeMap::from([
        ("cp::cmd", w(cp::cmd::ALL)),
        ("cp::rpy", w(cp::rpy::ALL)),
        ("cp::rm", w(cp::rm::ALL)),
        ("cp::obj", w(cp::obj::ALL)),
        ("cp::param", w(cp::param::ALL)),
        ("cp::mgr", w(cp::mgr::ALL)),
        ("cp::s38", w(cp::s38::ALL)),
        ("cp::secmec", w(cp::secmec::ALL)),
        ("cp::secchkcd", w(cp::secchkcd::ALL)),
        ("cp::svrcod", w(cp::svrcod::ALL)),
        ("types::drda", w(types::drda::ALL)),
        ("types::sqltype", w(types::sqltype::ALL)),
        ("desc::fd", w(desc::fd::ALL)),
    ])
}

fn layouts() -> BTreeMap<&'static str, &'static [(usize, usize, &'static str)]> {
    BTreeMap::from([
        ("dss::header", dss::HEADER_LAYOUT),
        ("pkg::pkgnamcsn", pkg::LAYOUT),
        ("ufcb::fixed", ufcb::LAYOUT),
        ("feedback::opnfb", feedback::OPNFB_LAYOUT),
        ("feedback::iofb", feedback::IOFB_LAYOUT),
    ])
}

fn doc() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/DDM.md")).expect("docs/DDM.md")
}

/// The table rows (cells, trimmed, backticks removed) after each marker.
fn tables(doc: &str, kind: &str) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut out = BTreeMap::new();
    let lines: Vec<&str> = doc.lines().collect();
    let prefix = format!("<!-- {kind}: ");
    for (i, l) in lines.iter().enumerate() {
        let Some(key) = l.trim().strip_prefix(&prefix).and_then(|r| r.strip_suffix(" -->")) else {
            continue;
        };
        let rows: Vec<Vec<String>> = lines[i + 1..]
            .iter()
            .skip_while(|l| l.trim().is_empty())
            .take_while(|l| l.trim_start().starts_with('|'))
            .skip(2)
            .map(|l| {
                l.trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().replace('`', ""))
                    .collect()
            })
            .collect();
        assert!(out.insert(key.to_string(), rows).is_none(), "{kind} {key} appears twice");
    }
    out
}

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or_else(|_| panic!("not hex: {s}"))
}

#[test]
fn code_points_match_the_doc() {
    let doc = tables(&doc(), "consts");
    let code = consts();
    for (key, list) in &code {
        let rows = doc
            .get(*key)
            .unwrap_or_else(|| panic!("docs/DDM.md has no consts table for {key}"));
        let mut d: Vec<(String, u64)> = rows.iter().map(|r| (r[1].clone(), hex(&r[0]))).collect();
        let mut c = list.clone();
        d.sort();
        c.sort();
        assert_eq!(d, c, "consts table {key} differs from the code");
    }
    for key in doc.keys() {
        assert!(code.contains_key(key.as_str()), "docs/DDM.md lists unknown consts {key}");
    }
}

#[test]
fn layouts_match_the_doc() {
    let doc = tables(&doc(), "layout");
    let code = layouts();
    for (key, fields) in &code {
        let rows = doc
            .get(*key)
            .unwrap_or_else(|| panic!("docs/DDM.md has no layout table for {key}"));
        let d: Vec<(usize, usize, String)> = rows
            .iter()
            .map(|r| (r[0].parse().unwrap(), r[1].parse().unwrap(), r[2].clone()))
            .collect();
        let c: Vec<(usize, usize, String)> = fields.iter().map(|f| (f.0, f.1, f.2.to_string())).collect();
        assert_eq!(d, c, "layout {key} differs from the code");
    }
    for key in doc.keys() {
        assert!(code.contains_key(key.as_str()), "docs/DDM.md lists unknown layout {key}");
    }
}

/// Print the tables the doc needs (`cargo test -p ddm --test spec -- --ignored --nocapture`).
#[test]
#[ignore = "prints the doc tables"]
fn print_tables() {
    for (key, list) in consts() {
        println!("<!-- consts: {key} -->\n\n| Value | Name |\n|---|---|");
        for (n, v) in list {
            println!("| `0x{v:04X}` | `{n}` |");
        }
        println!();
    }
    for (key, fields) in layouts() {
        println!("<!-- layout: {key} -->\n\n| Offset | Size | Field |\n|---|---|---|");
        for f in fields {
            println!("| {} | {} | `{}` |", f.0, f.1, f.2);
        }
        println!();
    }
}

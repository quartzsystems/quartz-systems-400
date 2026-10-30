// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code's layouts and parameter lists against the tables of
//! `docs/HOSTSVR.md` (§ Exit points). A table after `<!-- regfac: NAME
//! -->` lists layout `NAME`'s fields; one after `<!-- regfac-parms:
//! NAME -->` the parameters of format `NAME`. `cargo test -p regfac
//! --test spec -- --ignored --nocapture` prints them from the code.

use std::collections::BTreeMap;
use std::path::Path;

use regfac::formats::{ParmList, lists};
use regfac::spec::{Layout, layouts};

fn doc() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/HOSTSVR.md");
    std::fs::read_to_string(&path).expect("docs/HOSTSVR.md")
}

/// Each marked table's rows, by marker kind and name.
fn tables(md: &str, marker: &str) -> BTreeMap<String, (Option<usize>, Vec<Vec<String>>)> {
    let mut out = BTreeMap::new();
    let mut lines = md.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix(&format!("<!-- {marker}: ")) else {
            continue;
        };
        let name = rest.trim_end_matches("-->").trim().to_string();
        let mut size = None;
        for l in lines.by_ref() {
            if let Some(h) = l.strip_prefix("#### `")
                && let Some((_, tail)) = h.split_once('`')
            {
                size = tail
                    .trim()
                    .strip_prefix('(')
                    .and_then(|t| t.split_whitespace().next())
                    .and_then(|n| n.parse().ok());
            }
            if l.starts_with("| ") && !l.starts_with("|--") {
                break;
            }
        }
        lines.next();
        let mut rows = Vec::new();
        while let Some(row) = lines.peek() {
            if !row.starts_with('|') {
                break;
            }
            rows.push(
                row.trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().trim_matches('`').to_string())
                    .collect(),
            );
            lines.next();
        }
        out.insert(name, (size, rows));
    }
    out
}

fn layout_rows(l: &Layout) -> Vec<Vec<String>> {
    l.fields
        .iter()
        .map(|f| vec![f.offset.to_string(), f.size.to_string(), f.name.to_string()])
        .collect()
}

fn parm_rows(p: &ParmList) -> Vec<Vec<String>> {
    p.parms
        .iter()
        .map(|q| {
            vec![
                q.n.to_string(),
                q.usage.name().into(),
                q.ty.into(),
                q.name.into(),
            ]
        })
        .collect()
}

#[test]
fn layouts_match_the_tables() {
    let t = tables(&doc(), "regfac");
    let code = layouts();
    assert!(code.len() > 20);
    for l in &code {
        let (size, rows) = t
            .get(l.name)
            .unwrap_or_else(|| panic!("docs/HOSTSVR.md has no table for {}", l.name));
        assert_eq!(*size, Some(l.size), "{}: size", l.name);
        let rows: Vec<Vec<String>> = rows.iter().map(|r| r[..3].to_vec()).collect();
        assert_eq!(rows, layout_rows(l), "{}: fields", l.name);
        // No field runs past the fixed part.
        for f in l.fields {
            assert!(
                f.offset + f.size <= l.size,
                "{}.{} past the end",
                l.name,
                f.name
            );
        }
    }
    assert_eq!(t.len(), code.len(), "tables without a layout");
}

#[test]
fn parameter_lists_match_the_tables() {
    let t = tables(&doc(), "regfac-parms");
    for p in lists() {
        let (_, rows) = t
            .get(p.name)
            .unwrap_or_else(|| panic!("docs/HOSTSVR.md has no parameter table for {}", p.name));
        assert_eq!(rows, &parm_rows(p), "{}", p.name);
    }
    assert_eq!(t.len(), lists().len());
}

#[test]
fn structure_sizes_are_ibms() {
    use regfac::formats::host;
    // From the QSYSINC headers' structures (QRPGLESRC positions).
    for (l, n) in [
        (&host::ZSOY0100, 32),
        (&host::ZSCL0100, 314),
        (&host::ZSCN0100, 42),
        (&host::ZDAQ0100, 865),
        (&host::ZDAQ0200, 238),
        (&host::ZHQ00100, 314),
        (&host::PWFS0100, 40),
        (&host::PWFS0200, 132),
        (&host::CZRC0100, 56),
        (&regfac::exti::EXTI0200, 76),
        (&regfac::exti::EXTI0300, 156),
    ] {
        assert_eq!(l.size, n, "{}", l.name);
    }
    assert_eq!(regfac::exti::EXTI0100.get("TEXT").offset, 151);
    assert_eq!(regfac::exti::EXTI0300.get("OFFDTA").offset, 144);
    assert_eq!(
        host::PWFS0100.get("FUNCID").offset,
        20,
        "the file server puts the function before the format"
    );
}

#[test]
#[ignore = "prints the tables for docs/HOSTSVR.md"]
fn print_tables() {
    for l in layouts() {
        println!(
            "<!-- regfac: {} -->\n#### `{}` ({} bytes)\n",
            l.name, l.name, l.size
        );
        println!("| Offset | Size | Field |\n|---|---|---|");
        for f in l.fields {
            println!("| {} | {} | `{}` |", f.offset, f.size, f.name);
        }
        println!();
    }
    for p in lists() {
        println!(
            "<!-- regfac-parms: {} -->\n#### `{}` parameters\n",
            p.name, p.name
        );
        println!("| Parm | Usage | Type | Field |\n|---|---|---|---|");
        for q in p.parms {
            println!("| {} | {} | `{}` | {} |", q.n, q.usage.name(), q.ty, q.name);
        }
        println!();
    }
}

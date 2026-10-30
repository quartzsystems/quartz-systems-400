// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUIENVVAR` against pub400's `WRKENVVAR` captures at both levels
//! (`tools/conformance/golden/phase10/misc/wrkenvvar*.txt`). The entries
//! come from the golden rows, cut as the driver cuts them; row 24 (the
//! vendor's copyright line) is not compared.

mod common;

use std::fmt::Write as _;
use std::path::Path;

use common::shipped;
use uim::{ListState, MapDialog, Options, render};

fn golden(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase10/misc")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

fn cut(row: &str, from: usize, to: usize) -> String {
    row.chars()
        .skip(from)
        .take(to - from)
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn check(name: &str, panel: &str) {
    let rows = golden(name);
    // The blank row option 1 adds from, then the entries.
    let mut list = vec![vec![
        ("OPT".to_string(), String::new()),
        ("EVNAME".to_string(), String::new()),
        ("EVVALUE".to_string(), String::new()),
    ]];
    for r in &rows[7..18] {
        let n = cut(r, 6, 32);
        if n.trim().is_empty() {
            continue;
        }
        list.push(vec![
            ("OPT".to_string(), String::new()),
            ("EVNAME".to_string(), n),
            ("EVVALUE".to_string(), cut(r, 34, 76)),
        ]);
    }
    let total = list.len();
    let mut d = MapDialog::new();
    d.set_list(
        "VARS",
        ListState {
            rows: list,
            top: 0,
            total: Some(total),
            complete: true,
        },
    );
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&shipped("QUIENVVAR"), panel, &d, &opts).unwrap();
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in rows.iter().enumerate().take(23) {
        let ours = got.get(i).map_or("", String::as_str);
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(
                diff,
                "row {}:\n  want |{}|\n  got  |{}|",
                i + 1,
                w.trim_end(),
                ours.trim_end()
            );
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

#[test]
fn wrkenvvar_job() {
    check("wrkenvvar", "WRKENVJOB");
}

#[test]
fn wrkenvvar_sys() {
    check("wrkenvvar-sys", "WRKENVSYS");
}

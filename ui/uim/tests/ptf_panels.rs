// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUIDSPPTF` against the V7R1M0 captures of `DSPPTF` and
//! `WRKPTFGRP` (`golden/verified/oracle71/audit`): each panel rendered
//! with the golden's values, compared row by row, its input fields
//! (row, column, length) with the golden's `.attr` grid.

mod objcommon;

use std::collections::BTreeSet;

use objcommon::{compare, golden_dir, shipped};
use uim::{ListState, MapDialog, Options, Screen, render};

/// The 7.1 captures, from the pub400 golden directory.
const DIR: &str = "../oracle71/audit";

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

/// A list of entries, more of them below.
fn list(rows: Vec<Vec<(&str, &str)>>, complete: bool) -> ListState {
    let n = rows.len();
    ListState {
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
            .collect(),
        top: 0,
        total: complete.then_some(n),
        complete,
    }
}

/// The input fields of a golden: an attribute byte (`#`) followed by
/// cells with an input attribute, as (row, column, length), 1-based.
fn golden_fields(name: &str) -> BTreeSet<(usize, usize, usize)> {
    let path = golden_dir().join(format!("{DIR}/{name}.attr"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let cells: Vec<char> = text
        .replace("\r\n", "\n")
        .lines()
        .flat_map(|l| format!("{l:<80}").chars().collect::<Vec<_>>())
        .collect();
    let input = |c: char| {
        let a = u32::from(c).checked_sub(u32::from('0')).map_or(0, |d| d + 0x20);
        (0x20..=0x3f).contains(&a) && a & 0x04 != 0 && a & 0x07 != 0x07
    };
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i + 1 < cells.len() {
        if cells[i] == '#' && input(cells[i + 1]) {
            let mut n = 0;
            while cells.get(i + 1 + n).copied().is_some_and(input) {
                n += 1;
            }
            out.insert(((i + 1) / 80 + 1, (i + 1) % 80 + 1, n));
            i += n + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The screen's input fields.
fn fields(s: &Screen) -> BTreeSet<(usize, usize, usize)> {
    s.fields
        .iter()
        .filter(|f| f.usage.is_input())
        .map(|f| (f.row, f.col, f.len))
        .collect()
}

/// Rows and input fields against golden `name`.
fn check(name: &str, s: &Screen) {
    compare(&format!("{DIR}/{name}"), s);
    assert_eq!(fields(s), golden_fields(name), "{name}: input fields");
}

#[test]
fn dspptf() {
    let g = shipped("QUIDSPPTF");
    let mut d = MapDialog::new();
    d.set("PRODID", "5770999")
        .set("IPLSRC", "##MACH#B  ##SERV#P")
        .set("RELEASE", "V7R1M0 L00");
    let ids = ["RE16082", "RE16053", "RE16032", "RE15349", "RE15279", "RE15097", "RE14245", "RE14197"];
    let rows = ids
        .iter()
        .map(|id| vec![("PTFID", *id), ("PSTS", "Permanently applied"), ("PACT", "None")])
        .collect();
    d.set_list("PTFS", list(rows, false));
    check("dspptf", &render(&g, "DSPPTF", &d, &opts()).unwrap());
}

#[test]
fn wrkptfgrp() {
    let g = shipped("QUIDSPPTF");
    let mut d = MapDialog::new();
    d.set_list("GRPS", list(Vec::new(), true));
    check("wrkptfgrp", &render(&g, "WRKPTFGRP", &d, &opts()).unwrap());
}

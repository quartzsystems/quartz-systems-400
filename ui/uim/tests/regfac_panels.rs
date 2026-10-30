// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUIREGINF` against the Phase 10 goldens from pub400
//! (`tools/conformance/golden/phase10/misc/`): `WRKREGINF` on both
//! pages captured, filtered to one point, its option 5 (`DSPEXITPNT`)
//! and option 8 (`WRKEXITPGM`). The values come from the golden rows
//! themselves, so no IBM text is written here.

mod common;

use std::fmt::Write as _;
use std::path::Path;

use common::shipped;
use uim::{ListState, MapDialog, Options, Screen, render};

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
        .trim()
        .to_string()
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

/// Our rows against the golden's, row by row; `loose` rows compare
/// their words only.
fn compare(name: &str, s: &Screen, loose: &[usize], skip: &[usize]) {
    let want = golden(name);
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let ours = got.get(i).map_or("", String::as_str);
        if skip.contains(&(i + 1)) {
            continue;
        }
        let same = if loose.contains(&(i + 1)) {
            w.split_whitespace().eq(ours.split_whitespace())
        } else {
            w.trim_end() == ours.trim_end()
        };
        if !same {
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

/// The exit point list rows of a `WRKREGINF` golden.
fn points(rows: &[String]) -> ListState {
    let mut list = Vec::new();
    for r in &rows[8..19] {
        let name = cut(r, 6, 26);
        if name.is_empty() {
            continue;
        }
        let reg = cut(r, 38, 48);
        list.push(vec![
            ("OPT".to_string(), String::new()),
            ("EPNAME".to_string(), name),
            ("EPFMT".to_string(), cut(r, 28, 36)),
            ("EPREGD".to_string(), format!("   {reg}")),
            ("EPTEXT".to_string(), cut(r, 50, 80)),
        ]);
    }
    let n = list.len();
    ListState {
        rows: list,
        top: 0,
        total: Some(n),
        complete: true,
    }
}

fn wrkreginf(name: &str, more: bool) {
    let g = shipped("QUIREGINF");
    let rows = golden(name);
    let mut l = points(&rows);
    if more {
        l.total = None;
        l.complete = false;
    }
    let mut d = MapDialog::new();
    d.set_list("POINTS", l);
    compare(
        name,
        &render(&g, "WRKREGINF", &d, &opts()).unwrap(),
        &[],
        &[],
    );
}

#[test]
fn wrkreginf_page_one() {
    wrkreginf("wrkreginf", true);
}

#[test]
fn wrkreginf_page_two() {
    wrkreginf("wrkreginf-2", true);
}

#[test]
fn wrkreginf_one_point() {
    wrkreginf("wrkreginf-qtmf", false);
}

#[test]
fn wrkreginf_option_5_displays_the_point() {
    let g = shipped("QUIREGINF");
    let rows = golden("wrkreginf-qtmf-5");
    let v = |r: usize| cut(&rows[r - 1], 44, 80);
    let sub = |r: usize| cut(&rows[r - 1], 46, 80);
    let mut d = MapDialog::new();
    d.set("EPNAME", &v(3))
        .set("EPFMT", &v(4))
        .set("EPREG", &v(6))
        .set("EPDEREG", &v(7))
        .set("EPMAX", &v(8))
        .set("EPCOUNT", &v(9))
        .set("EPADDP", &v(10))
        .set("EPADDL", &sub(11))
        .set("EPADDF", &sub(12))
        .set("EPRMVP", &v(13))
        .set("EPRMVL", &sub(14))
        .set("EPRMVF", &sub(15))
        .set("EPRTVP", &v(16))
        .set("EPRTVL", &sub(17))
        .set("EPRTVF", &sub(18))
        .set("EPCHG", &v(19))
        .set("EPTEXT", "Quartz text on the second page");
    compare(
        "wrkreginf-qtmf-5",
        &render(&g, "DSPEXITPNT", &d, &opts()).unwrap(),
        &[],
        &[],
    );
}

#[test]
fn wrkreginf_option_8_works_with_the_programs() {
    let g = shipped("QUIREGINF");
    let rows = golden("wrkreginf-qtmf-8");
    let mut d = MapDialog::new();
    d.set("EPNAME", &cut(&rows[2], 15, 40))
        .set("EPFMT", &cut(&rows[2], 50, 60));
    d.set_list(
        "PGMS",
        ListState {
            rows: Vec::new(),
            top: 0,
            total: Some(0),
            complete: true,
        },
    );
    // IBM's column headings here are not laid out by UIM's rules (the
    // first row is centered over a number, the columns are wider than
    // their fields): those rows match by words. IBM shows the empty
    // list's text one row lower than its other lists do.
    let s = render(&g, "WRKEXITPGM", &d, &opts()).unwrap();
    compare("wrkreginf-qtmf-8", &s, &[8, 9, 10], &[12, 13]);
    let want = golden("wrkreginf-qtmf-8");
    assert_eq!(
        s.text_rows()[11].trim(),
        want[12].trim(),
        "the empty list's text"
    );
}

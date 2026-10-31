// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The panels the CPP audit's second round gave the displays that had
//! only the report viewer (`docs/CPP_AUDIT.md`): each against its
//! oracle capture row by row where one exists (DSPLICKEY from 7.1,
//! DSPBCKSTS and WRKOPTVOL from pub400), the message line aside, with
//! the input fields from the `.attr` grid; the panels no oracle could
//! show (no cover letter, tape, library, optical volume or UDFS there)
//! against goldens of our own rendering under `tests/golden/audit/`
//! (`QS400_UPDATE_GOLDEN=1` rewrites them).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, Screen, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name}:\n{}", c.listing);
    c.group.expect("a group")
}

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden")
}

fn read(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| format!("{l:<80}"))
        .collect()
}

fn opts(page: usize) -> Options {
    Options {
        system: "SYSNAME".into(),
        page,
        ..Options::default()
    }
}

fn list(rows: Vec<Vec<(&str, &str)>>) -> ListState {
    ListState {
        total: Some(rows.len()),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
            .collect(),
        top: 0,
        complete: true,
    }
}

/// The input fields of a capture's `.attr` grid, (row, column, length).
fn golden_fields(attr: &[String]) -> BTreeSet<(usize, usize, usize)> {
    let cells: Vec<char> = attr.iter().flat_map(|r| r.chars()).collect();
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

/// Rows 1 to 23 of capture `rel` (under the golden root) against the
/// screen, then its input fields; `skip` rows are left out (a row the
/// capture's data has and ours does not, said where it is called).
fn against(rel: &str, s: &Screen, skip: &[usize]) {
    let want = read(&goldens().join(format!("{rel}.txt")));
    let ours = s.text_rows();
    let mut diff = String::new();
    for (i, (w, o)) in want.iter().zip(&ours).enumerate().take(23) {
        if skip.contains(&(i + 1)) {
            continue;
        }
        if o.trim_end() != w.trim_end() {
            diff.push_str(&format!("row {:>2}\n  want |{}|\n  got  |{}|\n", i + 1, w.trim_end(), o.trim_end()));
        }
    }
    assert!(diff.is_empty(), "{rel}:\n{diff}");
    let attr = read(&goldens().join(format!("{rel}.attr")));
    let fields: BTreeSet<(usize, usize, usize)> =
        s.fields.iter().filter(|f| f.usage.is_input()).map(|f| (f.row, f.col, f.len)).collect();
    assert_eq!(fields, golden_fields(&attr), "{rel}: input fields");
}

/// Our own rendering against `tests/golden/audit/NAME.txt`.
fn ours(name: &str, s: &Screen) {
    let rows = s.text_rows();
    assert_eq!(rows.len(), 24, "{name}: 24 rows");
    let text = rows.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n") + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/audit/{name}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("a parent")).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_GOLDEN=1 writes it)", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(text, want, "{name} differs from its golden");
}

// ------------------------------------------------------------ DSPLICKEY

/// `DSPLICKEY` with the 7.1 oracle's key (serial and key masked as the
/// capture masks them), row for row.
#[test]
fn dsplickey_matches_the_71_capture() {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("*DATETIME", "MM/DD/YY  HH:MM:SS"),
        ("PRDID", "5770SS1"),
        ("LICTRM", "V7R1M0"),
        ("FEATURE", "5050"),
        ("PRDTEXT", "IBM i"),
        ("SERIAL", " SERIAL1"),
        ("PRCGRP", "*ANY"),
        ("LICKEY", "XXXXXX XXXXXX XXXXXX"),
        ("USGLMT", "*NOMAX"),
        ("EXPDATE", "*NONE"),
        ("VNDDTA", "*NONE"),
    ] {
        d.set(k, v);
    }
    let s = render(&group("QUILICKEY"), "DSPLICKEY", &d, &opts(0)).unwrap();
    against("verified/oracle71/audit/dsplickey", &s, &[]);
}

// ------------------------------------------------------------ DSPPTFCVR

/// `DSPPTFCVR`'s list and letter: IBM's documentation only (the 7.1
/// oracle has no cover letter: `CPF6613`, `verified/oracle71/audit/
/// ids-dspptfcvr`; pub400 refuses the command).
#[test]
fn dspptfcvr_list_and_letter() {
    let g = group("QUIDSPPTF");
    let mut d = MapDialog::new();
    d.set_list(
        "CVRS",
        list(vec![
            vec![("PTFID", "QS00001"), ("PRODID", "5770QS1"), ("PTFRLS", "V1R0M0"), ("PTFSTS", "Loaded"), ("CVRATR", "*SPCINST")],
            vec![("PTFID", "QS00002"), ("PRODID", "5770QS1"), ("PTFRLS", "V1R0M0"), ("PTFSTS", "Temporarily applied"), ("CVRATR", "*DFT")],
        ]),
    );
    ours("selcvr", &render(&g, "SELCVR", &d, &opts(0)).unwrap());
    let mut d = MapDialog::new();
    d.set("PTFID", "QS00001").set("PRODID", "5770QS1").set("PTFRLS", "V1R0M0");
    d.set_list(
        "LINES",
        list(vec![
            vec![("CVRLINE", "Corrects the level of the object store check.")],
            vec![("CVRLINE", "")],
            vec![("CVRLINE", "SPECIAL INSTRUCTIONS: apply it with the system in restricted state.")],
        ]),
    );
    ours("dspcvr", &render(&g, "DSPCVR", &d, &opts(0)).unwrap());
}

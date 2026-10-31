// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 panels against pub400's captures under
//! `tools/conformance/golden/phase11/` by geometry, not words: the
//! input fields (row, column, length, from the `.attr` grid), where the
//! runs of the column heading rows start, the `n=` options and the
//! function keys with their rows, and the `More...`/`Bottom` place.
//! Each panel's differing parts are recorded (`COVERAGE.md` there says
//! why); a change either way fails until the record follows.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, Screen, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase11")
}

fn read(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| format!("{l:<80}"))
        .collect()
}

fn group(name: &str) -> PanelGroup {
    let text = uim::shipped::source(name).unwrap_or_else(|| panic!("{name} is not shipped"));
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name}:\n{}", c.listing);
    c.group.expect("a group")
}

/// Runs of a row split at two or more blanks: their first columns
/// (1-based) and texts.
fn runs(row: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = row.chars().collect();
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !(chars[i] == ' ' && chars.get(i + 1).is_none_or(|c| *c == ' ')) {
            i += 1;
        }
        out.push((start + 1, chars[start..i].iter().collect()));
    }
    out
}

/// The input fields of a golden: underlined, displayed runs after an
/// attribute byte, as (row, column, length), 1-based.
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

/// `(row, column, key)` of every `Fn=` on the rows from 21.
fn fkeys(rows: &[String]) -> BTreeSet<(usize, usize, String)> {
    rows.iter()
        .enumerate()
        .skip(20)
        .flat_map(|(r, t)| runs(t).into_iter().map(move |(c, s)| (r + 1, c, s)))
        .filter_map(|(r, c, s)| {
            let (k, _) = s.split_once('=')?;
            (k.starts_with('F') && k[1..].chars().all(|d| d.is_ascii_digit())).then(|| (r, c, k.to_string()))
        })
        .collect()
}

/// `(row, column, number)` of every `n=` option above row 21.
fn options(rows: &[String]) -> BTreeSet<(usize, usize, String)> {
    rows.iter()
        .enumerate()
        .take(20)
        .flat_map(|(r, t)| runs(t).into_iter().map(move |(c, s)| (r + 1, c, s)))
        .filter_map(|(r, c, s)| {
            // `5=Display   6=Print` is one run per option.
            s.split_once('=')
                .filter(|(n, _)| !n.is_empty() && n.chars().all(|d| d.is_ascii_digit()))
                .map(|(n, _)| (r, c, n.to_string()))
        })
        .collect()
}

fn more(rows: &[String]) -> Option<(usize, usize)> {
    rows.iter().enumerate().find_map(|(r, t)| {
        runs(t)
            .into_iter()
            .find(|(_, s)| s == "More..." || s == "Bottom")
            .map(|(c, _)| (r + 1, c))
    })
}

/// The parts of `screen` that differ from golden `dir/name`;
/// `headings` are the 1-based rows of column headings.
fn differing(dir: &str, name: &str, s: &Screen, headings: &[usize]) -> BTreeSet<String> {
    let d = golden_dir().join(dir);
    let text = read(&d.join(format!("{name}.txt")));
    let attr = read(&d.join(format!("{name}.attr")));
    let ours = s.text_rows();
    let mut out = BTreeSet::new();
    let fields: BTreeSet<(usize, usize, usize)> = s.fields.iter().map(|f| (f.row, f.col, f.len)).collect();
    let want = golden_fields(&attr);
    if want != fields {
        println!("{name} fields:\n  want {want:?}\n  got  {fields:?}");
        out.insert("fields".to_string());
    }
    for &h in headings {
        let cols = |r: &str| runs(r).into_iter().map(|(c, _)| c).collect::<Vec<_>>();
        if cols(&text[h - 1]) != cols(&ours[h - 1]) {
            println!("{name} row {h}:\n  want {:?}\n  got  {:?}", text[h - 1].trim_end(), ours[h - 1].trim_end());
            out.insert(format!("headings {h}"));
        }
    }
    if options(&text) != options(&ours) {
        println!("{name} options:\n  want {:?}\n  got  {:?}", options(&text), options(&ours));
        out.insert("options".to_string());
    }
    if fkeys(&text) != fkeys(&ours) {
        println!("{name} keys:\n  want {:?}\n  got  {:?}", fkeys(&text), fkeys(&ours));
        out.insert("keys".to_string());
    }
    if more(&text) != more(&ours) {
        println!("{name} more:\n  want {:?}\n  got  {:?}", more(&text), more(&ours));
        out.insert("more".to_string());
    }
    out
}

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn list(rows: Vec<Vec<(&str, &str)>>) -> ListState {
    ListState {
        total: Some(rows.len()),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
            .collect(),
        top: 0,
        complete: false,
    }
}

/// `WRKSFWAGR` with pub400's first page of products. Ours lists the
/// release and the agreement's status where pub400 lists the option;
/// pub400 shows the selected language, `6=Print`, and F11, F13, F19
/// and F22 (`COVERAGE.md`).
#[test]
fn wrksfwagr() {
    let products = [
        ("*MCHCOD", "Machine Code"),
        ("5770SS1", "IBM i"),
        ("5770SS1", "Media and Storage Extensions"),
        ("5770SS1", "DB2 Symmetric Multiprocessing"),
        ("5770SS1", "DB2 Multisystem"),
        ("5770SS1", "PSF 1-55 IPM Printer Support"),
        ("5770SS1", "PSF 1-100 IPM Printer Support"),
        ("5770SS1", "PSF Any Speed Printer Support"),
        ("5770SS1", "HA Switchable Resources"),
        ("5770SS1", "HA Journal Performance"),
        ("5770SS1", "Db2 Data Mirroring"),
        ("5770SS1", "More"),
    ];
    let rows = products
        .iter()
        .map(|(id, t)| vec![("LPID", *id), ("LPRLS", "V7R5M0"), ("AGRSTS", "*ACCEPTED"), ("LPTEXT", *t)])
        .collect();
    let mut d = MapDialog::new();
    d.set_list("AGRS", list(rows));
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&group("QSZLICPGM"), "WRKSFWAGR", &d, &opts).unwrap();
    assert_eq!(
        differing("displays", "wrksfwagr", &s, &[8, 9]),
        set(&["fields", "headings 8", "headings 9", "keys", "options"])
    );
}

/// `DSPSAVF` with pub400's first page. Ours shows the save file and the
/// object count above the list and the text column where pub400 shows
/// the owner, the size in kilobytes and the data saved, `5=Display`,
/// F11 and F16 (`COVERAGE.md`).
#[test]
fn dspsavf() {
    let objs = [
        ("USERNAMEB", "*LIB", "PROD", "256"),
        ("QSTJRN", "*JRN", "", "12"),
        ("DCL001", "*PGM", "RPGLE", "204"),
        ("DCL002", "*PGM", "RPGLE", "200"),
        ("DCL003", "*PGM", "RPGLE", "188"),
        ("DCL004", "*PGM", "RPGLE", "200"),
        ("DCL005", "*PGM", "RPGLE", "200"),
        ("DCL006", "*PGM", "RPGLE", "192"),
        ("DCL007", "*PGM", "RPGLE", "192"),
        ("DCL008", "*PGM", "RPGLE", "200"),
        ("DCL010", "*PGM", "RPGLE", "200"),
        ("DCL011", "*PGM", "RPGLE", "196"),
        ("DCL012", "*PGM", "RPGLE", "200"),
        ("DCL013", "*PGM", "RPGLE", "200"),
    ];
    let rows = objs
        .iter()
        .map(|(o, t, a, k)| vec![("OBJ", *o), ("OBJTYPE", *t), ("OBJATR", *a), ("OBJSIZE", *k), ("OBJTEXT", "")])
        .collect();
    let mut d = MapDialog::new();
    d.set("SAVF", "QSTSAVF").set("SAVFLIB", "USERNAMEB").set("SAVLIB", "USERNAMEB").set("OBJS", "14");
    d.set_list("OBJS", list(rows));
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&group("QUIDSPSAVF"), "DSPSAVF", &d, &opts).unwrap();
    assert_eq!(
        differing("savf", "dspsavf-1", &s, &[8]),
        set(&["fields", "headings 8", "keys", "more", "options"])
    );
}

/// `DSPSFWRSC`'s F11 view with pub400's first rows: the heading rows,
/// the columns, the keys and the paging indicator.
#[test]
fn dspsfwrsc_libraries() {
    let lic = ["AJSCB01", "AJSDH00", "AJSDJ01", "AJSGK01", "AJSGM01", "AJSLP01"];
    let rows = lic
        .iter()
        .map(|id| {
            vec![
                ("RSCID", *id),
                ("RSCOPT", "*BASE"),
                ("RSCFTR", "5050"),
                ("RSCDESC", ""),
                ("RSCTYPE", "*CODE"),
                ("RSCLIB", "##MACH#B"),
                ("RSCRLS", "V7R5M0 L00"),
            ]
        })
        .collect();
    let mut d = MapDialog::new();
    let mut l = list(rows);
    l.complete = true;
    d.set_list("RESOURCES", l);
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&group("QUISFWRSC"), "DSPSFWLIB", &d, &opts).unwrap();
    // The type and release columns sit one column off, the release
    // shows six characters, the keys a row lower (`COVERAGE.md`).
    assert_eq!(
        differing("licpgm", "dspsfwrsc-f11", &s, &[3, 4, 5]),
        set(&["headings 4", "headings 5", "keys", "more"])
    );
}

/// `DSPIPLA` with pub400's values: the first page row for row (the
/// labels, the section headings, the colon at column 52, `More...`,
/// the instruction and the keys).
#[test]
fn dspipla() {
    let values = [
        ("RESTART", "*SYS"),
        ("KEYLCKPOS", "*NORMAL"),
        ("HDWDIAG", "*MIN"),
        ("CPRJOBTBL", "*NONE"),
        ("CHKJOBTBL", "*ALL"),
        ("RBDPRDDIR", "*NORMAL"),
        ("MSFRCY", "*NONE"),
        ("DSPSTS", "*ALL"),
        ("STRTCP", "*YES"),
        ("SPLFRCY", "*REMOVE"),
        ("CLRJOBQ", "*NO"),
        ("CLROUTQ", "*NO"),
        ("CLRINCJOB", "*NO"),
        ("STRPRTWTR", "*YES"),
        ("STRRSTD", "*NO"),
    ];
    let mut d = MapDialog::new();
    for (k, v) in values {
        d.set(k, v);
    }
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&group("QUIIPLA"), "DSPIPLA", &d, &opts).unwrap();
    let want = read(&golden_dir().join("displays/dspipla.txt"));
    let ours = s.text_rows();
    for (i, (w, o)) in want.iter().zip(&ours).enumerate().take(23) {
        assert_eq!(o.trim_end(), w.trim_end(), "row {}", i + 1);
    }
    assert!(differing("displays", "dspipla", &s, &[]).is_empty());
}

/// `WRKRDBDIRE` with pub400's two entries
/// (`tools/conformance/golden/phase10/misc/wrkrdbdire`): the option
/// numbers, the headings and the keys. Ours has no blank entry to add
/// from at the top of the list, two-character option fields, heads the
/// location column on one row and ends the list a row higher.
#[test]
fn wrkrdbdire() {
    let rows = vec![
        vec![("RDB", "PUB400"), ("RMTLOC", "*LOCAL"), ("RDBTEXT", "please do not change|")],
        vec![("RDB", "RZKHCOCKPIT"), ("RMTLOC", "cockpit.rzkh.de"), ("RDBTEXT", "")],
    ];
    let mut d = MapDialog::new();
    let mut l = list(rows);
    l.complete = true;
    d.set_list("RDBS", l);
    let opts = Options {
        system: "SYSNAME".into(),
        ..Options::default()
    };
    let s = render(&group("QUIRDBDIR"), "WRKRDBDIRE", &d, &opts).unwrap();
    assert_eq!(differing("../phase10/misc", "wrkrdbdire", &s, &[9]), set(&["fields", "more"]));
}

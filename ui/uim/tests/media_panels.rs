// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media panels (`QSMEDIA`): `WRKIMGCLG` against pub400's capture
//! (`tools/conformance/golden/phase11/displays/wrkimgclg.txt`, row by
//! row), and every panel against goldens of our own rendering under
//! `tests/golden/media/` (`QS400_UPDATE_GOLDEN=1` rewrites them).

#[path = "wmcommon/mod.rs"]
mod wmcommon;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QSMEDIA").expect("QSMEDIA is shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QSMEDIA:\n{}", c.listing);
    c.group.expect("a group")
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/media")
}

fn list(rows: &[&[(&str, &str)]]) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect()
            })
            .collect(),
        top: 0,
        total: Some(rows.len()),
        complete: true,
    }
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

/// Render, check the frame, compare with (or write) our golden.
fn golden(panel: &str, d: &MapDialog) -> Vec<String> {
    let s = render(&group(), panel, d, &opts()).unwrap();
    let rows = s.text_rows();
    assert_eq!(rows.len(), 24);
    for r in &rows {
        assert!(r.chars().count() <= 80, "{r:?}");
    }
    let text = rows
        .iter()
        .map(|r| r.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let path = dir().join(format!("{panel}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(text, want, "{panel} differs from {}", path.display());
    rows
}

fn has(rows: &[String], text: &str) {
    assert!(
        rows.iter().any(|r| r.contains(text)),
        "no {text:?} in\n{}",
        rows.join("\n")
    );
}

#[test]
fn wrkimgclg_matches_the_capture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase11/displays/wrkimgclg.txt");
    let want: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let mut d = MapDialog::new();
    // Option 1 creates from the add entry on row 9 (R27).
    d.set_list("CLGS", list(&[]));
    let s = render(&group(), "WRKIMGCLG", &d, &opts()).unwrap();
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let g = got.get(i).map_or("", |r| r.trim_end());
        if g != w {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{g}|", i + 1);
        }
    }
    assert!(diff.is_empty(), "wrkimgclg differs:\n{diff}");
}

#[test]
fn catalogs_and_their_entries() {
    let mut d = MapDialog::new();
    d.set_list(
        "CLGS",
        list(&[
            &[
                ("CLG", "QS76TAP"),
                ("CLGSTS", "Ready"),
                ("CLGTYPE", "Tape"),
                ("CLGASP", "*NONE"),
                ("CLGDEV", "TAPMLB76"),
                ("DEVSTS", "VARIED ON"),
                ("CLGTEXT", "Cartridges of the test library"),
            ],
            &[
                ("CLG", "QS76OPT"),
                ("CLGSTS", "Not ready"),
                ("CLGTYPE", "Optical"),
                ("CLGASP", "*NONE"),
                ("CLGTEXT", "Optical saves"),
            ],
        ]),
    );
    let rows = golden("WRKIMGCLG", &d);
    has(&rows, "12=Work with entries");
    has(&rows, "QS76TAP");
    d.set("CLGVIEW", "TEXT");
    let rows = golden("WRKIMGCLG2", &d);
    has(&rows, "Cartridges of the test library");
    has(&rows, "F11=View status and devices");

    let mut d = MapDialog::new();
    d.set("CLG", "QS76OPT")
        .set("CLGSTS", "Ready")
        .set("CLGTYPE", "Optical")
        .set("CLGDEV", "OPTVRT76")
        .set("CLGDIR", "/qs76/opt");
    d.set_list(
        "ENTS",
        list(&[
            &[
                ("IDX", "1"),
                ("ENTSTS", "Mounted"),
                ("WRTPTC", "N"),
                ("VOLNAM", "QS76O1"),
                ("IMGFILE", "OPT1.ISO"),
            ],
            &[
                ("IDX", "2"),
                ("ENTSTS", "Loaded"),
                ("WRTPTC", "N"),
                ("VOLNAM", "QS76O2"),
                ("IMGFILE", "OPT2.ISO"),
            ],
        ]),
    );
    let rows = golden("WRKIMGCLGE", &d);
    has(&rows, "Work with Image Catalog Entries");
    has(&rows, "6=Mount");
    has(&rows, "F6=Load/Unload image catalog");
}

#[test]
fn cartridges_and_library_status() {
    let mut d = MapDialog::new();
    d.set("MLBDEV", "TAPMLB76");
    d.set_list(
        "CTGS",
        list(&[
            &[
                ("CTG", "QS76A"),
                ("CTGVOL", "QS76A"),
                ("CTGCGY", "*SHARE400"),
                ("CTGSTS", "Mounted"),
                ("CTGLOC", "TAPVRT03D1"),
            ],
            &[
                ("CTG", "QS76B"),
                ("CTGVOL", "QS76B"),
                ("CTGCGY", "*INSERT"),
                ("CTGSTS", "Inserted"),
                ("CTGLOC", "Slot 2"),
            ],
        ]),
    );
    let rows = golden("WRKTAPCTG", &d);
    has(&rows, "Work with Tape Cartridges");
    has(&rows, "8=Demount");
    d.set_list(
        "RSRCS",
        list(&[
            &[
                ("RSRC", "TAPMLB76"),
                ("RSRCSTS", "VARIED ON"),
                ("RSRCLIB", "TAPMLB76"),
            ],
            &[
                ("RSRC", "  TAPVRT03D1"),
                ("RSRCSTS", "OPERATIONAL"),
                ("ALLOC", "*ALLOCATED"),
                ("RSRCCTG", "QS76A"),
                ("RSRCLIB", "TAPMLB76"),
            ],
        ]),
    );
    let rows = golden("WRKMLBSTS", &d);
    has(&rows, "Work with Media Library Status");
    has(&rows, "6=Deallocate resource");
}

/// `WRKIMGCLG` with the 7.1 oracle's one catalog
/// (`audit/displays/wrkimgclg71`): the add entry on row 9, the
/// catalog under it. Row 10's text is left out: 7.1 shows the ASP
/// threshold (`*CALC`) two columns into its column, where every other
/// list puts a value at the column's start, and pub400 lists no
/// catalog to show what 7.5 does; its fields are compared.
#[test]
fn wrkimgclg_matches_the_71_capture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/audit/displays/wrkimgclg71.txt");
    let want: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let mut d = MapDialog::new();
    d.set_list(
        "CLGS",
        list(&[&[
            ("CLG", "JAVA8"),
            ("CLGSTS", "Not ready"),
            ("CLGTYPE", "Optical"),
            ("CLGASP", "*CALC"),
        ]]),
    );
    let s = render(&group(), "WRKIMGCLG", &d, &opts()).unwrap();
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate().take(23).filter(|(i, _)| *i != 9) {
        let g = got.get(i).map_or("", |r| r.trim_end());
        if g != w {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{g}|", i + 1);
        }
    }
    assert!(diff.is_empty(), "wrkimgclg71 differs:\n{diff}");
    wmcommon::fields_on_row("audit/displays/wrkimgclg71", &s, 9);
    wmcommon::fields_on_row("audit/displays/wrkimgclg71", &s, 10);
}

/// Rows 1 to 23 of a capture under `audit/displays/` against the
/// screen, `skip` rows aside, then every row's input fields.
/// An empty list's text, `(...)`, at the same column on both rows: the
/// words are IBM's in the capture and ours in the panel.
fn same_empty_text(golden: &str, ours: &str) -> bool {
    let paren = |r: &str| {
        let t = r.trim();
        (t.len() > 2 && t.starts_with('(') && t.ends_with(')'))
            .then(|| r.len() - r.trim_start().len())
    };
    paren(golden).is_some() && paren(golden) == paren(ours)
}

fn against71(name: &str, s: &uim::Screen, skip: &[usize]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../tools/conformance/golden/audit/displays/{name}.txt"
    ));
    let want: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let got = s.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate().take(23) {
        let g = got.get(i).map_or("", |r| r.trim_end());
        if !skip.contains(&(i + 1)) && g != w && !same_empty_text(w, g) {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{g}|", i + 1);
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
    for row in 1..=23 {
        wmcommon::fields_on_row(&format!("audit/displays/{name}"), s, row);
    }
}

/// `WRKMLBSTS` with no media library, as the 7.1 oracle shows it.
#[test]
fn wrkmlbsts_matches_the_71_capture() {
    let mut d = MapDialog::new();
    d.set_list("RSRCS", list(&[]));
    against71(
        "wrkmlbsts71",
        &render(&group(), "WRKMLBSTS", &d, &opts()).unwrap(),
        &[],
    );
}

/// `WRKIMGCLGE` of the 7.1 oracle's one catalog: two data columns, the
/// add entry with `*AVAIL` in its index field, one mounted image.
#[test]
fn wrkimgclge_matches_the_71_capture() {
    let mut d = MapDialog::new();
    d.set("CLG", "JAVA8")
        .set("CLGSTS", "Not ready")
        .set("CLGTYPE", "Optical")
        .set("CLGDEV", "")
        .set("CLGDIR", "/images/java8");
    d.set_list(
        "ENTS",
        list(&[&[
            ("IDX", "1"),
            ("ENTSTS", "Mounted"),
            ("IMGFILE", "5761-JV1_opt17.iso"),
        ]]),
    );
    d.set_ext("ENTS", &[("IDX", "*AVAIL")]);
    against71(
        "wrkimgclge71",
        &render(&group(), "WRKIMGCLGE", &d, &opts()).unwrap(),
        &[],
    );
}

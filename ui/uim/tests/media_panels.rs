// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media panels (`QSMEDIA`): `WRKIMGCLG` against pub400's capture
//! (`tools/conformance/golden/phase11/displays/wrkimgclg.txt`, row by
//! row), and every panel against goldens of our own rendering under
//! `tests/golden/media/` (`QS400_UPDATE_GOLDEN=1` rewrites them).

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
            .map(|r| r.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect())
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
    let text = rows.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n") + "\n";
    let path = dir().join(format!("{panel}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    assert_eq!(text, want, "{panel} differs from {}", path.display());
    rows
}

fn has(rows: &[String], text: &str) {
    assert!(rows.iter().any(|r| r.contains(text)), "no {text:?} in\n{}", rows.join("\n"));
}

#[test]
fn wrkimgclg_matches_the_capture() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/phase11/displays/wrkimgclg.txt");
    let want: Vec<String> = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let mut d = MapDialog::new();
    // The blank row option 1 creates from.
    d.set_list("CLGS", list(&[&[("OPT", ""), ("CLG", "")]]));
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
                ("CLGTYPE", "*TAP"),
                ("CLGASP", "*NONE"),
                ("CLGDEV", "TAPMLB76"),
                ("DEVSTS", "VARIED ON"),
                ("CLGTEXT", "Cartridges of the test library"),
            ],
            &[("CLG", "QS76OPT"), ("CLGSTS", "Not ready"), ("CLGTYPE", "*OPT"), ("CLGASP", "*NONE"), ("CLGTEXT", "Optical saves")],
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
        .set("CLGTYPE", "*OPT")
        .set("CLGDEV", "OPTVRT76")
        .set("CLGDIR", "/qs76/opt");
    d.set_list(
        "ENTS",
        list(&[
            &[("IDX", "1"), ("ENTSTS", "Mounted"), ("WRTPTC", "N"), ("VOLNAM", "QS76O1"), ("IMGFILE", "OPT1.ISO")],
            &[("IDX", "2"), ("ENTSTS", "Loaded"), ("WRTPTC", "N"), ("VOLNAM", "QS76O2"), ("IMGFILE", "OPT2.ISO")],
        ]),
    );
    let rows = golden("WRKIMGCLGE", &d);
    has(&rows, "Work with Image Catalog Entries");
    has(&rows, "8=Mount");
    has(&rows, "F6=Add entry");
}

#[test]
fn cartridges_and_library_status() {
    let mut d = MapDialog::new();
    d.set("MLBDEV", "TAPMLB76");
    d.set_list(
        "CTGS",
        list(&[
            &[("CTG", "QS76A"), ("CTGVOL", "QS76A"), ("CTGCGY", "*SHARE400"), ("CTGSTS", "Mounted"), ("CTGLOC", "TAPVRT03D1")],
            &[("CTG", "QS76B"), ("CTGVOL", "QS76B"), ("CTGCGY", "*INSERT"), ("CTGSTS", "Inserted"), ("CTGLOC", "Slot 2")],
        ]),
    );
    let rows = golden("WRKTAPCTG", &d);
    has(&rows, "Work with Tape Cartridges");
    has(&rows, "8=Demount");
    d.set_list(
        "RSRCS",
        list(&[
            &[("RSRC", "TAPMLB76"), ("RSRCSTS", "VARIED ON"), ("RSRCLIB", "TAPMLB76")],
            &[("RSRC", "  TAPVRT03D1"), ("RSRCSTS", "OPERATIONAL"), ("ALLOC", "*ALLOCATED"), ("RSRCCTG", "QS76A"), ("RSRCLIB", "TAPMLB76")],
        ]),
    );
    let rows = golden("WRKMLBSTS", &d);
    has(&rows, "Work with Media Library Status");
    has(&rows, "6=Deallocate resource");
}

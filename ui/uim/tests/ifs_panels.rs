// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS panels (`QUIWRKLNK`, `QUIWRKAUT`) against the pub400 goldens
//! of `tools/conformance/golden/phase10/ifs/`: each panel rendered with
//! the golden's visible values and compared row by row, the goldens'
//! masks (`USERNAME`, dates and times) copied over our side. One test
//! per golden, named after it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::{CompileOpts, ListState, MapDialog, NoImbeds, NoImports, Options, PanelGroup, Screen, compile, render};

/// The masks the captures put over values that differ by system.
const MASKS: &[&str] = &["SYSNAME", "USERNAME", "MM/DD/YY", "HH:MM:SS"];

fn golden_rows(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase10/ifs")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

fn shipped(name: &str) -> PanelGroup {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("pnlgrp")
        .join(format!("{name}.pnlgrp"));
    let text = std::fs::read_to_string(&path).unwrap();
    let c = compile(&text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{name} must compile:\n{}", c.listing);
    c.group.expect("group")
}

/// Our row with the golden's masks copied over the same columns; a
/// vendor copyright counts only where it starts.
fn masked(golden: &str, ours: &str) -> String {
    let mark = "(C) COPYRIGHT";
    if golden.find(mark).is_some_and(|i| ours.find(mark) == Some(i)) {
        return golden.to_string();
    }
    let mut out: Vec<char> = ours.chars().collect();
    let g: Vec<char> = golden.chars().collect();
    for m in MASKS {
        let mc: Vec<char> = m.chars().collect();
        let mut i = 0;
        while i + mc.len() <= g.len() {
            if g[i..i + mc.len()] == mc[..] {
                if out.len() < i + mc.len() {
                    out.resize(i + mc.len(), ' ');
                }
                out[i..i + mc.len()].copy_from_slice(&mc);
                i += mc.len();
            } else {
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

fn compare(golden: &str, screen: &Screen) {
    let want = golden_rows(golden);
    let got = screen.text_rows();
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let ours = masked(w, got.get(i).map_or("", String::as_str));
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(diff, "row {}:\n  want |{}|\n  got  |{}|", i + 1, w.trim_end(), ours.trim_end());
        }
    }
    assert!(diff.is_empty(), "{golden} differs:\n{diff}");
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

fn with_copyright(g: &PanelGroup) -> Options {
    Options {
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..opts()
    }
}

/// Link entries: name and type.
fn links(rows: &[(&str, &str)], complete: bool) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|(n, t)| {
                vec![
                    ("LNK".to_string(), (*n).to_string()),
                    ("LNKTYPE".to_string(), (*t).to_string()),
                ]
            })
            .collect(),
        top: 0,
        total: if complete { Some(rows.len()) } else { None },
        complete,
    }
}

fn wrklnk(dir: &str, rows: &[(&str, &str)], complete: bool) -> MapDialog {
    let mut d = MapDialog::new();
    d.set("DIR", dir);
    d.set_list("LNKS", links(rows, complete));
    d
}

const ROOT_1: &[(&str, &str)] = &[
    ("_welcome.txt", "STMF"),
    ("a.out", "STMF"),
    ("bin", "DIR"),
    ("core.20261001.1401 >", "STMF"),
    ("dev", "DIR"),
    ("etc", "DIR"),
    ("fb400temp", "DIR"),
    ("home", "DIR"),
    ("imgclg", "DIR"),
];

const ROOT_2: &[(&str, &str)] = &[
    ("instancecache", "DIR"),
    ("javacore.20261001. >", "STMF"),
    ("jitdump.20261001.1 >", "STMF"),
    ("lib", "DIR"),
    ("lib64", "DIR"),
    ("parts.csv", "STMF"),
    ("qmgtools", "DIR"),
    ("temp", "DIR"),
    ("tempdir", "DIR"),
];

#[test]
fn wrklnk_root() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("", &[("/", "DIR")], true);
    compare("wrklnk-root", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
}

#[test]
fn wrklnk_root_f23() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("", &[("/", "DIR")], true);
    compare("wrklnk-root-f23", &render(&g, "WRKLNK2", &d, &opts()).unwrap());
}

#[test]
fn wrklnk_root_contents() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("/", ROOT_1, false);
    compare("wrklnk-root-contents", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
    compare("wrklnk-5-root", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
}

#[test]
fn wrklnk_root_contents_2() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("/", ROOT_2, false);
    compare("wrklnk-root-contents-2", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
    compare("wrklnk-5-root-2", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
}

#[test]
fn wrklnk_5_dir() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk(
        "/etc",
        &[
            ("cit", "DIR"),
            ("ibm", "DIR"),
            ("magic", "STMF"),
            ("pip.conf", "STMF"),
            ("utmp", "STMF"),
            ("vfs", "STMF"),
        ],
        true,
    );
    compare("wrklnk-5-dir", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
}

#[test]
fn wrklnk_home() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("/home/USERNAME", &[("qsnet.txt", "STMF")], true);
    compare("wrklnk-home", &render(&g, "WRKLNK1", &d, &opts()).unwrap());
}

/// The option refused on an entry: the option stays typed, the message
/// on the message line.
fn refused(dir: &str, rows: &[(&str, &str)], complete: bool, at: usize) -> (MapDialog, Options) {
    let mut d = wrklnk(dir, rows, complete);
    let mut l = links(rows, complete);
    l.rows[at].push(("OPT".into(), "12".into()));
    d.set_list("LNKS", l);
    let o = Options {
        message: Some(("Specified option number is not allowed.".into(), true)),
        ..opts()
    };
    (d, o)
}

#[test]
fn wrklnk_12_root_refused() {
    let g = shipped("QUIWRKLNK");
    let (d, o) = refused("", &[("/", "DIR")], true, 0);
    compare("wrklnk-12-root-refused", &render(&g, "WRKLNK1", &d, &o).unwrap());
}

#[test]
fn wrklnk_12_dir_refused() {
    let g = shipped("QUIWRKLNK");
    let (d, o) = refused("/", ROOT_1, false, 5);
    compare("wrklnk-12-dir-refused", &render(&g, "WRKLNK1", &d, &o).unwrap());
}

#[test]
fn dsplnk_root() {
    let g = shipped("QUIWRKLNK");
    let d = wrklnk("", &[("/", "DIR")], true);
    compare("dsplnk-root", &render(&g, "DSPLNK", &d, &opts()).unwrap());
}

/// The attributes pub400 showed for its probe stream file.
fn stmf_attrs() -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("OBJ", "/home/USERNAME/qsnet.txt"),
        ("OBJTYPE", "STMF"),
        ("OWNER", "USERNAME"),
        ("SYSON", "Local"),
        ("ASP", "1"),
        ("OVERFLOW", "No"),
        ("CCSID", "819"),
        ("HIDDEN", "No"),
        ("PCSYSTEM", "No"),
        ("RDONLY", "No"),
        ("PCARC", "Yes"),
        ("SYSARC", "Yes"),
        ("CRTDT", "MM/DD/YY  HH:MM:SS"),
        ("ACCDT", "MM/DD/YY  HH:MM:SS"),
        ("CHGDT", "MM/DD/YY  HH:MM:SS"),
        ("ATRDT", "MM/DD/YY  HH:MM:SS"),
        ("SIZE", "53"),
        ("ALLOC", "8192"),
        ("FILEFMT", "*TYPE2"),
        ("EASIZE", "0"),
        ("STGFREE", "No"),
        ("TEMP", "No"),
        ("DISKOPT", "*NORMAL"),
        ("MAINOPT", "*NORMAL"),
        ("AUDIT", "*NOTAVL"),
        ("AUTCOL", "*NONE"),
        ("DOMAIN", "*SYSTEM"),
        ("NLINK", "1"),
        ("SUID", "No"),
        ("SGID", "No"),
        ("RSTDRNMUNL", "No"),
        ("LASTUSED", "MM/DD/YY"),
        ("DAYSUSED", "1"),
    ] {
        d.set(k, v);
    }
    d
}

/// The attributes of a directory: `/` or `/etc` on pub400.
fn dir_attrs(obj: &str, pcarc: &str) -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("OBJ", obj),
        ("OBJTYPE", "DIR"),
        ("OWNER", "QSYS"),
        ("SYSON", "Local"),
        ("ASP", "1"),
        ("OVERFLOW", "No"),
        ("CCSID", "37"),
        ("HIDDEN", "No"),
        ("PCSYSTEM", "No"),
        ("RDONLY", "No"),
        ("PCARC", pcarc),
        ("SYSARC", "Yes"),
        ("CRTDT", "MM/DD/YY  HH:MM:SS"),
        ("ACCDT", "MM/DD/YY  HH:MM:SS"),
        ("CHGDT", "MM/DD/YY  HH:MM:SS"),
        ("ATRDT", "MM/DD/YY  HH:MM:SS"),
        ("SIZE", "1392640"),
        ("ALLOC", "1392640"),
        ("DIRFMT", "*TYPE2"),
        ("EASIZE", "0"),
        ("STGFREE", "No"),
        ("TEMP", "No"),
        ("AUDIT", "*NOTAVL"),
        ("AUTCOL", "*NONE"),
        ("DOMAIN", "*SYSTEM"),
        ("NLINK", "25"),
        ("CRTAUD", "*NOTAVL"),
        ("CRTSCAN", "*YES"),
        ("CRTRUNEXIT", "No"),
        ("SUID", "No"),
        ("SGID", "Yes"),
        ("RSTDRNMUNL", "No"),
    ] {
        d.set(k, v);
    }
    d
}

fn page(n: usize) -> Options {
    Options { page: n, ..opts() }
}

#[test]
fn wrklnk_8_stmf() {
    let g = shipped("QUIWRKLNK");
    let d = stmf_attrs();
    compare("wrklnk-8-stmf", &render(&g, "DSPATRF", &d, &page(0)).unwrap());
    compare("wrklnk-8-stmf-2", &render(&g, "DSPATRF", &d, &page(1)).unwrap());
    compare("wrklnk-8-stmf-3", &render(&g, "DSPATRF", &d, &page(2)).unwrap());
}

#[test]
fn wrklnk_8_root() {
    let g = shipped("QUIWRKLNK");
    let d = dir_attrs("/", "Yes");
    compare("wrklnk-8-root", &render(&g, "DSPATRD", &d, &page(0)).unwrap());
    compare("wrklnk-8-root-2", &render(&g, "DSPATRD", &d, &page(1)).unwrap());
    compare("wrklnk-8-root-3", &render(&g, "DSPATRD", &d, &page(2)).unwrap());
}

#[test]
fn wrklnk_8_dir() {
    let g = shipped("QUIWRKLNK");
    let d = dir_attrs("/etc", "No");
    compare("wrklnk-8-dir", &render(&g, "DSPATRD", &d, &page(0)).unwrap());
}

/// Authority rows: user, data authority, the four object authorities.
fn auts(rows: &[(&str, &str, &str)]) -> ListState {
    let mut v = vec![Vec::new()];
    for (u, d, o) in rows {
        let mut r = vec![
            ("USER".to_string(), (*u).to_string()),
            ("DTAAUT".to_string(), (*d).to_string()),
        ];
        // The X sits in the middle of its column, as the driver puts it.
        for (i, (k, x)) in [("EXIST", "  X"), ("MGT", " X"), ("ALTER", "  X"), ("REF", " X")].iter().enumerate() {
            let x = if o.as_bytes().get(i) == Some(&b'X') { *x } else { "" };
            r.push(((*k).to_string(), x.to_string()));
        }
        v.push(r);
    }
    let n = v.len();
    ListState {
        rows: v,
        top: 0,
        total: Some(n),
        complete: true,
    }
}

fn wrkaut(obj: &str, ty: &str, owner: &str, rows: &[(&str, &str, &str)]) -> MapDialog {
    let mut d = MapDialog::new();
    d.set("OBJ", obj).set("OBJTYPE", ty).set("OWNER", owner);
    d.set_list("AUTS", auts(rows));
    d
}

#[test]
fn wrklnk_9_root() {
    let g = shipped("QUIWRKAUT");
    let d = wrkaut(
        "/",
        "DIR",
        "QSYS",
        &[("*PUBLIC", "*RWX", "XXXX"), ("QSYS", "*RWX", "XXXX"), ("QDIRSRV", "*X", "")],
    );
    compare("wrklnk-9-root", &render(&g, "WRKAUT", &d, &with_copyright(&g)).unwrap());
}

#[test]
fn wrklnk_9_stmf() {
    let g = shipped("QUIWRKAUT");
    let d = wrkaut(
        "/home/USERNAME/qsnet.txt",
        "STMF",
        "USERNAME",
        &[("*PUBLIC", "*EXCLUDE", ""), ("USERNAME", "*RWX", "XXXX")],
    );
    compare("wrklnk-9-stmf", &render(&g, "WRKAUT", &d, &with_copyright(&g)).unwrap());
}

/// A panel without a golden: its title centred on row 1, its keys on
/// the key rows.
fn title_and_keys(group: &str, panel: &str, d: &MapDialog, title: &str, keys: &[&str]) {
    let g = shipped(group);
    let s = render(&g, panel, d, &opts()).unwrap();
    let rows = s.text_rows();
    let col = (80 - title.len()) / 2;
    assert_eq!(rows[0].get(col..col + title.len()), Some(title), "{panel} title:\n{}", rows.join("\n"));
    let key_rows = format!("{}\n{}", rows[21], rows[22]);
    for k in keys {
        assert!(key_rows.contains(k), "{panel}: {k} not on the key rows:\n{}", rows.join("\n"));
    }
}

#[test]
fn dspaut_title_and_keys() {
    let d = wrkaut("/home/USERNAME/qsnet.txt", "STMF", "USERNAME", &[("*PUBLIC", "*EXCLUDE", "")]);
    title_and_keys("QUIWRKAUT", "DSPAUT", &d, "Display Authority", &["F3=Exit", "F11=Display detail data authorities", "F12=Cancel"]);
}

#[test]
fn wrklnk_other_option_sets() {
    let d = wrklnk("/", ROOT_1, false);
    title_and_keys("QUIWRKLNK", "WRKLNK3", &d, "Work with Object Links", &["F23=More options"]);
    let g = shipped("QUIWRKLNK");
    let s = render(&g, "WRKLNK3", &d, &opts()).unwrap();
    assert!(s.text_rows()[5].contains("12=Work with links"));
}

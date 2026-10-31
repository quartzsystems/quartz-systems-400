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
use std::fmt::Write as _;
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

/// An empty list's text, `(...)`, at the same column on both rows: the
/// words are IBM's in the capture and ours in the panel.
fn same_empty_text(golden: &str, ours: &str) -> bool {
    let paren = |r: &str| {
        let t = r.trim();
        (t.len() > 2 && t.starts_with('(') && t.ends_with(')')).then(|| r.len() - r.trim_start().len())
    };
    paren(golden).is_some() && paren(golden) == paren(ours)
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
        if o.trim_end() != w.trim_end() && !same_empty_text(w, o) {
            let _ = write!(diff, "row {:>2}\n  want |{}|\n  got  |{}|\n", i + 1, w.trim_end(), o.trim_end());
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

// ------------------------------------------------------------ DSPBCKSTS

/// `DSPBCKSTS` with no backup run (pub400 has none either): both pages
/// row for row, the system name pub400's.
#[test]
fn dspbcksts_matches_the_captures() {
    let g = group("QEZBCKUP");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS");
    let o = |page| Options {
        system: "PUB400".into(),
        page,
        ..Options::default()
    };
    against("audit/displays/dspbcksts", &render(&g, "DSPBCKSTS", &d, &o(0)).unwrap(), &[]);
    against("audit/displays/dspbcksts-2", &render(&g, "DSPBCKSTS", &d, &o(1)).unwrap(), &[]);
    // F22 with no history; ours says so in its own words and keeps
    // `Bottom` under the empty list, where pub400 shows none.
    d.set_list("HIST", list(Vec::new()));
    let s = render(&g, "DSPBCKHST", &d, &o(0)).unwrap();
    against("audit/displays/dspbckhst", &s, &[9, 22]);
    assert_eq!(s.text_rows()[8].trim(), "(No backup has run.)");
}

/// `DSPBCKSTS` after two backups, and F22's history.
#[test]
fn dspbcksts_with_backups() {
    let g = group("QEZBCKUP");
    let mut d = MapDialog::new();
    d.set("*DATETIME", "12/31/26  23:59:59");
    for (k, v) in [
        ("ULLISTD", "12/30/26"),
        ("ULLISTT", "DAYA"),
        ("SECDTAD", "12/30/26"),
        ("SECDTAT", "DAYA"),
        ("UDALLD", "12/24/26"),
        ("UDALLT", "WEEK1"),
    ] {
        d.set(k, v);
    }
    ours("dspbcksts", &render(&g, "DSPBCKSTS", &d, &opts(0)).unwrap());
    d.set_list(
        "HIST",
        list(vec![
            vec![("HDATE", "12/30/26"), ("HOPT", "*DAILY"), ("HSET", "DAYA"), ("HCHG", "Yes")],
            vec![("HDATE", "12/24/26"), ("HOPT", "*WEEKLY"), ("HSET", "WEEK1"), ("HCHG", "No")],
        ]),
    );
    ours("dspbckhst", &render(&g, "DSPBCKHST", &d, &opts(0)).unwrap());
    for (k, v) in [
        ("HDATE", "12/24/26"),
        ("HTIME", "23:00:00"),
        ("HOPT", "*WEEKLY"),
        ("HDEV", "TAP01"),
        ("HSET", "WEEK1"),
        ("HSTS", "Ended with errors"),
    ] {
        d.set(k, v);
    }
    d.set_list(
        "STEPS",
        list(vec![vec![("HSTEP", "SAVLIB completed")], vec![("HSTEP", "SAVCFG failed")], vec![("HSTEP", "SAV completed")]]),
    );
    ours("dspbckdtl", &render(&g, "DSPBCKDTL", &d, &opts(0)).unwrap());
}

// ------------------------------------------------- tape and optical

/// `WRKOPTVOL` with no volumes against pub400's capture (which has no
/// optical device either) and the 7.1 oracle's (two empty devices),
/// every input field included: row 11 is the extended action entry, an
/// option and a volume field over the list.
#[test]
fn wrkoptvol_matches_the_captures() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    d.set("WDEV", "*ALL").set("CSI", "*ALL");
    d.set_list("VOLS", list(Vec::new()));
    let s = render(&g, "WRKOPTVOL", &d, &opts(0)).unwrap();
    against("audit/displays/wrkoptvol", &s, &[]);
    against("verified/oracle71/audit/wrkoptvol71", &s, &[]);
    // The cursor starts on the add entry's option.
    assert_eq!(s.cursor, Some((11, 2)));
}

/// `WRKOPTVOL`'s add entry with option 1 and a volume typed (7.1), and
/// as the oracle shows it after F12 on `ADDOPTCTG`'s prompt: the
/// option blank, the volume kept.
#[test]
fn wrkoptvol_add_entry_typed() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    d.set("WDEV", "*ALL").set("CSI", "*ALL");
    d.set_list("VOLS", list(Vec::new()));
    d.set_ext("VOLS", &[("OPT", "1"), ("VOL", "QSTESTVOL")]);
    against("audit/displays/wrkoptvol-ext71", &render(&g, "WRKOPTVOL", &d, &opts(0)).unwrap(), &[]);
    d.set_ext("VOLS", &[("VOL", "QSTESTVOL")]);
    against("audit/displays/wrkoptvol-add-back71", &render(&g, "WRKOPTVOL", &d, &opts(0)).unwrap(), &[]);
}

/// `WRKOPTVOL` with two volumes: IBM's columns, ours filled.
#[test]
fn wrkoptvol_with_volumes() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    d.set("WDEV", "*ALL").set("CSI", "*ALL");
    d.set_list(
        "VOLS",
        list(vec![
            vec![("VOL", "QS400V1R0M0"), ("DEV", "OPTVRT01"), ("VOLTYPE", "*PRIMARY"), ("MEDTYPE", "*CD-ROM"), ("AUTL", "*NONE")],
            vec![("VOL", "BACKUP1"), ("DEV", "OPTVRT01"), ("VOLTYPE", "*PRIMARY"), ("MEDTYPE", "*DVD-RAM"), ("AUTL", "*NONE")],
        ]),
    );
    ours("wrkoptvol", &render(&g, "WRKOPTVOL", &d, &opts(0)).unwrap());
}

/// The 7.1 oracle's `DSPTAP` displays, on the SAVSYS volume in its
/// TAP01 (`audit/displays/dsptap71-*`, 2026-10-31).
const TAP71: &str = "audit/displays/dsptap71";

/// Capture `name`'s text at `row` from `col` (both from 1), `len`
/// wide; `trim` drops the blanks around it. The tests fill the panels
/// with the capture's own values and hold our geometry to its rows.
fn cap(name: &str, row: usize, col: usize, len: usize, trim: bool) -> String {
    let rows = read(&goldens().join(format!("{TAP71}-{name}.txt")));
    let s: String = rows[row - 1].chars().skip(col - 1).take(len).collect();
    if trim { s.trim().to_string() } else { s }
}

/// The volume rows every labels display shows.
fn tape_volume(d: &mut MapDialog) {
    for (k, row, col) in [("DEV", 3, 25), ("OWNER", 4, 25), ("VOLTYPE", 5, 25), ("VOLID", 3, 66), ("DENS", 4, 66), ("CODE", 5, 66)] {
        d.set(k, &cap("lbl-1", row, col, 14, true));
    }
}

/// One file's labels: the first page's rows from `first`, the second
/// page's from `lbl-2`.
fn tape_file(d: &mut MapDialog, first: &str) {
    for (k, row) in [("FLABEL", 8), ("FSEQ", 9), ("RECFMT", 11), ("BLKATR", 12), ("RECLEN", 13), ("BLKLEN", 14), ("FLEN", 15), ("BUFOFF", 16), ("CTLCHR", 17)] {
        d.set(k, &cap(first, row, 41, 20, true));
    }
    for (k, row) in [("MVIND", 8), ("FVSEQ", 9), ("FCRT", 11), ("FEXP", 12), ("FILTYP", 14), ("CMPACT", 15), ("SYSCOD", 16)] {
        d.set(k, &cap("lbl-2", row, 41, 20, true));
    }
}

/// `DSPTAP DATA(*LABELS)`: one file to a display, two pages; F14 shows
/// from the second file on. Row 23 is the oracle's message line.
#[test]
fn dsptap_labels_match_the_71_captures() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    tape_volume(&mut d);
    tape_file(&mut d, "lbl-1");
    d.set("PRVENT", "N");
    against(&format!("{TAP71}-lbl-1"), &render(&g, "DSPTAP", &d, &opts(0)).unwrap(), &[23]);
    against(&format!("{TAP71}-lbl-2"), &render(&g, "DSPTAP", &d, &opts(1)).unwrap(), &[23]);
    tape_file(&mut d, "lbl-next1");
    d.set("PRVENT", "Y");
    against(&format!("{TAP71}-lbl-next1"), &render(&g, "DSPTAP", &d, &opts(0)).unwrap(), &[23]);
    // `DATA(*VOLLBL)`, ours: the volume rows alone.
    for (k, v) in [("DEV", "TAPVRT01"), ("VOLID", "VOL001"), ("OWNER", "QS400"), ("DENS", "*VRT256K")] {
        d.set(k, v);
    }
    ours("dsptapvol", &render(&g, "DSPTAPVOL", &d, &opts(0)).unwrap());
}

/// `DATA(*SAVRST)` on a file that holds no save: Display File
/// Information. Row 11 is IBM's description of its own Licensed
/// Internal Code file, which no QS/400 file has.
#[test]
fn dsptap_file_information_matches_the_71_capture() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    for (k, row) in [("DEV", 4), ("VOLID", 5), ("SSEQ", 6), ("FLABEL", 7), ("FEXP", 8)] {
        d.set(k, &cap("sav-1", row, 44, 20, true));
    }
    against(&format!("{TAP71}-sav-1"), &render(&g, "DSPTAPFIL", &d, &opts(0)).unwrap(), &[11, 23]);
}

/// The objects of capture `name`'s rows 9 to 21.
fn saved_objects(name: &str) -> ListState {
    let rows: Vec<Vec<(String, String)>> = (9..=21)
        .map(|r| {
            vec![
                ("OBJ".to_string(), cap(name, r, 7, 10, true)),
                ("OTYPE".to_string(), cap(name, r, 19, 10, true)),
                ("OATTR".to_string(), cap(name, r, 29, 10, true)),
                ("OOWN".to_string(), cap(name, r, 41, 10, true)),
                ("OSIZK".to_string(), cap(name, r, 56, 8, false)),
                ("ODATA".to_string(), cap(name, r, 66, 4, true)),
            ]
        })
        .collect();
    ListState { total: None, rows, top: 0, complete: false }
}

/// `DATA(*SAVRST)` on a save: Display Saved Objects, its text view (F11)
/// and the save's header (F16, two pages).
#[test]
fn dsptap_saves_match_the_71_captures() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    d.set("SLIB", &cap("obj-1", 3, 34, 10, true));
    d.set_list("OBJS", saved_objects("obj-1"));
    against(&format!("{TAP71}-obj-1"), &render(&g, "DSPTAPSAV", &d, &opts(0)).unwrap(), &[]);
    d.set_list("OBJS", saved_objects("obj-3"));
    against(&format!("{TAP71}-obj-f11"), &render(&g, "DSPTAPSAV2", &d, &opts(0)).unwrap(), &[]);
    for (k, row) in [("DEV", 4), ("VOLID", 5), ("SSEQ", 6), ("FLABEL", 7), ("FEXP", 8), ("SAVCMD", 10), ("SAVDTS", 11), ("SAVCMP", 12), ("SAVCPT", 13), ("SAVRLS", 14), ("SAVFMT", 15)] {
        d.set(k, &cap("obj-f16", row, 44, 20, true));
    }
    for (k, row) in [("SLIB", 4), ("SASP", 5), ("SSYS", 6), ("SSER", 7), ("SPVT", 8)] {
        d.set(k, &cap("obj-f16-2", row, 44, 20, true));
    }
    against(&format!("{TAP71}-obj-f16"), &render(&g, "DSPTAPHDR", &d, &opts(0)).unwrap(), &[]);
    against(&format!("{TAP71}-obj-f16-2"), &render(&g, "DSPTAPHDR", &d, &opts(1)).unwrap(), &[]);
}

/// `DSPTAPCTG`: one cartridge (no oracle has a library: `CPF9814`).
#[test]
fn dsptapctg_one_cartridge() {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("MLBDEV", "TAPMLB01"),
        ("CTG", "VOL001"),
        ("CTGVOL", "VOL001"),
        ("CTGCGY", "*SHARE400"),
        ("CTGSYS", "SYSNAME"),
        ("CTGSTS", "Available"),
        ("CTGLOC", "*SLOT"),
        ("CTGHOME", "3"),
    ] {
        d.set(k, v);
    }
    ours("dsptapctg", &render(&group("QSMEDIA"), "DSPTAPCTG", &d, &opts(0)).unwrap());
}

/// `DSPOPT`'s four views (no oracle has a volume: `OPT1860`).
#[test]
fn dspopt_views() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    for (k, v) in [
        ("VOLID", "QS400V1R0M0"),
        ("DEV", "OPTVRT01"),
        ("VOLTYPE", "*PRIMARY"),
        ("MEDTYPE", "*CD-ROM"),
        ("MEDFMT", "*ISO9660"),
        ("WRTPTC", "Yes"),
        ("CRTDTS", "10/31/26  12:00:00"),
        ("VOLCAP", "734003200"),
        ("VOLUSED", "512000000"),
        ("VOLFREE", "222003200"),
        ("BLKSIZ", "2048"),
        ("SETSIZE", "1"),
        ("SETSEQ", "1"),
    ] {
        d.set(k, v);
    }
    ours("dspoptvol", &render(&g, "DSPOPTVOL", &d, &opts(0)).unwrap());
    d.set_list(
        "DIRS",
        list(vec![
            vec![("DPATH", "/"), ("DFILES", "3"), ("DDIRS", "2"), ("DBYTES", "4096")],
            vec![("DPATH", "/QSAVSYS"), ("DFILES", "1"), ("DDIRS", "0"), ("DBYTES", "512000000")],
        ]),
    );
    ours("dspoptdir", &render(&g, "DSPOPTDIR", &d, &opts(0)).unwrap());
    d.set_list("OFILES", list(vec![vec![("FPATH", "/QSAVSYS/SAVSYS.SAV"), ("FSIZE", "512000000")]]));
    ours("dspoptfil", &render(&g, "DSPOPTFIL", &d, &opts(0)).unwrap());
    d.set_list(
        "OSAVS",
        list(vec![vec![("OFILE", "/QSAVSYS/SAVSYS.SAV"), ("OBJ", "QSYS"), ("OTYPE", "*LIB"), ("OLIB", "QSYS"), ("OSIZE", "512000000"), ("OTEXT", "System library")]]),
    );
    ours("dspoptsav", &render(&g, "DSPOPTSAV", &d, &opts(0)).unwrap());
}

// -------------------------------------------------------------- DSPUDFS

/// `DSPUDFS`: no oracle has a UDFS (pub400 answers `CPFA0A9`,
/// `audit/displays/dspudfs-none`); mounted once, and not mounted.
#[test]
fn dspudfs_mounted_and_not() {
    let g = group("QUIDSPUDFS");
    let mut d = MapDialog::new();
    for (k, v) in [
        ("UDFS", "/dev/QASP01/mine.udfs"),
        ("OWNER", "QSECOFR"),
        ("CASE", "*MONO"),
        ("DFTFMT", "*TYPE2"),
        ("CRTAUD", "*SYSVAL"),
        ("CRTDTS", "10/31/26  12:00:00"),
        ("TEXT", "probe"),
        ("MOUNTED", "Yes"),
    ] {
        d.set(k, v);
    }
    d.set_list("MOUNTS", list(vec![vec![("MNTDIR", "/mnt")]]));
    ours("dspudfs", &render(&g, "DSPUDFS", &d, &opts(0)).unwrap());
    d.set("MOUNTED", "No");
    d.set_list("MOUNTS", list(Vec::new()));
    ours("dspudfs-unmounted", &render(&g, "DSPUDFS", &d, &opts(0)).unwrap());
}

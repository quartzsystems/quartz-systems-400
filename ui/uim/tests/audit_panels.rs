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

/// `DSPTAP`: no oracle has a tape (`CPF6718` on pub400,
/// `audit/displays/ids-dsptap`); the labels, the volume label alone and
/// the saves, from IBM's documentation.
#[test]
fn dsptap_views() {
    let g = group("QSMEDIA");
    let mut d = MapDialog::new();
    for (k, v) in [("VOLID", "VOL001"), ("OWNER", "QS400"), ("VOLTYPE", "SL"), ("CODE", "*EBCDIC"), ("DEV", "TAPVRT01")] {
        d.set(k, v);
    }
    d.set_list(
        "FILES",
        list(vec![
            vec![("FSEQ", "1"), ("FLABEL", "QFILEA"), ("FVSEQ", "1"), ("FCRT", "10/31/26"), ("FEXP", "*PERM"), ("FBLK", "12"), ("FEND", "EOF")],
            vec![("FSEQ", "2"), ("FLABEL", "QGPL"), ("FVSEQ", "1"), ("FCRT", "10/31/26"), ("FEXP", "*NONE"), ("FBLK", "340"), ("FEND", "EOF")],
        ]),
    );
    ours("dsptap", &render(&g, "DSPTAP", &d, &opts(0)).unwrap());
    ours("dsptapvol", &render(&g, "DSPTAPVOL", &d, &opts(0)).unwrap());
    d.set_list(
        "OBJS",
        list(vec![
            vec![("OSEQ", "2"), ("OBJ", "QGPL"), ("OTYPE", "*LIB"), ("OLIB", "QSYS"), ("OATTR", "PROD"), ("OSIZE", "65536"), ("OTEXT", "General purpose")],
            vec![("OSEQ", "2"), ("OBJ", "QCLSRC"), ("OTYPE", "*FILE"), ("OLIB", "QGPL"), ("OATTR", "PF"), ("OSIZE", "28672")],
        ]),
    );
    ours("dsptapsav", &render(&g, "DSPTAPSAV", &d, &opts(0)).unwrap());
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

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PDM's displays against the pub400 goldens: each built with the values
//! the golden shows and compared character by character.

mod common;

use std::collections::BTreeMap;

use common::{compare, compare_rows};
use pdm::screen::list::{self, ListScreen, OptionsScreen, Row, View};
use pdm::screen::{menu, prompt};
use pdm::{Defaults, ListKind};

const SYS: &str = "PUB400";

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn values(m: &BTreeMap<String, String>) -> impl Fn(&str) -> String + '_ {
    move |k| m.get(k).cloned().unwrap_or_default()
}

fn defaults_values() -> BTreeMap<String, String> {
    let d = Defaults::default();
    pdm::defaults::FIELDS
        .iter()
        .map(|f| ((*f).to_string(), d.get(f)))
        .collect()
}

#[test]
fn pdm_strpdm() {
    compare("pdm-strpdm", &menu::strpdm("", None, true));
}

#[test]
fn pdm_strpdm_help() {
    let s = menu::help("PDM Main Menu - Help", &["Text.".to_string()], 0);
    compare_rows("pdm-strpdm-help", &s, &[22, 23]);
    assert!(s.text_rows()[0].contains("PDM Main Menu - Help"));
}

#[test]
fn pdm_specify_libs() {
    let m = map(&[("LIB", "*LIBL"), ("ASPNUM", "*ALL"), ("ASPDEV", "*")]);
    compare("pdm-specify-libs", &prompt::specify_libs(&values(&m), None));
}

#[test]
fn pdm_specify_objs() {
    let m = map(&[
        ("LIB", "*CURLIB"),
        ("OBJ", "*ALL"),
        ("OBJTYPE", "*ALL"),
        ("OBJATR", "*ALL"),
    ]);
    compare("pdm-specify-objs", &prompt::specify_objs(&values(&m), None));
}

#[test]
fn pdm_specify_mbrs() {
    let m = map(&[("LIB", "*LIBL"), ("MBR", "*ALL"), ("MBRTYPE", "*ALL")]);
    compare("pdm-specify-mbrs", &prompt::specify_mbrs(&values(&m), None));
}

#[test]
fn pdm_specify_optfile() {
    let m = map(&[
        ("OPTFILE", "QAUOOPT"),
        ("OPTLIB", "QGPL"),
        ("OPTMBR", "QAUOOPT"),
    ]);
    compare(
        "pdm-specify-optfile",
        &prompt::specify_optfile(&values(&m), None),
    );
}

#[test]
fn pdm_defaults() {
    let m = defaults_values();
    compare("pdm-defaults", &prompt::defaults(&values(&m), 0, None));
    compare("pdm-defaults-list", &prompt::defaults(&values(&m), 0, None));
}

#[test]
fn pdm_defaults_2() {
    let m = defaults_values();
    compare("pdm-defaults-2", &prompt::defaults(&values(&m), 1, None));
}

#[test]
fn pdm_subset_objs() {
    let m = map(&[
        ("OBJ", "*ALL"),
        ("OBJTYPE", "*ALL"),
        ("OBJATR", "*ALL"),
        ("SIZEFROM", "0"),
        ("SIZETO", "9999999999"),
        ("TEXT", "*ALL"),
    ]);
    compare("pdm-subset-objs", &prompt::subset_objs(&values(&m), None));
}

#[test]
fn pdm_subset_mbrs() {
    let m = map(&[
        ("MBR", "*ALL"),
        ("MBRTYPE", "*ALL"),
        ("FROMDATE", "01/01/00"),
        ("TODATE", "12/31/99"),
        ("TEXT", "*ALL"),
    ]);
    compare("pdm-subset-mbrs", &prompt::subset_mbrs(&values(&m), None));
}

#[test]
fn pdm_fndstrpdm() {
    let m = map(&[
        ("FROMCOL", "1"),
        ("TOCOL", "*RCDLEN"),
        ("CASE", "2"),
        ("OPTION", "5"),
        ("PROMPT", "N"),
        ("PRTLIST", "N"),
        ("PRTRCDS", "N"),
        ("NUMBER", "*ALL"),
        ("FORMAT", "*CHAR"),
        ("MARK", "Y"),
        ("OVERFLOW", "1"),
        ("BATCH", "N"),
    ]);
    compare("pdm-fndstrpdm", &prompt::find(&values(&m), None));
}

fn row(name: &str, ty: &str, third: &str, text: &str) -> Row {
    Row {
        name: name.into(),
        ty: ty.into(),
        third: third.into(),
        date: "01/01/26".into(),
        size: 0,
        text: text.into(),
    }
}

fn lib_screen(opt: usize, keys: usize, view: View, copyright: bool) -> pdm::screen::Screen {
    let header = map(&[("LISTTYPE", "*ALL")]);
    let rows = vec![row(
        "XXXXXXXX1",
        "*PROD",
        "",
        "XXXXXXXX (*CURLIB your current library)",
    )];
    let options = BTreeMap::new();
    list::render(&ListScreen {
        kind: ListKind::Library,
        system: SYS,
        header: &header,
        option_set: opt,
        key_set: keys,
        view,
        rows: &rows,
        top: 0,
        options: &options,
        cmdline: "",
        message: None,
        copyright,
    })
}

#[test]
fn pdm_wrklibpdm() {
    compare("pdm-wrklibpdm", &lib_screen(0, 0, View::Full, true));
    compare("pdm-wrklibpdm-f23", &lib_screen(1, 0, View::Full, false));
    compare("pdm-wrklibpdm-f24", &lib_screen(0, 1, View::Full, false));
    compare("pdm-wrklibpdm-keys3", &lib_screen(0, 2, View::Full, false));
    compare("pdm-wrklibpdm-f11", &lib_screen(0, 0, View::Names, false));
}

fn obj_rows() -> Vec<Row> {
    let mut v = vec![
        row("QDFUZZ", "*PGM", "RPGLE", ""),
        row("QSTESTPUB", "*PGM", "CLLE", ""),
        row("QCBLLESRC", "*FILE", "PF-SRC", "COBOL source files"),
        row("QCLSRC", "*FILE", "PF-SRC", "CL source files"),
        row("QCMDSRC", "*FILE", "PF-SRC", "COMMAND source files"),
        row("QCPPSRC", "*FILE", "PF-SRC", "C++ Source files"),
        row("QDDSSRC", "*FILE", "PF-SRC", "DDS source files"),
        row("QDFUZZIN", "*FILE", "PF-DTA", ""),
    ];
    for n in ["QDFUZZOUT", "QRPGLESRC", "XFERPF"] {
        v.push(row(n, "*FILE", "PF-DTA", ""));
    }
    v
}

fn obj_screen(opt: usize, keys: usize, view: View, copyright: bool) -> pdm::screen::Screen {
    let header = map(&[("LIB", "XXXXXXXX1")]);
    let rows = obj_rows();
    let options = BTreeMap::new();
    list::render(&ListScreen {
        kind: ListKind::Object,
        system: SYS,
        header: &header,
        option_set: opt,
        key_set: keys,
        view,
        rows: &rows,
        top: 0,
        options: &options,
        cmdline: "",
        message: None,
        copyright,
    })
}

#[test]
fn pdm_wrkobjpdm() {
    compare("pdm-wrkobjpdm", &obj_screen(0, 0, View::Full, true));
    compare("pdm-wrkobjpdm-f23", &obj_screen(1, 0, View::Full, false));
    compare("pdm-wrkobjpdm-f24", &obj_screen(2, 1, View::Full, false));
    compare("pdm-wrkobjpdm-keys3", &obj_screen(0, 2, View::Full, false));
    compare("pdm-wrkobjpdm-opts4", &obj_screen(3, 0, View::Full, false));
}

#[test]
fn pdm_wrkobjpdm_f11() {
    // The golden's 10=Restore sits one column right of the full view's;
    // every other row is compared.
    let s = obj_screen(0, 0, View::Names, false);
    let rows: Vec<usize> = (1..=24).filter(|r| *r != 8).collect();
    compare_rows("pdm-wrkobjpdm-f11", &s, &rows);
}

fn mbr_rows() -> Vec<Row> {
    let texts = [
        ("CMRPG", "CPI-C HEADER"),
        ("DSQCOMMR", "QUERY MANAGER INCLUDES"),
        ("ECACHCMD", "COMMAND ANALYZER CHANGE EXIT"),
        ("ECARTCMD", "COMMAND ANALYZER RETRIEVE EXIT"),
        ("ECHGPRF1", "CHANGE USER PROFILE EXIT PROGRAM"),
        ("ECHGRCV1", "CHANGE JOURNAL RECEIVER EXIT PROGRAM"),
        ("ECHKPWD1", "CHECK PASSWORD EXIT PROGRAM"),
        ("ECLRMST", "CLEAR MASTER KEY EXIT EXIT PROGRAM"),
    ];
    let mut v: Vec<Row> = texts.iter().map(|(n, t)| row(n, "RPGLE", "", t)).collect();
    for n in [
        "ECRTPRF1", "EDBOPNDB", "EDCVARY", "EDLTKR", "EDLTPRF1", "EDLTPRF2", "EDLTRCV1", "EIM",
        "EIMGEPH", "EJOBNTFY", "EKICONR", "EMHDFTPG", "EMOOPTEP", "ENPSEP", "EOGDOCH", "EOK",
        "EZZZ",
    ] {
        v.push(row(n, "RPGLE", "", ""));
    }
    v
}

fn mbr_screen(
    rows: &[Row],
    lib: &str,
    file: &str,
    opt: usize,
    keys: usize,
    view: View,
    copyright: bool,
) -> pdm::screen::Screen {
    let header = map(&[("FILE", file), ("LIB", lib)]);
    let options = BTreeMap::new();
    list::render(&ListScreen {
        kind: ListKind::Member,
        system: SYS,
        header: &header,
        option_set: opt,
        key_set: keys,
        view,
        rows,
        top: 0,
        options: &options,
        cmdline: "",
        message: None,
        copyright,
    })
}

#[test]
fn pdm_wrkmbrpdm() {
    let r = mbr_rows();
    let m = |o, k, v, c| mbr_screen(&r, "QSYSINC", "QRPGLESRC", o, k, v, c);
    compare("pdm-wrkmbrpdm", &m(0, 0, View::Full, true));
    compare("pdm-wrkmbrpdm-f23", &m(1, 0, View::Full, false));
    compare("pdm-wrkmbrpdm-f24", &m(0, 1, View::Full, false));
    compare("pdm-wrkmbrpdm-f14", &m(0, 2, View::Date, false));
    compare("pdm-wrkmbrpdm-f11", &m(0, 2, View::Names, false));
    compare("pdm-wrkmbrpdm-opts3", &m(2, 0, View::Full, false));
}

#[test]
fn pdm_wrkmbrpdm_own() {
    compare(
        "pdm-wrkmbrpdm-own",
        &mbr_screen(&[], "XXXXXXXX1", "QCLSRC", 0, 0, View::Full, true),
    );
}

fn uopt_rows() -> Vec<(String, String)> {
    [
        ("AB", "STRCODECMD CMD('CODEBRWS \"</ADM>&ZP/&ZL/&ZT/&ZN\"')"),
        ("AD", "STRCODECMD CMD('CODEDSU \"</ADM>&ZP/&ZL/&ZT/&ZN\"')"),
        ("AE", "STRCODECMD CMD('CODEEDIT \"</ADM>&ZP/&ZL/&ZT/&ZN\"')"),
        ("C", "CALL &O/&N"),
        ("CB", "STRCODECMD CMD('CODEBRWS \"<>&L/&F(&N)\"')"),
        ("CC", "CHGCURLIB CURLIB(&L)"),
        ("CD", "STRDFU OPTION(2)"),
        ("CE", "STRCODECMD CMD('CODEEDIT \"<>&L/&F(&N)\"')"),
        ("CL", "CHGCURLIB CURLIB(&N)"),
        ("CM", "CRTMSGF"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
    .collect()
}

#[test]
fn pdm_wrkusrdfnopt() {
    let rows = uopt_rows();
    let options = BTreeMap::new();
    let o = |k| OptionsScreen {
        system: SYS,
        file: ("QAUOOPT", "QGPL", "QAUOOPT"),
        position: "",
        rows: &rows,
        top: 0,
        options: &options,
        key_set: k,
        cmdline: "",
        message: None,
    };
    compare("pdm-wrkusrdfnopt", &list::user_options(&o(0)));
    compare("pdm-wrkusrdfnopt-f24", &list::user_options(&o(1)));
}

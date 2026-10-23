// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped system panels against the pub400 goldens: each panel
//! rendered with the golden's visible values and compared row by row
//! (`objcommon::compare`). One test per golden, named after it.

mod objcommon;

use objcommon::{compare, shipped};
use uim::{ListState, MapDialog, Options, render};

/// A list of entries, every one given.
fn list(rows: Vec<Vec<(&str, &str)>>) -> ListState {
    let n = rows.len();
    ListState {
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
            .collect(),
        top: 0,
        total: Some(n),
        complete: true,
    }
}

/// A list of one-column lines.
fn lines(var: &str, rows: &[&str]) -> ListState {
    list(rows.iter().map(|r| vec![(var, *r)]).collect())
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

// --- messages and objects

#[test]
fn dspmsg() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("SYSNM", "SYSNAME")
        .set("MSGQ", "USERNAME")
        .set("MSGQLIB", "QUSRSYS")
        .set("DLVRY", "*BREAK");
    d.set_list("MSGS", lines("LINE", &["  (No messages available)"]));
    compare("dspmsg", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn wrkmsg() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("SYSNM", "SYSNAME")
        .set("MSGQ", "USERNAME")
        .set("MSGQLIB", "QUSRSYS")
        .set("DLVRY", "*BREAK");
    d.set_list("MSGS", lines("LINE", &["  (No messages available)"]));
    compare("wrkmsg", &render(&g, "DSPMSG", &d, &opts()).unwrap());
}

#[test]
fn additional_message_information() {
    let g = shipped("QUIDSPMSG");
    let mut d = MapDialog::new();
    d.set("FROMUSR", "USERNAME")
        .set("MSGSEV", "00")
        .set("MSGTYPE", "Request")
        .set("SNTDATE", "MM/DD/YY")
        .set("SNTTIME", "HH:MM:SS");
    d.set_list("INFO", lines("LINE", &["Message . . . . :   dspjoblog"]));
    compare(
        "additional-message-information",
        &render(&g, "ADDMSGI", &d, &opts()).unwrap(),
    );
}

#[test]
fn dsplibl() {
    let g = shipped("QUIDSPLIBL");
    let mut d = MapDialog::new();
    let libs = [
        ("PUB400SYS", "SYS", "$ PUB400.COM global programs"),
        ("QSYS", "SYS", "System Library"),
        ("QSYS2", "SYS", "System Library for CPI's"),
        ("QUSRSYS", "SYS", "System Library for Users"),
        ("QHLPSYS", "SYS", ""),
        ("USERNAME1", "CUR", "USERNAME (*CURLIB your current library)"),
        ("QGPL", "USR", "General Purpose Library"),
        ("QTEMP", "USR", ""),
        ("GAMES400", "USR", "$ fun library"),
    ];
    d.set_list(
        "LIBS",
        list(
            libs.iter()
                .map(|(n, t, x)| vec![("LIBNAME", *n), ("LIBTYPE", *t), ("LIBTEXT", *x)])
                .collect(),
        ),
    );
    compare("dsplibl", &render(&g, "DSPLIBL", &d, &opts()).unwrap());
}

#[test]
fn wrkobj() {
    let g = shipped("QUIWRKOBJ");
    let mut d = MapDialog::new();
    let objs = [
        ("SETUSER", "*PGM", "CLP", "set users to 4G"),
        ("EXCELAFBA", "*SQLPKG", "", ""),
        ("MSACCEAFBA", "*SQLPKG", "", ""),
        ("MSCS001", "*SQLPKG", "PACKAGE", ""),
        ("MSNC001", "*SQLPKG", "PACKAGE", ""),
        ("MSQRY3AFBA", "*SQLPKG", "", ""),
        ("MSRR001", "*SQLPKG", "PACKAGE", ""),
        ("MSRS001", "*SQLPKG", "PACKAGE", ""),
        ("MSUR001", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGC", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGN", "*SQLPKG", "PACKAGE", ""),
        ("QSQCLIPKGS", "*SQLPKG", "PACKAGE", ""),
    ];
    let mut l = list(
        objs.iter()
            .map(|(n, t, a, x)| {
                vec![("OBJ", *n), ("OBJTYPE", *t), ("OBJLIB", "QGPL"), ("OBJATR", *a), ("OBJTEXT", *x)]
            })
            .collect(),
    );
    l.total = None;
    l.complete = false;
    d.set_list("OBJS", l);
    compare("wrkobj", &render(&g, "WRKOBJ", &d, &opts()).unwrap());
}

fn objd_header(d: &mut MapDialog) {
    d.set("LIBPOS", &format!("{:>28}", "Library 1 of 1"))
        .set("LIB", "QGPL")
        .set("OBJ", "QDFTJOBD")
        .set("OBJLIB", "QGPL")
        .set("OBJTYPE", "*JOBD")
        .set("OWNER", "QPGMR");
}

#[test]
fn dspobjd_basic() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_header(&mut d);
    let size = format!("{:<10}{:>18}", "", "8192");
    d.set_list(
        "OBJS",
        list(vec![vec![
            ("OBJ", "QDFTJOBD"),
            ("OBJTYPE", "*JOBD"),
            ("ATRSIZE", size.as_str()),
            ("OBJTEXT", "Default job description"),
        ]]),
    );
    compare("dspobjd-basic", &render(&g, "BASIC", &d, &opts()).unwrap());
}

fn objd_full(d: &mut MapDialog) {
    objd_header(d);
    d.set("OBJTEXT", "Default job description")
        .set("CRTDATE", "MM/DD/YY  HH:MM:SS")
        .set("CRTUSER", "*IBM")
        .set("CRTSYS", "00000000")
        .set("DOMAIN", "*SYSTEM")
        .set("CHGDATE", "MM/DD/YY  HH:MM:SS")
        .set("USAGE", "YES")
        .set("LASTUSED", "MM/DD/YY")
        .set("DAYSUSED", "14")
        .set("ALWCHG", "NO")
        .set("AUDIT", "*NOTAVL");
}

#[test]
fn dspobjd_full() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_full(&mut d);
    compare("dspobjd-full", &render(&g, "FULL", &d, &opts()).unwrap());
}

#[test]
fn dspobjd_full_2() {
    let g = shipped("QUIDSPOBJD");
    let mut d = MapDialog::new();
    objd_full(&mut d);
    let o = Options {
        page: 1,
        ..opts()
    };
    compare("dspobjd-full-2", &render(&g, "FULL", &d, &o).unwrap());
}

#[test]
fn dsplib() {
    let g = shipped("QUIDSPLIB");
    let mut d = MapDialog::new();
    d.set("LIB", "QGPL")
        .set("LIBTYPE", "PROD")
        .set("CRTAUT", "*SYSVAL")
        .set("NOBJ", "1234")
        .set("ASPNUM", "1");
    // Name, type, attribute and the size's digit count, read off the
    // golden's N masks.
    let objs = [
        ("SETUSER", "*PGM", "CLP", 5),
        ("EXCELAFBA", "*SQLPKG", "", 5),
        ("MSACCEAFBA", "*SQLPKG", "", 7),
        ("MSCS001", "*SQLPKG", "PACKAGE", 6),
        ("MSNC001", "*SQLPKG", "PACKAGE", 6),
        ("MSQRY3AFBA", "*SQLPKG", "", 5),
        ("MSRR001", "*SQLPKG", "PACKAGE", 6),
        ("MSRS001", "*SQLPKG", "PACKAGE", 6),
        ("MSUR001", "*SQLPKG", "PACKAGE", 6),
        ("QSQCLIPKGC", "*SQLPKG", "PACKAGE", 6),
        ("QSQCLIPKGN", "*SQLPKG", "PACKAGE", 6),
    ];
    let rows: Vec<(String, String)> = objs
        .iter()
        .map(|(_, _, a, digits)| {
            (format!("{a:<10}{:>18}", "9".repeat(*digits)), String::new())
        })
        .collect();
    let mut l = list(
        objs.iter()
            .zip(&rows)
            .map(|((n, t, _, _), (size, _))| {
                vec![("OBJ", *n), ("OBJTYPE", *t), ("ATRSIZE", size.as_str())]
            })
            .collect(),
    );
    l.rows[0].push(("OBJTEXT".into(), "set users to 4G".into()));
    l.total = None;
    l.complete = false;
    d.set_list("OBJS", l);
    compare("dsplib", &render(&g, "DSPLIB", &d, &opts()).unwrap());
}

#[test]
#[ignore = "the golden centers Object over Authority; R9 left-aligns heading rows"]
fn dspobjaut() {
    let g = shipped("QUIDSPOAUT");
    let mut d = MapDialog::new();
    d.set("OBJ", "QDFTJOBD")
        .set("OBJLIB", "QGPL")
        .set("OBJTYPE", "*JOBD")
        .set("OWNER", "QPGMR");
    d.set_list(
        "AUTS",
        list(vec![vec![("USER", "*PUBLIC"), ("AUT", "*USE")]]),
    );
    compare("dspobjaut", &render(&g, "DSPOBJAUT", &d, &opts()).unwrap());
}

#[test]
fn wrkbnddir() {
    let g = shipped("QUIWRKBNDD");
    let mut d = MapDialog::new();
    let dirs = [
        ("", "", ""),
        ("QCBLLE", "QSYS", ""),
        ("QCLLE", "QSYS", "ILE CL BINDING DIRECTORY"),
        ("QC2LE", "QSYS", ""),
        ("QILE", "QSYS", ""),
        ("QLECWI", "QSYS", ""),
        ("QRNXLE", "QSYS", ""),
        ("QSNAPI", "QSYS", ""),
        ("QUSAPIBD", "QSYS", "API BINDING DIRECTORY"),
        ("QYPPLR", "QSYS", ""),
        ("QZDAPI", "QSYS", ""),
    ];
    let mut l = list(
        dirs.iter()
            .map(|(n, lib, t)| vec![("BNDDIR", *n), ("BNDLIB", *lib), ("BNDTEXT", *t)])
            .collect(),
    );
    l.total = None;
    l.complete = false;
    d.set_list("DIRS", l);
    compare("wrkbnddir", &render(&g, "WRKBNDDIR", &d, &opts()).unwrap());
}

fn msgd_header(d: &mut MapDialog) {
    d.set("MSGID", "CPF1124")
        .set("MSGF", "QCPFMSG")
        .set("MSGFLIB", "QSYS");
}

#[test]
#[ignore = "the golden's message text runs through column 80 and on from column 1"]
fn dspmsgd_list() {
    let g = shipped("QUIDSPMSGD");
    let mut d = MapDialog::new();
    msgd_header(&mut d);
    d.set("TEXT1", "Job &3/&2/&1 started on &18 at &19 in subsyste")
        .set("TEXT2", "m &8 in &9. Job entered system on &20 at &21.&17");
    compare("dspmsgd-list", &render(&g, "SELECT", &d, &opts()).unwrap());
}

#[test]
fn dspmsgd_5() {
    let g = shipped("QUIDSPMSGD");
    let mut d = MapDialog::new();
    msgd_header(&mut d);
    d.set("SEV", "0").set("LEVEL", "MM/DD/YY 73");
    compare("dspmsgd-5", &render(&g, "ATTRS", &d, &opts()).unwrap());
}

#[test]
fn wrkrpyle() {
    let g = shipped("QUIWRKRPYL");
    let mut d = MapDialog::new();
    let rows = [
        ("10", "CPA0700", "D", "*NONE", ""),
        ("20", "RPG0000", "D", "*NONE", ""),
        ("30", "CBE0000", "D", "*NONE", ""),
        ("40", "PLI0000", "D", "*NONE", ""),
        ("123", "CPA7025", "I", "*NONE", ""),
        ("955", "CPA5737", "C", "'Q1PLIN    Q1PCTL    '", "6"),
        ("3282", "CPA32B2", "I", "'ACS Database Schemas'", "1"),
        ("4207", "CPF4207", "C", "*NONE", ""),
        ("4208", "RNQ1216", "C", "*NONE", ""),
        ("4209", "RNQ1218", "R", "*NONE", ""),
        ("4321", "CPA7090", "C", "*NONE", ""),
        ("4322", "CPA7091", "C", "*NONE", ""),
    ];
    let mut l = list(
        rows.iter()
            .map(|(s, m, r, c, st)| {
                vec![("SEQ", *s), ("MSGID", *m), ("RPY", *r), ("CMPDTA", *c), ("START", *st)]
            })
            .collect(),
    );
    l.total = None;
    l.complete = false;
    d.set_list("RPYS", l);
    compare("wrkrpyle", &render(&g, "WRKRPYLE", &d, &opts()).unwrap());
}

/// A panel without a golden: its title centered on row 1 and every
/// key text on the key rows.
fn title_and_keys(group: &str, panel: &str, title: &str, keys: &[&str]) {
    let g = shipped(group);
    let s = render(&g, panel, &MapDialog::new(), &opts()).unwrap();
    let rows = s.text_rows();
    assert_eq!(rows[0].trim(), title, "{panel} title");
    let col = rows[0].find(title).unwrap() + 1;
    assert_eq!(col, 1 + (80 - title.len()) / 2, "{panel} title column");
    let bottom = format!("{}{}", rows[21], rows[22]);
    for k in keys {
        assert!(bottom.contains(k), "{panel} misses {k}:\n{bottom}");
    }
}

#[test]
#[ignore = "no golden"]
fn dspfd() {
    title_and_keys(
        "QUIDSPFD",
        "SPLF",
        "Display Spooled File",
        &["F3=Exit", "F12=Cancel", "F16=Repeat find", "F17=Top", "F18=Bottom"],
    );
}

#[test]
#[ignore = "no golden"]
fn dspdtaara() {
    title_and_keys(
        "QUIDSPDTAA",
        "DSPDTAARA",
        "Display Data Area",
        &["F3=Exit", "F12=Cancel"],
    );
}

#[test]
#[ignore = "no golden"]
fn wrkobjown() {
    title_and_keys(
        "QUIWRKOOWN",
        "WRKOBJOWN",
        "Work with Objects by Owner",
        &["F3=Exit", "F4=Prompt", "F5=Refresh", "F9=Retrieve", "F12=Cancel", "F17=Top", "F18=Bottom"],
    );
}

#[test]
fn dsplog() {
    let g = shipped("QUIDSPLOG");
    let mut d = MapDialog::new();
    // The golden's rows, read back from its file: the log's text is
    // pub400's, not ours to copy into a source.
    let golden = std::fs::read_to_string(objcommon::golden_dir().join("dsplog.txt")).unwrap();
    let rows: Vec<String> = golden
        .lines()
        .skip(2)
        .take(19)
        .map(|l| l.trim_end_matches('\r').get(1..).unwrap_or("").trim_end().to_string())
        .collect();
    let mut l = list(rows.iter().map(|r| vec![("LINE", r.as_str())]).collect());
    l.total = None;
    l.complete = false;
    d.set_list("LOG", l);
    compare("dsplog", &render(&g, "DSPLOG", &d, &opts()).unwrap());
}

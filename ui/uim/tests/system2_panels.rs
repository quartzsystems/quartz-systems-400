// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The second batch of system panels (programs, configuration,
//! security, the editors) against the pub400 goldens: each panel
//! rendered with the golden's visible values and compared row by row.
//! One test per golden, named after it; panels pub400 cannot show are
//! checked for their title and keys.

mod common;

use common::{compare, shipped};
use uim::render::{ListState, MapDialog, Options, render};

/// The system name masked, the copyright line shown.
fn opts() -> Options {
    let g = shipped("QUIDSPCMD");
    Options {
        system: "SYSNAME".into(),
        show_copyright: true,
        copyright: g.copyright.clone().unwrap_or_default(),
        ..Options::default()
    }
}

/// Without the copyright line (a later page).
fn plain() -> Options {
    Options {
        show_copyright: false,
        ..opts()
    }
}

/// A dialog from `(variable, value)` pairs.
fn vars(pairs: &[(&str, &str)]) -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in pairs {
        d.set(k, v);
    }
    d
}

/// A complete list of entries.
fn list(rows: &[&[(&str, &str)]]) -> ListState {
    let mut l = ListState {
        complete: true,
        ..ListState::default()
    };
    for r in rows {
        l.rows.push(
            r.iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        );
    }
    l.total = Some(l.rows.len());
    l
}

/// A panel without a golden: its title centered on row 1 and its keys
/// on the key rows.
fn title_and_keys(group: &str, panel: &str, title: &str, keys: &[&str]) {
    let g = shipped(group);
    let s = render(&g, panel, &MapDialog::new(), &plain()).unwrap();
    let rows = s.text_rows();
    let col = 1 + (80 - title.len()) / 2;
    assert_eq!(
        rows[0].get(col - 1..col - 1 + title.len()),
        Some(title),
        "{panel} title"
    );
    let key_rows = format!("{}\n{}", rows[21], rows[22]);
    for k in keys {
        assert!(
            key_rows.contains(k),
            "{panel}: {k} not on the key rows:\n{key_rows}"
        );
    }
}

// --- panels batch 2: programs, config, security, editors

fn dspcmd_dialog() -> MapDialog {
    vars(&[
        ("CMD", "WRKACTJOB"),
        ("CMDLIB", "QSYS"),
        ("PGM", "QWCCDSAC"),
        ("PGMLIB", "QSYS"),
        ("PGMSTATE", "*SYSTEM"),
        ("SRCF", "S000023563"),
        ("SRCFLIB", "QTEMP"),
        ("SRCMBR", "WRKACTJOB"),
        ("VLDCKR", "*NONE"),
        ("MODE1", "*PROD"),
        ("MODE2", "*DEBUG"),
        ("MODE3", "*SERVICE"),
        ("ALLOW1", "*IMOD      *BMOD      *IREXX"),
        ("ALLOW2", "*BREXX     *BPGM      *IPGM"),
        ("ALLOW3", "*EXEC      *INTERACT  *BATCH"),
        ("ALWLMTUSR", "*NO"),
        ("MAXPOS", "2"),
        ("PMTFILE", "QCPFPMT"),
        ("PMTFILEL", "*LIBL"),
        ("MSGTXT", "*DYNAMIC"),
        ("MSGF", "QCPFMSG"),
        ("MSGFLIB", "*LIBL"),
        ("CURLIB", "*NOCHG"),
        ("PRDLIB", "*NOCHG"),
        ("HLPPNLGRP", "QHWCCMD"),
        ("HLPPNLGRPL", "*LIBL"),
        ("HLPID", "WRKACTJOB"),
        ("PMTOVRPGM", "*NONE"),
        ("ENBGUI", "*YES"),
        ("THDSAFE", "*NO"),
        ("MLTTHDACN", "*SYSVAL"),
        ("CCSID", "37"),
    ])
}

#[test]
fn dspcmd() {
    let g = shipped("QUIDSPCMD");
    let s = render(&g, "DSPCMD", &dspcmd_dialog(), &opts()).unwrap();
    compare("dspcmd", &s);
}

#[test]
fn dspcmd_2() {
    let g = shipped("QUIDSPCMD");
    let o = Options { page: 1, ..plain() };
    let s = render(&g, "DSPCMD", &dspcmd_dialog(), &o).unwrap();
    compare("dspcmd-2", &s);
}

/// The program header as pub400 shows `QSYS/QZDASOINIT`.
fn pgm_dialog(pos: &str, detail: &str) -> MapDialog {
    vars(&[
        ("DSPPOS", pos),
        ("PGM", "QZDASOINIT"),
        ("PGMLIB", "QSYS"),
        ("OWNER", "QSYS"),
        ("PGMATTR", "CPPLE"),
        ("DETAIL", detail),
        ("CRTDT", "MM/DD/YY  HH:MM:SS"),
        ("PGMTYPE", "ILE"),
        ("PEPMOD", "QZDASOIT"),
        ("PEPMODL", "QBUILDSS1"),
        ("ACTGRP", "*CALLER"),
        ("SHRACTGRP", "*NO"),
        ("USRPRF", "*USER"),
        ("USEADPAUT", "*YES"),
        ("CCSID", "65535"),
        ("NBRMOD", "1"),
        ("NBRSRV", "3"),
        ("NBRUNRES", "0"),
        ("NBRCPYR", "1"),
        ("ALLCRTDTA", "*NO"),
        ("OBSCMPR", "*NO"),
        ("RUNCMPR", "*NO"),
        ("ALWUPD", "*YES"),
        ("ALWLIBUPD", "*NO"),
        ("PRFDTA", "*NOCOL"),
        ("TSPEP", "\u{d6}"),
        ("TSMOD", "\u{d6}"),
        ("SIZECUR", "56"),
        ("SIZEMAX", "268435456"),
        ("MODCUR", "1"),
        ("MODMAX", "32767"),
        ("SRVCUR", "3"),
        ("SRVMAX", "32766"),
    ])
}

/// Entries from rows of `(variable, value)` arrays.
fn rows<const N: usize>(rows: &[[(&str, &str); N]]) -> ListState {
    let refs: Vec<&[(&str, &str)]> = rows.iter().map(|r| &r[..]).collect();
    list(&refs)
}

#[test]
fn dsppgm() {
    let g = shipped("QUIDSPPGM");
    let d = pgm_dialog(" Display 1 of 7", "*BASIC");
    compare("dsppgm", &render(&g, "PGMBASIC", &d, &opts()).unwrap());
}

#[test]
fn dsppgm_pgdn() {
    let g = shipped("QUIDSPPGM");
    let d = pgm_dialog(" Display 1 of 7", "*BASIC");
    let o = Options { page: 1, ..plain() };
    compare("dsppgm-pgdn", &render(&g, "PGMBASIC", &d, &o).unwrap());
}

#[test]
fn dsppgm_2() {
    let g = shipped("QUIDSPPGM");
    let d = pgm_dialog(" Display 2 of 7", "*SIZE");
    compare("dsppgm-2", &render(&g, "PGMSIZE", &d, &plain()).unwrap());
}

#[test]
fn dsppgm_3() {
    let g = shipped("QUIDSPPGM");
    let mut d = pgm_dialog(" Display 3 of 7", "*MODULE");
    d.set_list(
        "MODS",
        rows(&[[
            ("MODNAME", "QZDASOIT"),
            ("MODLIB", "QBUILDSS1"),
            ("MODATTR", "CPPLE"),
            ("MODDATE", "MM/DD/YY"),
            ("MODOPT", "40"),
            ("MODDBG", "*NO"),
        ]]),
    );
    compare("dsppgm-3", &render(&g, "PGMMOD", &d, &plain()).unwrap());
}

/// Service program entries: name, activation, signature.
fn srv_rows(srv: &[(&str, &str, &str)]) -> ListState {
    let r: Vec<[(&str, &str); 4]> = srv
        .iter()
        .map(|(n, a, s)| [("SRVNAME", *n), ("SRVLIB", "QSYS"), ("SRVACT", *a), ("SRVSIG", *s)])
        .collect();
    rows(&r)
}

#[test]
fn dsppgm_4() {
    let g = shipped("QUIDSPPGM");
    let mut d = pgm_dialog(" Display 4 of 7", "*SRVPGM");
    d.set_list(
        "SRVS",
        srv_rows(&[
            ("QZDASRV", "*IMMED", "D8E9C4C1E2D9E5404040404040404040"),
            ("QC2UTIL1", "*IMMED", "CB1F803582C31AC25B4DCBDD1B6D0948"),
            ("QLEAWI", "*IMMED", "44F70FABA08585397BDF0CF195F82EC1"),
        ]),
    );
    compare("dsppgm-4", &render(&g, "PGMSRV", &d, &plain()).unwrap());
}

/// A heading centered over the 80 columns, as the driver pads it.
fn centered(text: &str) -> String {
    format!("{}{text}", " ".repeat((80 - text.len()) / 2 - 1))
}

#[test]
fn dsppgm_5() {
    let g = shipped("QUIDSPPGM");
    let mut d = pgm_dialog(" Display 5 of 7", "*ACTGRPEXP");
    d.set("EXPHEAD", &centered("Activation group exports:"))
        .set("EXPEMPTY", "(No activation group exports)");
    d.set_list("EXPS", list(&[]));
    let o = Options { view: 1, ..plain() };
    compare("dsppgm-5", &render(&g, "PGMEXP", &d, &o).unwrap());
}

/// The service program header as pub400 shows `QSYS/QLEAWI`.
fn srv_dialog(pos: &str, detail: &str) -> MapDialog {
    vars(&[
        ("DSPPOS", pos),
        ("PGM", "QLEAWI"),
        ("PGMLIB", "QSYS"),
        ("OWNER", "QSECOFR"),
        ("PGMATTR", "CPPLE"),
        ("DETAIL", detail),
        ("CRTDT", "MM/DD/YY  HH:MM:SS"),
        ("EXPSRCF", "S000028822"),
        ("EXPSRCFL", "$BLDSS1"),
        ("EXPMBR", "S000028822"),
        ("ACTGRP", "*CALLER"),
        ("SHRACTGRP", "*NO"),
        ("CURSIG", "44F70FABA08585397BDF0CF195F82EC1"),
        ("SIZECUR", "488"),
        ("SIZEMAX", "268435456"),
        ("MODCUR", "23"),
        ("MODMAX", "32767"),
        ("SRVCUR", "8"),
        ("SRVMAX", "32766"),
    ])
}

#[test]
#[ignore = "the golden wraps the 32-character signature into column 1 of row 18 and keeps two key rows"]
fn dspsrvpgm() {
    let g = shipped("QUIDSPPGM");
    let d = srv_dialog("Display 1 of 10", "*BASIC");
    compare("dspsrvpgm", &render(&g, "SRVBASIC", &d, &opts()).unwrap());
}

#[test]
#[ignore = "the golden keeps two key rows (F11 shows on the first display), moving Press Enter to row 20"]
fn dspsrvpgm_2() {
    let g = shipped("QUIDSPPGM");
    let d = srv_dialog("Display 2 of 10", "*SIZE");
    compare("dspsrvpgm-2", &render(&g, "SRVSIZE", &d, &plain()).unwrap());
}

#[test]
fn dspsrvpgm_3() {
    let g = shipped("QUIDSPPGM");
    let mut d = srv_dialog("Display 3 of 10", "*MODULE");
    let names = [
        "QLEDEH", "QLETOOL", "QLEPM", "QLETREC", "QLESCAW", "QLESC", "QLERTX", "QLENCOD", "QLEX1",
    ];
    let r: Vec<[(&str, &str); 6]> = names
        .iter()
        .map(|n| {
            [
                ("MODNAME", *n),
                ("MODLIB", "QBUILDSS1"),
                ("MODATTR", "CPPLE"),
                ("MODDATE", "MM/DD/YY"),
                ("MODOPT", "40"),
                ("MODDBG", "*NO"),
            ]
        })
        .collect();
    d.set_list("MODS", rows(&r));
    compare("dspsrvpgm-3", &render(&g, "SRVMOD", &d, &plain()).unwrap());
}

#[test]
fn dspsrvpgm_4() {
    let g = shipped("QUIDSPPGM");
    let mut d = srv_dialog("Display 4 of 10", "*SRVPGM");
    d.set_list(
        "SRVS",
        srv_rows(&[
            ("QLESYS", "*IMMED", "D8D3C5E2E8E240404040404040404040"),
            ("QP0WEXIT", "*DEFER", "D8D7F0E6C5E7C9E34040404040404040"),
            ("QLECWI", "*DEFER", "FFB714CFF747ECD0DB3DFAFB271C4C8E"),
            ("QLESSNFP", "*IMMED", "D8D3C5E2E2D5C6D74040404040404040"),
            ("QLESPI", "*IMMED", "D8D3C5E2D7C940404040404040404040"),
            ("QC2UTIL1", "*IMMED", "CB1F803582C31AC25B4DCBDD1B6D0948"),
            ("QC2UTIL2", "*DEFER", "A11653CED63C16950B06C44C1F6E95C2"),
            ("QX", "*DEFER", "0"),
        ]),
    );
    compare("dspsrvpgm-4", &render(&g, "SRVSRV", &d, &plain()).unwrap());
}

#[test]
fn dspsrvpgm_5() {
    let g = shipped("QUIDSPPGM");
    let mut d = srv_dialog("Display 5 of 10", "*PROCEXP");
    d.set("EXPHEAD", &centered("Procedure Exports:"));
    let names = [
        "Q LE leDefaultEh",
        "CEEMRCR",
        "CEEMGET",
        "CEEMOUT",
        "CEEMSG",
        "CEENCOD",
        "CEEDCOD",
        "Q LE leBdyCh",
        "Q LE leBdyEpilog",
        "CEE4FCB",
        "CEEX",
    ];
    let r: Vec<[(&str, &str); 2]> = names
        .iter()
        .map(|n| [("PROCNAME", *n), ("ARGOPT", "*NO")])
        .collect();
    d.set_list("EXPS", rows(&r));
    compare("dspsrvpgm-5", &render(&g, "SRVEXP", &d, &plain()).unwrap());
}

#[test]
fn dspbnddir() {
    let g = shipped("QUIDSPBNDD");
    let mut d = vars(&[("BNDDIR", "QC2LE"), ("BNDLIB", "QSYS")]);
    d.set(
        "CRTHEAD",
        &format!("{}Creation{}", "-".repeat(7), "-".repeat(8)),
    );
    let names = [
        "QC2SYS", "QC2IO", "QC2NIO", "QC2UTIL1", "QC2UTIL2", "QC2UTIL3", "QC2IFS", "QC2NIFS",
        "QC2POSIX", "QC2NPOSX", "QC2LOCAL", "QC2ULOCL", "QC2VLOCC", "QC2VLOCL", "QC2ASYSIG",
        "QC2X",
    ];
    let r: Vec<[(&str, &str); 6]> = names
        .iter()
        .map(|n| {
            [
                ("ENTOBJ", *n),
                ("ENTTYPE", "*SRVPGM"),
                ("ENTLIB", "QSYS"),
                ("ENTACT", "*IMMED"),
                ("ENTDATE", "MM/DD/YY"),
                ("ENTTIME", "HH:MM:SS"),
            ]
        })
        .collect();
    d.set_list("ENTS", rows(&r));
    compare("dspbnddir", &render(&g, "DSPBNDDIR", &d, &opts()).unwrap());
}

#[test]
fn wrkdevd() {
    let g = shipped("QUIWRKDEVD");
    let mut d = MapDialog::new();
    let names = [
        "ABCDEFG", "ABCDEFG1", "ACCESS", "ADRIANO", "AUGUSTA_CH", "AUGUSTE", "BORIS1993P",
        "CCHARPRT", "CDHOME", "CDX",
    ];
    let r: Vec<[(&str, &str); 3]> = names
        .iter()
        .map(|n| {
            [
                ("DEVD", *n),
                ("DEVTYPE", "3812"),
                ("DEVTEXT", "Device created for PUB400."),
            ]
        })
        .collect();
    d.set_list("DEVS", rows(&r));
    compare("wrkdevd", &render(&g, "WRKDEVD", &d, &plain()).unwrap());
}

#[test]
fn wrkhdwrsc() {
    let g = shipped("QUIHDWRSC");
    let mut d = vars(&[("RSCKIND", "Communication")]);
    let rsc = [
        ("CMB09", "6B03", "Operational", "Comm Processor"),
        ("  LIN03", "6B03", "Operational", "Comm Adapter"),
        ("    CMN01", "6B03", "Operational", "Comm Port"),
        ("CMB10", "6B03", "Operational", "Comm Processor"),
        ("  LIN01", "6B03", "Operational", "Comm Adapter"),
        ("    CMN02", "6B03", "Operational", "Comm Port"),
        ("CMB11", "268C", "Not detected", "Comm Processor"),
        ("  LIN02", "268C", "Not detected", "LAN Adapter"),
        ("    CMN03", "268C", "Not detected", "Ethernet Port"),
        ("CMB12", "268C", "Operational", "Combined function IOP"),
        ("  LIN04", "6B26", "Operational", "Comm Adapter"),
        ("CMB17", "268C", "Operational", "Comm Processor"),
        ("  LIN05", "268C", "Operational", "LAN Adapter"),
        ("    CMN04", "268C", "Operational", "Ethernet Port"),
    ];
    let r: Vec<[(&str, &str); 4]> = rsc
        .iter()
        .map(|(n, t, s, x)| [("RSCNAME", *n), ("RSCTYPE", *t), ("RSCSTS", *s), ("RSCTEXT", *x)])
        .collect();
    d.set_list("RSCS", rows(&r));
    compare("wrkhdwrsc", &render(&g, "WRKHDWRSC", &d, &plain()).unwrap());
}

#[test]
#[ignore = "no golden: DSPHDWRSC is not authorized to the pub400 account"]
fn dsphdwrsc() {
    let g = shipped("QUIHDWRSC");
    let d = vars(&[("RSCKIND", "Communication")]);
    let s = render(&g, "DSPHDWRSC", &d, &plain()).unwrap();
    let title = "Display Communication Resources";
    let rows = s.text_rows();
    assert!(rows[0].contains(title));
    assert!(rows[22].contains("F3=Exit") && rows[22].contains("F12=Cancel"));
}

/// `---Name---` heading `width` wide, the name centered as the driver
/// makes it.
fn dashed(name: &str, width: usize) -> String {
    let left = (width - name.len()) / 2;
    format!(
        "{}{name}{}",
        "-".repeat(left),
        "-".repeat(width - name.len() - left)
    )
}

#[test]

fn dspusrprf_objaut() {
    let g = shipped("QUIUPRFTYP");
    let mut d = vars(&[("UPNAME", "USERNAME"), ("OBJHEAD", &dashed("Object", 27))]);
    d.set_cond("OBJVIEW", true).set_cond("DTAVIEW", false);
    d.set_list(
        "AUTS",
        rows(&[[
            ("OBJ", "USERNAME"),
            ("OBJLIB", "QSYS"),
            ("OBJTYPE", "*USRPRF"),
            ("ASPDEV", "*SYSBAS"),
            ("OPR", " X"),
            ("MGT", " X"),
        ]]),
    );
    compare("dspusrprf-objaut", &render(&g, "OBJAUT", &d, &opts()).unwrap());
}

#[test]

fn dspusrprf_objpgp() {
    let g = shipped("QUIUPRFTYP");
    let mut d = vars(&[
        ("UPNAME", "USERNAME"),
        ("TOTAL", "0"),
        ("OBJHEAD", &dashed("Object", 27)),
    ]);
    d.set_cond("OBJVIEW", true).set_cond("DTAVIEW", false);
    d.set_list("AUTS", list(&[]));
    compare("dspusrprf-objpgp", &render(&g, "OBJPGP", &d, &opts()).unwrap());
}

#[test]
#[ignore = "the golden starts Authority one column left of Holder and its field (60 against 61)"]
fn dspusrprf_objown() {
    let g = shipped("QUIUPRFTYP");
    let mut d = vars(&[("UPNAME", "USERNAME"), ("TOTAL", "173")]);
    let objs = [
        ("RLATEST", "*PGM"),
        ("QSTRCV0001", "*JRNRCV"),
        ("QSTJRN", "*JRN"),
        ("ACTIVE", "*FILE"),
        ("BIGORD", "*FILE"),
        ("CUSBYNAME", "*FILE"),
        ("CUSORD", "*FILE"),
        ("CUSTOMER", "*FILE"),
        ("EVENTS", "*FILE"),
        ("FFD", "*FILE"),
        ("INVENTORY", "*FILE"),
        ("JRN", "*FILE"),
        ("MORE", "*FILE"),
    ];
    let r: Vec<[(&str, &str); 4]> = objs
        .iter()
        .map(|(n, t)| {
            [
                ("OOBJ", *n),
                ("OOBJLIB", "USERNAMEB"),
                ("OOBJTYPE", *t),
                ("OASPDEV", "*SYSBAS"),
            ]
        })
        .collect();
    d.set_list("OWNS", rows(&r));
    compare("dspusrprf-objown", &render(&g, "OBJOWN", &d, &opts()).unwrap());
}

#[test]
fn dspusrprf_cmdaut() {
    let g = shipped("QUIUPRFTYP");
    let mut d = vars(&[("UPNAME", "USERNAME")]);
    d.set_list("CMDS", list(&[]));
    compare("dspusrprf-cmdaut", &render(&g, "CMDAUT", &d, &opts()).unwrap());
}

#[test]
fn dspusrprf_devaut() {
    let g = shipped("QUIUPRFTYP");
    let mut d = vars(&[("UPNAME", "USERNAME")]);
    d.set_list("DEVS", list(&[]));
    compare("dspusrprf-devaut", &render(&g, "DEVAUT", &d, &opts()).unwrap());
}

#[test]
#[ignore = "no golden: the pub400 account is not a group profile"]
fn dspusrprf_grpmbr() {
    title_and_keys(
        "QUIUPRFTYP",
        "GRPMBR",
        "Display Group Members",
        &["F3=Exit", "F12=Cancel"],
    );
}

/// EDTLIBL's rows: sequence numbers ten apart, three columns of 15.
fn libl_rows(libs: &[(&str, &str)]) -> ListState {
    let name = |seq: usize| {
        libs.iter()
            .find(|(s, _)| s.parse::<usize>().ok() == Some(seq))
            .map_or("", |(_, n)| *n)
    };
    let mut r: Vec<[(String, String); 6]> = Vec::new();
    for i in 0..15 {
        let s = [i * 10, 150 + i * 10, 300 + i * 10];
        r.push([
            ("SEQ1".into(), s[0].to_string()),
            ("LIB1".into(), name(s[0]).into()),
            ("SEQ2".into(), s[1].to_string()),
            ("LIB2".into(), name(s[1]).into()),
            ("SEQ3".into(), s[2].to_string()),
            ("LIB3".into(), name(s[2]).into()),
        ]);
    }
    let mut l = ListState {
        complete: false,
        ..ListState::default()
    };
    l.rows = r.into_iter().map(Vec::from).collect();
    l
}

#[test]
fn edtlibl() {
    let g = shipped("QUIEDTLIBL");
    let mut d = MapDialog::new();
    d.set_list(
        "LIBS",
        libl_rows(&[("10", "QGPL"), ("20", "QTEMP"), ("30", "GAMES400")]),
    );
    compare("edtlibl", &render(&g, "EDTLIBL", &d, &plain()).unwrap());
}

/// The editor's header as pub400 shows the account's own library.
fn edtaut_dialog(view: &str) -> MapDialog {
    vars(&[
        ("AUTVIEW", view),
        ("OBJ", "USERNAME1"),
        ("OBJLIB", "QSYS"),
        ("OBJTYPE", "*LIB"),
        ("OWNER", "USERNAME"),
        ("OBJHEAD", &dashed("Object", 27)),
    ])
}

#[test]
fn edtobjaut() {
    let g = shipped("QUIEDTAUT");
    let mut d = edtaut_dialog("OBJ");
    d.set_list(
        "AUTS",
        rows(&[
            [("USER", "*PUBLIC"), ("AUT", "*EXCLUDE")],
            [("USER", "USERNAME"), ("AUT", "*ALL")],
        ]),
    );
    compare("edtobjaut", &render(&g, "EDTOBJAUT", &d, &opts()).unwrap());
}

#[test]
fn edtobjaut_f11() {
    let g = shipped("QUIEDTAUT");
    let mut d = edtaut_dialog("DTL");
    let x = [
        ("USER", "USERNAME"),
        ("AUT", "*ALL"),
        ("OPR", "X"),
        ("MGT", "X"),
        ("EXIST", "X"),
        ("ALTER", "X"),
        ("REF", "X"),
    ];
    let pubrow = [
        ("USER", "*PUBLIC"),
        ("AUT", "*EXCLUDE"),
        ("OPR", ""),
        ("MGT", ""),
        ("EXIST", ""),
        ("ALTER", ""),
        ("REF", ""),
    ];
    d.set_list("AUTS", rows(&[pubrow, x]));
    let o = Options { view: 1, ..plain() };
    compare("edtobjaut-f11", &render(&g, "EDTOBJAUT", &d, &o).unwrap());
}

#[test]
fn edtobjaut_f6() {
    let g = shipped("QUIEDTAUT");
    let mut d = edtaut_dialog("DTL");
    let blank: Vec<[(&str, &str); 1]> = (0..12).map(|_| [("USER", "")]).collect();
    d.set_list("AUTS", rows(&blank));
    compare("edtobjaut-f6", &render(&g, "ADDUSERS", &d, &plain()).unwrap());
}

#[test]
#[ignore = "no golden: pub400 has no authorization list the account may edit"]
fn edtautl() {
    title_and_keys(
        "QUIEDTAUT",
        "EDTAUTL",
        "Edit Authorization List",
        &["F3=Exit", "F5=Refresh", "F6=Add new users", "F12=Cancel"],
    );
}

#[test]
#[ignore = "no golden: pub400's QSYS holds no module the account may display"]
fn dspmod() {
    title_and_keys(
        "QUIDSPPGM",
        "MODBASIC",
        "Display Module Information",
        &["F3=Exit", "F12=Cancel"],
    );
}

#[test]
#[ignore = "no golden: the module's procedure list"]
fn dspmod_2() {
    title_and_keys(
        "QUIDSPPGM",
        "MODPROC",
        "Display Module Information",
        &["F3=Exit", "F12=Cancel", "F17=Top", "F18=Bottom"],
    );
}

#[test]
#[ignore = "no golden: DSPPTF is not authorized to the pub400 account"]
fn dspptf() {
    title_and_keys(
        "QUIDSPPTF",
        "DSPPTF",
        "Display PTF Status",
        &["F3=Exit", "F11=Display alternate view", "F12=Cancel"],
    );
}

#[test]
#[ignore = "no golden: pub400 has no authorization list the account may display"]
fn dspautlobj() {
    title_and_keys(
        "QUIAUTLOBJ",
        "DSPAUTLOBJ",
        "Display Authorization List Objects",
        &["F3=Exit", "F12=Cancel", "F17=Top", "F18=Bottom"],
    );
}

#[test]
#[ignore = "no golden: DSPAUDJRNE needs *AUDIT, which the pub400 account lacks"]
fn dspaudjrne() {
    let g = shipped("QUIRPTVW");
    let d = vars(&[("RPTTITLE", "Display Audit Journal Entries")]);
    let s = render(&g, "RPTVIEW", &d, &plain()).unwrap();
    let rows = s.text_rows();
    assert!(rows[0].contains("Display Audit Journal Entries"));
    let keys = format!("{}\n{}", rows[21], rows[22]);
    for k in ["F3=Exit", "F12=Cancel", "F19=Left", "F20=Right"] {
        assert!(keys.contains(k), "{k}:\n{keys}");
    }
}

#[test]
#[ignore = "the golden is DSPSPLF over the spooled report; ours shows the lines directly"]
fn dsppgmref() {
    let g = shipped("QUIRPTVW");
    let d = vars(&[("RPTTITLE", "Display Spooled File")]);
    compare("dsppgmref", &render(&g, "RPTVIEW", &d, &plain()).unwrap());
}

#[test]
#[ignore = "no golden: WRKSYSVAL is not authorized to the pub400 account"]
fn chgsysval() {
    title_and_keys(
        "QUICHGSV",
        "CHGSV",
        "Change System Value",
        &["F3=Exit", "F5=Refresh", "F12=Cancel"],
    );
}

#[test]
#[ignore = "no golden: WRKSYSVAL is not authorized to the pub400 account"]
fn chgsysval_list() {
    title_and_keys(
        "QUICHGSV",
        "CHGSVL",
        "Change System Value",
        &["F3=Exit", "F5=Refresh", "F12=Cancel"],
    );
}

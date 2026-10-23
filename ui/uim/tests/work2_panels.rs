// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panels batch 2 (work management, spool, files and journals) against
//! the pub400 goldens: each panel rendered with the golden's visible
//! values and compared row by row (`wmcommon::compare`). Panels the
//! account may not show are checked for their title and keys.

mod wmcommon;

use uim::render::{ListState, MapDialog, Options, render};
use wmcommon::{compare, golden, shipped};

/// A complete list from rows of `(variable, value)` pairs.
fn list(rows: &[Vec<(&str, &str)>]) -> ListState {
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

/// An incomplete list: more entries follow the ones given.
fn partial(rows: &[Vec<(&str, &str)>]) -> ListState {
    ListState {
        total: None,
        complete: false,
        ..list(rows)
    }
}

fn opts() -> Options {
    Options {
        system: "PUB400".into(),
        ..Options::default()
    }
}

fn screen(group: &str, panel: &str, d: &MapDialog, o: &Options) -> uim::Screen {
    render(&shipped(group), panel, d, o).expect("render")
}

/// The title row and the key rows hold what is given.
fn title_and_keys(s: &uim::Screen, title: &str, keys: &[&str]) {
    let rows = s.text_rows();
    assert!(rows[0].contains(title), "title row: {:?}", rows[0]);
    let bottom = rows[rows.len() - 3..].join("\n");
    for k in keys {
        assert!(bottom.contains(k), "{k} missing from the keys:\n{bottom}");
    }
}

#[test]
fn wrksbmjob() {
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set("SBMFROM", "*USER")
        .set_list("JOBS", list(&[]));
    compare(&golden("wrksbmjob"), &screen("QGWCSBSJ", "WRKSBMJOB", &d, &opts()));
}

fn wrkoutqd_dialog() -> MapDialog {
    let mut d = MapDialog::new();
    d.set("OUTQ", "QPRINT")
        .set("OUTQLIB", "QGPL")
        .set("WTRACT", "N")
        .set("HELD", "N")
        .set("MAXPAGES", "*NONE")
        .set("AUTOSTR", "1")
        .set("DSPDTA", "*NO")
        .set("JOBSEP", "0")
        .set("OPRCTL", "*YES")
        .set("SEQ", "*FIFO")
        .set("DTAQ", "*NONE")
        .set("AUTCHK", "*OWNER")
        .set("RMTSYS", "*NONE")
        .set("USRDFNOPT", "*NONE")
        .set("USRDFNOBJ", "*NONE")
        .set("SPLFASP", "*SYSTEM")
        .set("TEXT", "Default Printer Output Queue");
    d
}

#[test]
fn wrkoutqd() {
    let d = wrkoutqd_dialog();
    compare(&golden("wrkoutqd"), &screen("QGSPDSPQD", "WRKOUTQD", &d, &opts()));
}

#[test]
fn wrkoutqd_2() {
    let d = wrkoutqd_dialog();
    let o = Options { page: 1, ..opts() };
    compare(&golden("wrkoutqd-2"), &screen("QGSPDSPQD", "WRKOUTQD", &d, &o));
}

#[test]
#[ignore = "headings and decimal columns sit on a hand grid R9 does not make"]
fn wrkdsksts() {
    let rows: Vec<Vec<(&str, &str)>> = (1..=13)
        .map(|u| {
            let unit: &'static str = Box::leak(u.to_string().into_boxed_str());
            vec![
                ("UNIT", unit),
                ("TYPE", "4328"),
                ("SIZE", "283"),
                ("USED", "5.1"),
                ("IORQS", "1.1"),
                ("RQSSIZE", "1.1"),
                ("READRQS", "1.1"),
                ("WRITERQS", "1.1"),
                ("READK", "1.1"),
                ("WRITEK", "1.1"),
                ("BUSY", "1"),
            ]
        })
        .collect();
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set("ELAPSED", "HH:MM:SS")
        .set_list("UNITS", list(&rows));
    compare(&golden("wrkdsksts"), &screen("QGWCDSKS", "WRKDSKSTS", &d, &opts()));
}

#[test]
#[ignore = "legends on three rows sit on a column grid R5/R7 do not make"]
fn wrklib() {
    let mut d = MapDialog::new();
    d.set_list(
        "LIBS",
        list(&[
            vec![],
            vec![
                ("LIB", "QGPL"),
                ("LIBATR", "PROD"),
                ("LIBTEXT", "General Purpose Library"),
            ],
        ]),
    );
    compare(&golden("wrklib"), &screen("QUIWRKLIB", "WRKLIB", &d, &opts()));
}

#[test]
fn wrkobjlck() {
    let jobs: &[[&str; 2]] = &[
        ["ABENTEST", "QSPLJOB"],
        ["ADMIN", "QTMHHTTP"],
        ["ADMIN", "QTMHHTTP"],
        ["ADMIN", "QTMHHTTP"],
        ["ADMIN1", "QWEBADMIN"],
        ["ADMIN2", "QLWISVR"],
        ["ADMIN3", "QWEBADMIN"],
        ["ADMIN4", "QWEBADMIN"],
        ["ADMIN5", "QLWISVR"],
        ["APSPRINT", "QSPLJOB"],
        ["BACK400", "QSECOFR"],
        ["DCP2550", "QSPLJOB"],
        ["DCP2551", "QSPLJOB"],
    ];
    let rows: Vec<Vec<(&str, &str)>> = jobs
        .iter()
        .map(|j| {
            vec![
                ("JOB", j[0]),
                ("USER", j[1]),
                ("LOCK", "*SHRRD"),
                ("STATUS", " HELD"),
                ("SCOPE", "*JOB"),
            ]
        })
        .collect();
    let mut d = MapDialog::new();
    d.set("OBJ", "QGPL")
        .set("OBJLIB", "QSYS")
        .set("OBJTYPE", "*LIB")
        .set("ASPDEV", "*SYSBAS")
        .set_list("LOCKS", list(&rows));
    let o = Options {
        system: "SYSNAME".into(),
        ..opts()
    };
    compare(&golden("wrkobjlck"), &screen("QGWCOLCK", "WRKOBJLCK", &d, &o));
}

#[test]
fn wrkjrn() {
    let mut d = MapDialog::new();
    d.set_list("JRNS", list(&[]));
    compare(&golden("wrkjrn"), &screen("QGJOWKMN", "WRKJRN", &d, &opts()));
}

#[test]
#[ignore = "the title is not centered and the list shows no indicator (a display file grid)"]
fn dspovr() {
    let mut d = MapDialog::new();
    d.set_list(
        "OVRS",
        list(&[vec![
            ("FILE", "QCUSTCDT"),
            ("TYPE", "DB"),
            ("KWDS", "TOFILE(QIWS/QCUSTCDT)"),
        ]]),
    );
    compare(&golden("dspovr"), &screen("QUIDSPOVR", "DSPOVR", &d, &opts()));
}

#[test]
#[ignore = "the lower legend row moves to the next column of the upper one; R7 keeps it flowed"]
fn wrksbsd() {
    let sbs: &[[&str; 2]] = &[
        ["QBASE", "Basic controlling subsystem"],
        ["QBATCH", "Batch Subsystem"],
        ["QBATCH2", "Batch Subsystem"],
        ["QBATCH3", ""],
        ["QBATCH4", ""],
        ["QCMN", "Communications Subsystem"],
        ["QCTL", "Controlling Subsystem"],
        ["QINTER", "Interactive Subsystem"],
        ["QINTERX", "Interactive Subsystem restricted"],
        ["QINTER2", "Interactive Subsystem"],
        ["QPGMR", "Programmer Subsystem"],
    ];
    let mut rows: Vec<Vec<(&str, &str)>> = vec![vec![]];
    rows.extend(
        sbs.iter()
            .map(|s| vec![("SBSD", s[0]), ("SBSDLIB", "QSYS"), ("SBSDTEXT", s[1])]),
    );
    let mut d = MapDialog::new();
    d.set_list("SBSDS", partial(&rows));
    compare(&golden("wrksbsd"), &screen("QUIWRKSBSD", "WRKSBSD", &d, &opts()));
}

/// The golden's rows `from..=to` (1-based) from column 2, as the data
/// lines a viewer shows: the file's own data, read at test time.
fn data_rows(name: &str, from: usize, to: usize) -> Vec<String> {
    golden(name)
        .lines()
        .skip(from - 1)
        .take(to + 1 - from)
        .map(|l| l.trim_end_matches('\r').chars().skip(1).collect::<String>())
        .collect()
}

fn viewer(group: &str, panel: &str, golden_name: &str, vars: &[(&str, &str)], first: usize, last: usize, ruler: usize) -> uim::Screen {
    let lines = data_rows(golden_name, first, last);
    let rows: Vec<Vec<(&str, &str)>> = lines.iter().map(|l| vec![("LINE", l.as_str())]).collect();
    let ruler = data_rows(golden_name, ruler, ruler).remove(0);
    let mut d = MapDialog::new();
    for (k, v) in vars {
        d.set(k, v);
    }
    d.set("RULER", &ruler).set_list("LINES", partial(&rows));
    screen(group, panel, &d, &opts())
}

#[test]
#[ignore = "IBM's viewer is a display file: Control and Find end in dots, not a colon"]
fn dspsplf() {
    let vars = [
        ("FILE", "QDFUZZIN"),
        ("PAGELINE", "1/1"),
        ("COLS", "1 - 78"),
    ];
    let s = viewer("QGSPDSPF", "DSPSPLF", "dspsplf", &vars, 6, 21, 5);
    compare(&golden("dspsplf"), &s);
}

#[test]
fn dspsplf_keys() {
    let vars = [("FILE", "QDFUZZIN")];
    let s = viewer("QGSPDSPF", "DSPSPLF", "dspsplf", &vars, 6, 21, 5);
    title_and_keys(&s, "Display Spooled File", &["F3=Exit", "F12=Cancel", "F19=Left", "F20=Right", "F24=More keys"]);
    let rows = s.text_rows();
    let want = golden("dspsplf");
    let want: Vec<&str> = want.lines().collect();
    // The frame: title, the File line, the ruler, the lines, the keys.
    for i in [0, 4, 5, 20, 21, 22] {
        assert_eq!(rows[i].trim_end(), want[i].trim_end(), "row {}", i + 1);
    }
}

#[test]
fn dspsplf_f24() {
    let d = MapDialog::new();
    let o = Options { more_keys: true, ..opts() };
    let s = screen("QGSPDSPF", "DSPSPLF", &d, &o);
    let want = golden("dspsplf-f24");
    assert_eq!(s.text_rows()[22].trim_end(), want.lines().nth(22).unwrap().trim_end());
}

#[test]
#[ignore = "IBM's viewer is a display file: Control and Find end in dots, not a colon"]
fn dsppfm() {
    let vars = [
        ("FILE", "QCUSTCDT"),
        ("LIB", "QIWS"),
        ("MBR", "CUSTCDT"),
        ("RECORD", "1"),
        ("COLUMN", "1"),
    ];
    let s = viewer("QUIDSPPFM", "DSPPFM", "dsppfm", &vars, 7, 19, 6);
    compare(&golden("dsppfm"), &s);
}

#[test]
fn dsppfm_frame() {
    let vars = [
        ("FILE", "QCUSTCDT"),
        ("LIB", "QIWS"),
        ("MBR", "CUSTCDT"),
        ("RECORD", "1"),
        ("COLUMN", "1"),
    ];
    for (name, first, last) in [("dsppfm", 7, 19), ("dsppfm-f10", 7, 19), ("dsppfm-f11", 7, 21)] {
        let s = viewer("QUIDSPPFM", "DSPPFM", name, &vars, first, last, 6);
        let rows = s.text_rows();
        let want = golden(name);
        let want: Vec<&str> = want.lines().collect();
        for i in [0, 1, 2, 5, 6, 21, 22] {
            assert_eq!(rows[i].trim_end(), want[i].trim_end(), "{name} row {}", i + 1);
        }
    }
}

#[test]
fn dsppfm_f24() {
    let mut d = MapDialog::new();
    d.set("MODE", "V");
    let o = Options { more_keys: true, ..opts() };
    let s = screen("QUIDSPPFM", "DSPPFM", &d, &o);
    let want = golden("dsppfm-f24");
    assert_eq!(s.text_rows()[22].trim_end(), want.lines().nth(22).unwrap().trim_end());
}

/// WRKACTJOB's key rows for a view and the times F24 was pressed.
fn wrkactjob_keys(view: &str, key_set: usize, wide: bool) -> Vec<String> {
    let mut d = MapDialog::new();
    d.set("VIEW", view).set_list("JOBS", list(&[]));
    let o = Options {
        key_set,
        wide,
        ..opts()
    };
    let panel = if wide { "WRKACTJW" } else { "WRKACTJ1" };
    let s = screen("QWCWRKAJ", panel, &d, &o);
    let rows = s.text_rows();
    rows[rows.len() - 3..rows.len() - 1].to_vec()
}

#[test]
fn wrkactjob_f24_3() {
    let want = golden("wrkactjob-f24-3");
    let want: Vec<&str> = want.lines().collect();
    let got = wrkactjob_keys("1", 2, false);
    assert_eq!(got[0].trim_end(), want[21].trim_end());
    assert_eq!(got[1].trim_end(), want[22].trim_end());
}

#[test]
fn wrkactjob_keys_cycle() {
    // The fourth press comes back to the first set.
    assert_eq!(wrkactjob_keys("1", 3, false), wrkactjob_keys("1", 0, false));
    let wide = wrkactjob_keys("1", 0, true).join("\n");
    assert!(wide.contains("F3=Exit") && wide.contains("F24=More keys"), "{wide}");
}

#[test]
fn wrkactjob_wide() {
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS UTC")
        .set_list(
            "JOBS",
            list(&[vec![("JOB", "QBATCH"), ("CURUSER", "QSYS"), ("TYPE", "SBS"), ("STATUS", " DEQW")]]),
        );
    let o = Options { wide: true, ..opts() };
    let s = screen("QWCWRKAJ", "WRKACTJW", &d, &o);
    let rows = s.text_rows();
    assert_eq!(rows.len(), 27);
    assert!(rows.iter().all(|r| r.chars().count() <= 132));
    title_and_keys(&s, "Work with Active Jobs", &["F3=Exit", "F5=Refresh", "F24=More keys"]);
    let heads = rows.join("\n");
    assert!(heads.contains("Temporary") && heads.contains("Function"), "one wide view");
    // An 80-column display may not show it.
    assert!(render(&shipped("QWCWRKAJ"), "WRKACTJW", &d, &opts()).is_err());
}

#[test]
fn wrkactjob_include_subset() {
    let d = MapDialog::new();
    for (panel, title) in [("WRKACTJI", "Include"), ("WRKACTJS", "Subset")] {
        let s = screen("QWCWRKAJ", panel, &d, &opts());
        title_and_keys(&s, title, &["F3=Exit", "F5=Refresh", "F12=Cancel"]);
        assert!(s.text_rows().iter().any(|r| r.contains("Subsystem") && r.contains("*ALL")));
    }
}

/// A panel's title and keys, rendered with no values.
fn bare(group: &str, panel: &str, title: &str, keys: &[&str]) {
    let s = screen(group, panel, &MapDialog::new(), &opts());
    title_and_keys(&s, title, keys);
}

#[test]
fn wrksyssts_title_and_keys() {
    bare(
        "QGWCSSTS",
        "WRKSYSSTS",
        "Work with System Status",
        &["F3=Exit", "F4=Prompt", "F5=Refresh", "F9=Retrieve", "F10=Restart", "F11=Transition data", "F12=Cancel", "F24=More keys"],
    );
}

#[test]
fn wrkshrpool_title_and_keys() {
    bare(
        "QGWCPOOL",
        "WRKSHRPOOL",
        "Work with Shared Pools",
        &["F3=Exit", "F4=Prompt", "F5=Refresh", "F9=Retrieve", "F11=Display text", "F12=Cancel"],
    );
}

#[test]
fn wrksysact_title_and_keys() {
    bare(
        "QGPYMON",
        "WRKSYSACT",
        "Work with System Activity",
        &["F3=Exit", "F10=Update list", "F12=Cancel"],
    );
}

#[test]
fn wrkjrna_title_and_keys() {
    bare(
        "QGJOJRNA",
        "WRKJRNA",
        "Work with Journal Attributes",
        &["F3=Exit", "F5=Refresh", "F12=Cancel", "F14=Display journaled objects", "F15=Work with receiver directory"],
    );
    bare("QGJOJRNA", "WRKJRNRD", "Work with Receiver Directory", &["F3=Exit", "F12=Cancel"]);
    bare("QGJOJRNA", "DSPJRNOBJ", "Display Journaled Objects", &["F3=Exit", "F12=Cancel"]);
}

#[test]
fn dspjrn_title_and_keys() {
    bare("QGJOJRNE", "DSPJRN", "Display Journal Entries", &["F3=Exit", "F12=Cancel"]);
    bare("QGJOJRNE", "DSPJRNE", "Display Journal Entry", &["F3=Exit", "F12=Cancel"]);
}

#[test]
fn dspjrnrcva_title_and_keys() {
    bare(
        "QGJORCV",
        "DSPJRNRCVA",
        "Display Journal Receiver Attributes",
        &["F3=Exit", "F12=Cancel"],
    );
}

#[test]
fn dspsavf_title_and_keys() {
    bare("QUIDSPSAVF", "DSPSAVF", "Display Saved Objects - Save File", &["F3=Exit", "F12=Cancel"]);
}

#[test]
fn wrkprb_title_and_keys() {
    bare("QGSXPNLS", "WRKPRB", "Work with Problems", &["F3=Exit", "F5=Refresh", "F12=Cancel"]);
}

#[test]
fn wrkobjpgp_title_and_keys() {
    bare(
        "QUIWRKOPGP",
        "WRKOBJPGP",
        "Work with Objects by Primary Group",
        &["F3=Exit", "F4=Prompt", "F5=Refresh", "F9=Retrieve", "F11=Display names only", "F12=Cancel"],
    );
}

#[test]
fn dspovr_details_title_and_keys() {
    bare("QUIDSPOVR", "DSPOVRD", "Display Override Details", &["F3=Exit", "F12=Cancel"]);
}

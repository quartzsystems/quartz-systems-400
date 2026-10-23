// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DFU's and RLU's displays against the pub400 goldens: each built with
//! the values the golden shows and compared character by character. A
//! golden whose message line holds IBM's message is compared without
//! that line (ours says it in our words).

mod common;

use common::{compare, compare_rows};
use dfu::field::signed;
use dfu::program::Program;
use dfu::screen::rlu::{Design, ImageLine, banner};
use dfu::screen::work::{Row, View};
use dfu::screen::{define, menu, prompt, rlu, work};
use dfu::{Mode, Session};
use uim::Screen;

/// The account's library as the goldens mask it.
const LIB: &str = "XXXXXXXX1";

fn all_but_message(name: &str, s: &Screen) {
    compare_rows(name, s, &(1..24).collect::<Vec<_>>());
}

fn values(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> String {
    move |k| {
        pairs
            .iter()
            .find(|(a, _)| *a == k)
            .map_or_else(String::new, |(_, v)| (*v).to_string())
    }
}

#[test]
fn dfu_strdfu() {
    compare("dfu-strdfu", &menu::strdfu("", None, true));
}

#[test]
fn dfu_run() {
    compare("dfu-run", &menu::run("", None));
}

fn lib_values(pgmlib: &'static str) -> impl Fn(&str) -> String {
    move |k| {
        match k {
            "PGM" => "QSTESTDFU",
            "PGMLIB" => pgmlib,
            "FILE" => "QDFUZZIN",
            "FILELIB" => LIB,
            "MBR" => "*FIRST",
            _ => "",
        }
        .to_string()
    }
}

#[test]
fn dfu_run_change() {
    compare(
        "dfu-run-change",
        &prompt::panel(&prompt::RUN_CHANGE, &lib_values("*CURLIB"), None),
    );
}

#[test]
fn dfu_run_display() {
    compare(
        "dfu-run-display",
        &prompt::panel(&prompt::RUN_DISPLAY, &lib_values("*CURLIB"), None),
    );
}

#[test]
fn dfu_create() {
    compare(
        "dfu-create",
        &prompt::panel(&prompt::CREATE, &lib_values("*CURLIB"), None),
    );
    compare(
        "dfu-create-back",
        &prompt::panel(&prompt::CREATE, &lib_values(LIB), None),
    );
}

#[test]
fn dfu_change_pgm() {
    compare(
        "dfu-change-pgm",
        &prompt::panel(&prompt::CHANGE, &lib_values(LIB), None),
    );
}

#[test]
fn dfu_delete_pgm() {
    compare(
        "dfu-delete-pgm",
        &prompt::panel(&prompt::DELETE, &lib_values(LIB), None),
    );
}

#[test]
fn dfu_temp() {
    compare(
        "dfu-temp",
        &prompt::panel(&prompt::TEMP, &lib_values(LIB), None),
    );
}

#[test]
fn dfu_define_general() {
    let p = Program::new("QSTESTDFU", LIB, "QDFUZZIN");
    compare("dfu-define-general", &define::general(&p, false, None));
    all_but_message(
        "dfu-define-changed",
        &define::general(&p, false, Some("The definition was changed.")),
    );
}

fn view(mode: Mode, file: &str, keys: Vec<Row>, rows: Vec<Row>) -> View {
    View {
        title: "WORK WITH DATA IN A FILE".into(),
        mode,
        format: format!("{file}R"),
        file: file.into(),
        keys,
        rows,
        page: 0,
        message: None,
        copyright: false,
    }
}

fn recnbr(typed: &str) -> Vec<Row> {
    vec![Row::input("*RECNBR", "*RECNBR", 9, typed)]
}

/// `QDFUZZIN`'s fields: name, width, value.
fn infields(vals: [&str; 6]) -> Vec<Row> {
    let shape = [("IID", 10), ("OP", 2), ("SA", 3), ("A", 70), ("SB", 3), ("B", 70)];
    shape
        .iter()
        .zip(vals)
        .map(|((n, w), v)| Row::input(n, n, *w, v))
        .collect()
}

#[test]
fn dfu_upddta() {
    let mut v = view(Mode::Change, "QDFUZZIN", recnbr(""), Vec::new());
    v.copyright = true;
    compare("dfu-upddta", &work::screen(&v));
    v.copyright = false;
    compare("dfu-change", &work::screen(&v));
    v.message = Some("That function key cannot be used here.".into());
    all_but_message("dfu-upddta-badkey", &work::screen(&v));
    v.message = Some("No record with that key in member QDFUZZIN.".into());
    v.keys = recnbr("99999999");
    all_but_message("dfu-notfound", &work::screen(&v));
}

#[test]
fn dfu_entry() {
    let v = view(Mode::Entry, "QDFUZZIN", Vec::new(), infields([""; 6]));
    compare("dfu-entry", &work::screen(&v));
    let v = view(Mode::Insert, "QDFUZZIN", Vec::new(), infields([""; 6]));
    compare("dfu-insert", &work::screen(&v));
}

#[test]
fn dfu_record() {
    let keys = vec![Row::output("*RECNBR", 9, &signed("9", 9))];
    let v = view(
        Mode::Change,
        "QDFUZZIN",
        keys,
        infields([&signed("9", 10), "-", "", "1.00", "", "3.00"]),
    );
    compare("dfu-record-in", &work::screen(&v));
    let out = |n: &str, rrn: &str, inter: &str, r: &str| {
        let rows = vec![
            Row::input("OID", "OID", 10, &signed(n, 10)),
            Row::input("INTER", "INTER", 80, inter),
            Row::input("R1T", "R1T", 80, r),
            Row::input("RHT", "RHT", 80, r),
            Row::input("RRT", "RRT", 80, r),
        ];
        view(
            Mode::Change,
            "QDFUZZOUT",
            vec![Row::output("*RECNBR", 9, &signed(rrn, 9))],
            rows,
        )
    };
    compare(
        "dfu-record",
        &work::screen(&out("1", "1", "1999.98", "1999.98000")),
    );
    compare(
        "dfu-record-2",
        &work::screen(&out("2", "2", "998.99", "998.99000")),
    );
    let mut v = view(Mode::Change, "QDFUZZOUT", recnbr(""), Vec::new());
    v.message = Some("There are no more records in that direction.".into());
    all_but_message("dfu-record-bof", &work::screen(&v));
}

#[test]
fn dfu_select_format() {
    compare(
        "dfu-select-format",
        &work::formats(&[("QDFUZZINR".into(), String::new())], None),
    );
}

#[test]
fn dfu_end() {
    let s = Session::new(false);
    compare("dfu-end", &work::end(&s, "Y", None));
    compare("dfu-end-viewed", &work::end(&s, "Y", None));
}

fn design(key_set: usize) -> Screen {
    rlu::design(&Design {
        lib: LIB.into(),
        file: "QDDSSRC".into(),
        member: "QDFUZZOUT".into(),
        from: 1,
        cmdline: String::new(),
        lines: vec![
            ImageLine {
                prefix: String::new(),
                text: banner("Start of Page 001"),
                editable: false,
            },
            ImageLine {
                prefix: "QDFUZZOUTR".into(),
                text: banner("AFPDS Record Format"),
                editable: false,
            },
            ImageLine {
                prefix: String::new(),
                text: banner("End of Report"),
                editable: false,
            },
        ],
        key_set,
        message: None,
    })
}

#[test]
fn rlu_first() {
    all_but_message("rlu-first", &design(0));
    all_but_message("rlu-f24", &design(1));
    all_but_message("rlu-f24-2", &design(2));
}

#[test]
fn rlu_exit() {
    const V: &[(&str, &str)] = &[
        ("OPTION", "2"),
        ("MBR", "QDFUZZOUT"),
        ("FILE", "QDDSSRC"),
        ("LIB", LIB),
        ("CRTPRTF", "N"),
        ("CRTDFT", "N"),
        ("PROTO", "N"),
        ("PROTODFT", "N"),
        ("BATCH", "N"),
        ("JOBD", "*USRPRF"),
    ];
    compare("rlu-exit", &rlu::exit(&values(V), None));
}

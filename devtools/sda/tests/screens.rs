// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SDA's displays against the pub400 goldens (`tools/conformance/probes/
//! sda.tn`): each built with the values the golden shows and compared
//! character by character. The member behind them is the probe's
//! `SDAPROBE` with record `SCREEN1`, as SDA saved it there.

mod common;

use std::collections::BTreeMap;

use common::{compare, compare_lines, compare_rows};
use sda::image::{View, render};
use sda::keywords::{self, Level};
use sda::screen::{comments, entry, fields, menu, records};

/// The probe member as SDA wrote it (stamps aside).
const SDAPROBE: &str = "\
     A*%%TS  SD  20261023  165956  QPGMR       REL-V1R0M0  QS400
     A*%%EC
     A                                      DSPSIZ(24 80 *DS3)
     A          R SCREEN1
     A*%%TS  SD  20261023  165956  QPGMR       REL-V1R0M0  QS400
     A                                  3  2'Customer'
     A            FLD001        10   I  3 21
     A                                  5  2'Amount'
     A                                  5  9'due'
     A            FLD002         7  2O  5 21EDTWRD('     .  ')";

fn member() -> sda::Member {
    let lines: Vec<String> = SDAPROBE.lines().map(str::to_string).collect();
    sda::parse(&lines)
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

fn values(m: &BTreeMap<String, String>) -> impl Fn(&str) -> String + '_ {
    move |k| m.get(k).cloned().unwrap_or_default()
}

const HEAD: (&str, &str, &str, &str) = ("QDDSSRC", "USERNAME1", "SDAPROBE", "DSPF");

#[test]
fn sda_strsda() {
    compare("sda-strsda", &menu::strsda("", None, true));
}

#[test]
fn sda_design_screens() {
    let m = map(&[("SRCFILE", "QDDSSRC"), ("SRCLIB", "USERNAME1"), ("SRCMBR", "SDAPROBE")]);
    compare("sda-design-screens", &entry::design_screens(&values(&m), None));
}

#[test]
fn sda_design_menus() {
    let m = map(&[("SRCLIB", "*LIBL")]);
    compare("sda-design-menus", &entry::design_menus(&values(&m), None));
}

#[test]
fn sda_test_dspf() {
    let m = map(&[("FILE", "SDAPROBE"), ("LIB", "*LIBL")]);
    compare("sda-test-dspf", &entry::test_dspf(&values(&m), None));
}

#[test]
fn sda_recover() {
    compare("sda-recover", &entry::recover("SDAPROBE", "QDDSSRC", "USERNAME1", "1"));
}

fn record_rows() -> Vec<records::Row> {
    vec![records::Row {
        order: 10,
        name: "SCREEN1".into(),
        ty: "RECORD".into(),
        date: "MM/DD/YY".into(),
        ..records::Row::default()
    }]
}

fn view(text: bool, keys2: bool) -> records::View {
    records::View {
        file: HEAD.0.into(),
        lib: HEAD.1.into(),
        member: HEAD.2.into(),
        srctype: HEAD.3.into(),
        text,
        keys2,
        top: 0,
    }
}

#[test]
fn sda_records() {
    compare("sda-records", &records::records(&view(false, false), &record_rows(), ("", ""), None));
}

#[test]
fn sda_records_f24() {
    compare(
        "sda-records-f24",
        &records::records(&view(false, true), &record_rows(), ("", ""), None),
    );
}

#[test]
fn sda_records_f11() {
    compare(
        "sda-records-f11",
        &records::records(&view(true, true), &record_rows(), ("", ""), None),
    );
}

#[test]
fn sda_addrcd() {
    let m = map(&[("NAME", "SCREEN2"), ("TYPE", "RECORD")]);
    compare("sda-addrcd", &entry::add_record(HEAD, &values(&m), None));
}

#[test]
fn sda_subset() {
    let m = map(&[("NAME", "*ALL"), ("TYPE", "*ALL")]);
    compare("sda-subset", &entry::subset(HEAD, &values(&m), None));
}

#[test]
fn sda_defaults() {
    let m = map(&[("FLDDEF", "Y")]);
    compare("sda-defaults", &entry::defaults(&values(&m), None));
}

#[test]
fn sda_dbfiles() {
    compare("sda-dbfiles", &entry::db_files(&|_| String::new(), None));
}

#[test]
fn sda_image_exit() {
    compare("sda-image-exit", &entry::image_exit("1", None));
}

#[test]
fn sda_save() {
    let m = map(&[
        ("SAVE", "Y"),
        ("SRCFILE", "QDDSSRC"),
        ("SRCLIB", "USERNAME1"),
        ("SRCMBR", "SDAPROBE"),
        ("CREATE", "Y"),
        ("FILE", "SDAPROBE"),
        ("LIB", "USERNAME1"),
        ("BATCH", "Y"),
    ]);
    let s = entry::save(&values(&m), None);
    compare_rows("sda-save", &s, &(1..=23).collect::<Vec<_>>());
}

#[test]
fn sda_comments_exit() {
    let m = map(&[("SAVE", "N")]);
    compare("sda-comments-exit", &entry::comments_exit(&values(&m)));
}

#[test]
fn sda_comments() {
    let s = comments::editor("Record comments", "SCREEN1", &[], None);
    compare_rows("sda-comments", &s, &(1..=23).collect::<Vec<_>>());
}

#[test]
fn sda_filecmt() {
    let s = comments::editor("File comments", "", &[], None);
    compare_rows("sda-filecmt", &s, &(1..=23).collect::<Vec<_>>());
}

fn on_page(level: Level, ids: &[&str]) -> Vec<keywords::Group> {
    keywords::groups(level)
        .iter()
        .filter(|g| ids.contains(&g.id))
        .copied()
        .collect()
}

#[test]
fn sda_rcdkwd() {
    let s = fields::select_keywords(
        &fields::Head::Record("SCREEN1".into()),
        keywords::RECORD,
        &|_| String::new(),
        None,
    );
    compare("sda-rcdkwd", &s);
}

#[test]
fn sda_filekwd() {
    let s = fields::select_keywords(
        &fields::Head::Member("SDAPROBE".into()),
        keywords::FILE,
        &|_| String::new(),
        None,
    );
    compare("sda-filekwd", &s);
}

#[test]
fn sda_fldkwd() {
    let m = member();
    let e = &m.records[0].elements[0];
    let groups = keywords::field_groups(e);
    assert_eq!(groups, on_page(Level::Field, &["DSPATR", "COLOR", "GENERAL", "TEXT"]));
    let s = fields::select_keywords(&fields::Head::of(e), &groups, &|_| String::new(), None);
    compare("sda-fldkwd", &s);
}

#[test]
fn sda_fields() {
    let m = member();
    let rows: Vec<fields::Row> = m.records[0]
        .elements
        .iter()
        .enumerate()
        .map(|(i, e)| fields::Row::of(u32::try_from(i + 1).unwrap() * 10, e, false))
        .collect();
    compare("sda-fields", &fields::work_with_fields("SCREEN1", &rows, 0, "6", None));
}

#[test]
fn sda_image() {
    let m = member();
    let rows = render(&m.records[0], &[], &View::default());
    compare_lines("sda-image", &rows, 1, 23);
}

#[test]
fn sda_image_ruler() {
    let m = member();
    let v = View {
        ruler: Some((5, 20)),
        hidden: false,
    };
    let rows = render(&m.records[0], &[], &v);
    compare_lines("sda-image-ruler", &rows, 1, 24);
}

#[test]
fn sda_image_codes() {
    // H before the constant and R before the field change attributes
    // only: the characters are the image's.
    let mut m = member();
    let r = &mut m.records[0];
    let shown = render(r, &[], &View::default());
    let mut typed = shown.clone();
    typed[2].replace_range(0..1, "H");
    typed[2].replace_range(19..20, "R");
    let edits = sda::image::place::parse(r, &shown, &typed).unwrap();
    sda::image::place::apply(r, &edits, &[]);
    assert_eq!(r.elements[0].attrs, ["HI"]);
    assert_eq!(r.elements[1].attrs, ["RI"]);
    let rows = render(r, &[], &View::default());
    compare_lines("sda-image-codes", &rows, 1, 24);
}

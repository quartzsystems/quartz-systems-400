// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The services' displays, Change Session Attributes as SELECT output
//! and the formats reshape it, and Display Data under the session's
//! formats, against pub400's captures in
//! `tools/conformance/golden/sql/strsql/` (V7R5M0,
//! `tools/conformance/probes/sql/strsql-*.tn`).

mod common;

use common::{compare, compare_rows, words};
use isql::Attrs;
use isql::format::{DateFmt, Format, TimeFmt};
use isql::result::{Column, Data};
use isql::screen::services::{self, SourceFile};
use isql::screen::{data, menus};

fn every_row_but(skip: &[usize]) -> Vec<usize> {
    (1..=24).filter(|r| !skip.contains(r)).collect()
}

#[test]
fn printer_file() {
    compare(
        "sql/strsql-print",
        &services::printer_file("QSYSPRT", None, &words),
    );
}

#[test]
fn source_file() {
    compare(
        "sql/strsql-savesrc",
        &services::source_file(&SourceFile::new("USERNAME1"), None, &words),
    );
}

#[test]
fn confirm_remove() {
    // Rows 3 to 5 are what removing does, in Quartz's words.
    compare_rows(
        "sql/strsql-removed",
        &services::confirm_remove(&words),
        &every_row_but(&[3, 4, 5]),
    );
}

/// The attributes as pub400's session had them.
fn session(changes: &[(&str, &str)]) -> Attrs {
    let mut a = Attrs::default();
    for (k, v) in [
        ("DATFMT", "*YMD"),
        ("DATSEP", "-"),
        ("TIMSEP", ":"),
        ("DECPNT", "*PERIOD"),
        ("SRTSEQ", "*HEX"),
        ("LANGID", "ENU"),
        ("OUTLIB", "USERNAME1"),
    ]
    .iter()
    .chain(changes)
    {
        a.set(k, v).unwrap();
    }
    a
}

#[test]
fn attributes_reshape() {
    let printer = session(&[("OUTPUT", "2")]);
    compare(
        "sql/strsql-out-printer",
        &menus::attributes(0, &printer, None, &words),
    );
    let file = session(&[("OUTPUT", "3")]);
    compare(
        "sql/strsql-out-file",
        &menus::attributes(0, &file, None, &words),
    );
    compare(
        "sql/strsql-out-file-2",
        &menus::attributes(1, &file, None, &words),
    );
    assert_eq!(menus::attr_pages(&file, &words), 3);
    let iso = session(&[("DATFMT", "*ISO"), ("TIMFMT", "*ISO")]);
    compare(
        "sql/strsql-attrs-iso",
        &menus::attributes(0, &iso, None, &words),
    );
    let resumed = session(&[("DATFMT", "*ISO")]);
    compare(
        "sql/strsql-resume-attrs",
        &menus::attributes(0, &resumed, None, &words),
    );
    compare(
        "sql/strsql-fresh-attrs",
        &menus::attributes(0, &session(&[]), None, &words),
    );
    compare(
        "sql/strsql-attrs-now-2",
        &menus::attributes(1, &session(&[]), None, &words),
    );
}

fn col(name: &str, ty: &str) -> Column {
    Column {
        name: name.into(),
        data_type: ty.into(),
    }
}

/// `select 'ABC' as c, dec(-1234.5,7,2) as d, date(..) as dt,
/// time(..) as tm, cast(null as int) as n`.
fn first(fmt: Format) -> Data {
    Data {
        columns: vec![
            col("C", "CHAR(3)"),
            col("D", "DECIMAL(7, 2)"),
            col("DT", "DATE"),
            col("TM", "TIME"),
            col("N", "INTEGER"),
        ],
        rows: vec![vec![
            Some("ABC".into()),
            Some("-1234.50".into()),
            Some("2026-01-15".into()),
            Some("13.45.30".into()),
            None,
        ]],
        fmt,
    }
}

/// `select smallint(-12) s, bigint(..) b, double(1.5e3) f,
/// timestamp(..) ts, varchar('xyz',20) v, dec(5,5,0) z, dec(0.5,3,2) h`.
fn second(fmt: Format) -> Data {
    Data {
        columns: vec![
            col("S", "SMALLINT"),
            col("B", "BIGINT"),
            col("F", "DOUBLE"),
            col("TS", "TIMESTAMP"),
            col("V", "VARCHAR(20)"),
            col("Z", "DECIMAL(5, 0)"),
            col("H", "DECIMAL(3, 2)"),
        ],
        rows: vec![vec![
            Some("-12".into()),
            Some("123456789012".into()),
            Some("1500".into()),
            Some("2026-01-15-13.45.30.123456".into()),
            Some("xyz".into()),
            Some("5".into()),
            Some("0.50".into()),
        ]],
        fmt,
    }
}

const DATA_ROWS: &[usize] = &[1, 2, 3, 4, 5, 6, 7, 22, 23];

fn check(name: &str, d: &Data) {
    compare_rows(name, &data::display(d, &data::pager(), None), DATA_ROWS);
}

fn check_right(name: &str, d: &Data) {
    let mut p = data::pager();
    assert!(p.right(d.width()));
    compare_rows(name, &data::display(d, &p, None), DATA_ROWS);
}

#[test]
fn display_data_formats() {
    let ymd = Format {
        date: DateFmt::Ymd,
        datsep: '-',
        time: TimeFmt::Hms,
        timsep: ':',
        point: '.',
    };
    check("sql/strsql-fmt-ymd", &first(ymd));
    check("sql/strsql-fmt2-ymd", &second(ymd));
    check_right("sql/strsql-fmt2-ymd-f20", &second(ymd));
    let iso = Format {
        date: DateFmt::Iso,
        time: TimeFmt::Iso,
        ..ymd
    };
    check("sql/strsql-fmt-iso", &first(iso));
    check("sql/strsql-fmt2-iso", &second(iso));
    let usa = Format {
        date: DateFmt::Usa,
        time: TimeFmt::Usa,
        point: ',',
        ..ymd
    };
    check("sql/strsql-fmt-usa", &first(usa));
    check("sql/strsql-fmt2-usa", &second(usa));
    check_right("sql/strsql-fmt2-usa-f20", &second(usa));
    let jul = Format {
        date: DateFmt::Jul,
        datsep: '/',
        timsep: '.',
        ..ymd
    };
    check("sql/strsql-fmt-jul", &first(jul));
    check("sql/strsql-fmt2-jul", &second(jul));
}

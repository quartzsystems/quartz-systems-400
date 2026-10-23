// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! STRSQL's displays against the pub400 goldens: each built with the
//! values the golden shows and compared character by character. Rows
//! holding a message are left out: message text is ours, not IBM's.

mod common;

use common::{compare, compare_rows};
use isql::assemble::Values;
use isql::attrs::Attrs;
use isql::history::History;
use isql::result::{Column, Data};
use isql::screen::lists::{self, Which};
use isql::screen::{data, enter, menus, prompt};

/// Every row but `skip`.
fn rows_but(skip: &[usize]) -> Vec<usize> {
    (1..=24).filter(|r| !skip.contains(r)).collect()
}

/// A history with the connection message a session starts with.
fn started() -> History {
    let mut h = History::new();
    h.message("", "connected");
    h.fit();
    h
}

#[test]
fn strsql_main() {
    let s = enter::enter(&started(), 0, false, None, None);
    compare_rows("strsql-main", &s, &rows_but(&[4]));
    compare_rows("strsql-restored", &s, &rows_but(&[4]));
}

#[test]
fn strsql_f24() {
    let s = enter::enter(&started(), 1, false, None, None);
    compare_rows("strsql-f24", &s, &rows_but(&[4]));
    let s = enter::enter(&started(), 2, false, None, None);
    compare_rows("strsql-f24-2", &s, &rows_but(&[4]));
}

/// The session of the third probe after its wide query: the statement
/// typed on the last of 16 input lines.
fn after_query() -> History {
    let mut h = started();
    let mut lines = vec![String::new(); 15];
    lines.push("select * from qsys2.systables where table_schema='QSYS2' limit 40".into());
    h.statement(lines);
    h.message("", "ran");
    h.to_end();
    h
}

#[test]
fn strsql_history() {
    let h = after_query();
    let s = enter::enter(&h, 0, false, None, None);
    compare_rows("strsql-history", &s, &rows_but(&[16]));
}

#[test]
fn strsql_error() {
    let mut h = after_query();
    let typed = vec![
        String::new(),
        String::new(),
        String::new(),
        "selec 1 from sysibm.sysdummy1".to_string(),
    ];
    h.statement(typed.clone());
    h.message("SQL0104", "bad token");
    h.input = typed;
    h.to_last_statement();
    let s = enter::enter(&h, 0, false, None, None);
    compare_rows("strsql-error", &s, &rows_but(&[8]));
    compare_rows("strsql-f9", &s, &rows_but(&[8]));
}

#[test]
fn strsql_prompt_list() {
    compare("strsql-prompt-list", &menus::statement_list(0, "", None));
    compare("strsql-prompt-list-2", &menus::statement_list(1, "", None));
    compare("strsql-prompt-list-3", &menus::statement_list(2, "", None));
    assert_eq!(menus::statement_pages(), 3);
}

#[test]
fn strsql_services() {
    compare("strsql-services", &menus::services("", None));
}

fn golden_attrs() -> Attrs {
    let mut a = Attrs::default();
    for (k, v) in [
        ("PROCESS", "*RUN"),
        ("OUTPUT", "1"),
        ("COMMIT", "*NONE"),
        ("DATFMT", "*YMD"),
        ("DATSEP", "-"),
        ("TIMFMT", "*HMS"),
        ("TIMSEP", ":"),
        ("REFRESH", "*ALWAYS"),
        ("ALWCPYDTA", "*YES"),
        ("NAMING", "*SYS"),
        ("LIBOPT", "*LIBL"),
        ("LISTTYPE", "*ALL"),
        ("DECPNT", "*COMMA"),
        ("SRTSEQ", "*HEX"),
        ("LANGID", "ENU"),
        ("SQLCURRULE", "*DB2"),
        ("DSPPWD", "*NO"),
    ] {
        a.set(k, v).unwrap();
    }
    a
}

#[test]
fn strsql_attrs() {
    let a = golden_attrs();
    compare("strsql-attrs", &menus::attributes(0, &a, None));
    compare("strsql-attrs-2", &menus::attributes(1, &a, None));
    compare_rows(
        "strsql-attrs-3",
        &menus::attributes(1, &a, None),
        &rows_but(&[24]),
    );
}

#[test]
fn strsql_exit() {
    compare("strsql-exit", &menus::exit("1", None));
}

fn prompt_of(name: &str, p: &prompt::Prompt) {
    compare(name, &prompt::render(p, &Values::new(), false, None));
}

#[test]
fn strsql_prompt_select() {
    prompt_of("strsql-prompt-select", &prompt::SELECT);
}

#[test]
fn strsql_prompt_insert() {
    prompt_of("strsql-prompt-insert", &prompt::INSERT);
}

#[test]
fn strsql_prompt_update() {
    prompt_of("strsql-prompt-update", &prompt::UPDATE);
}

#[test]
fn strsql_prompt_delete() {
    prompt_of("strsql-prompt-delete", &prompt::DELETE);
}

#[test]
fn strsql_prompt_crttbl() {
    prompt_of("strsql-prompt-crttbl", &prompt::CREATE_TABLE);
}

#[test]
fn strsql_prompt_crtview() {
    prompt_of("strsql-prompt-crtview", &prompt::CREATE_VIEW);
}

#[test]
fn strsql_prompt_crtidx() {
    prompt_of("strsql-prompt-crtidx", &prompt::CREATE_INDEX);
}

#[test]
fn strsql_data_dummy() {
    let d = Data {
        columns: vec![Column {
            name: "IBMREQD".into(),
            data_type: "CHAR(1)".into(),
        }],
        rows: vec![vec![Some("Y".into())]],
    };
    compare(
        "strsql-data-dummy",
        &data::display(&d, &data::pager(), None),
    );
}

/// A query as wide as the golden's, one `VARCHAR(128)` name column.
fn wide() -> Data {
    let mut columns = vec![Column {
        name: "TABLE_NAME".into(),
        data_type: "VARCHAR(128)".into(),
    }];
    columns.push(Column {
        name: "REST".into(),
        data_type: format!("CHAR({})", 5321 - 128 - 2),
    });
    let rows = (0..40)
        .map(|i| vec![Some(format!("T{i}")), Some(String::new())])
        .collect();
    Data { columns, rows }
}

#[test]
fn strsql_data_wide() {
    let d = wide();
    let mut p = data::pager();
    let s = data::display(&d, &p, None);
    compare_rows("strsql-data-wide", &s, &[1, 2, 3, 4, 22, 23]);
    p.right(d.width());
    let s = data::display(&d, &p, None);
    compare_rows("strsql-data-f20", &s, &[1, 2, 3, 4, 22, 23]);
}

fn libraries() -> Vec<lists::Entry> {
    ["QSYS", "QSYS2", "QUSRSYS", "QHLPSYS", "QGPL", "QTEMP"]
        .iter()
        .map(|n| lists::Entry {
            name: (*n).to_string(),
            kind: "SYS".into(),
            ..lists::Entry::default()
        })
        .collect()
}

#[test]
fn strsql_f16() {
    let s = lists::list(Which::Libraries, &libraries(), 0, false, None);
    compare_rows("strsql-f16", &s, &[1, 2, 3, 4, 5, 22, 23, 24]);
}

#[test]
fn strsql_f17_files() {
    let files: Vec<lists::Entry> = (0..20)
        .map(|i| lists::Entry {
            name: format!("VIEW{i}"),
            owner: "QSYS2".into(),
            kind: "VW".into(),
            ..lists::Entry::default()
        })
        .collect();
    let s = lists::list(Which::Files, &files, 0, false, None);
    compare_rows("strsql-f17-files", &s, &[1, 2, 3, 4, 5, 21, 22, 23]);
    compare_rows("strsql-f18-files", &s, &[1, 2, 3, 4, 5, 21, 22, 23]);
}

#[test]
fn strsql_f18() {
    let s = lists::list(Which::Libraries, &libraries(), 0, false, None);
    compare_rows("strsql-f18", &s, &[1, 2, 3, 4, 5, 22, 23]);
    compare_rows("strsql-f17", &s, &[1, 2, 3, 4, 5, 22, 23]);
}

#[test]
fn strsql_f18_fields() {
    let fields: Vec<lists::Entry> = (0..11)
        .map(|i| lists::Entry {
            name: format!("COLUMN_NUMBER_{i}"),
            owner: "T".into(),
            kind: "TIMESTAMP".into(),
            length: "26".into(),
            scale: "6".into(),
            ..lists::Entry::default()
        })
        .collect();
    let s = lists::list(Which::Fields, &fields, 0, false, None);
    compare_rows(
        "strsql-f18-fields",
        &s,
        &[1, 2, 3, 4, 5, 6, 18, 19, 20, 21, 22, 23, 24],
    );
    let rows = s.text_rows();
    assert_eq!(rows[6].find("COLUMN_NUMBER_0"), Some(6));
    assert_eq!(rows[16].find("COLUMN_NUMBER_1 >"), Some(6));
    assert_eq!(rows[6].rfind("26"), Some(69));
    assert_eq!(rows[6].rfind('6'), Some(76));
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statement prompts of ALTER TABLE to SET TRANSACTION and the
//! column displays of INSERT, UPDATE and CREATE INDEX against the
//! pub400 captures (`golden/sql/strsql/`, V7R5M0, `*SYS` naming),
//! character by character.

mod common;

use common::{compare, words};
use isql::assemble::Values;
use isql::screen::stmt::{self, Panel};
use isql::screen::{columns, prompt};

fn panel(name: &str, p: &Panel) {
    compare(&format!("sql/strsql-prompt-{name}"), &stmt::render(p, &Values::new(), 0, false, None, &words));
}

#[test]
fn every_statement_prompt() {
    for (name, p) in [
        ("alter-table", &stmt::ALTER_TABLE),
        ("call", &stmt::CALL),
        ("comment-on", &stmt::COMMENT_ON),
        ("commit", &stmt::COMMIT),
        ("connect", &stmt::CONNECT),
        ("create-alias", &stmt::CREATE_ALIAS),
        ("create-collection", &stmt::CREATE_COLLECTION),
        ("create-procedure", &stmt::CREATE_PROCEDURE),
        ("disconnect", &stmt::DISCONNECT),
        ("drop-alias", &stmt::DROP_ALIAS),
        ("drop-collection", &stmt::DROP_COLLECTION),
        ("drop-index", &stmt::DROP_INDEX),
        ("drop-package", &stmt::DROP_PACKAGE),
        ("drop-table", &stmt::DROP_TABLE),
        ("drop-view", &stmt::DROP_VIEW),
        ("grant-package", &stmt::GRANT_PACKAGE),
        ("grant-table", &stmt::GRANT_TABLE),
        ("label-on", &stmt::LABEL_ON),
        ("lock-table", &stmt::LOCK_TABLE),
        ("release", &stmt::RELEASE),
        ("rename", &stmt::RENAME),
        ("revoke-package", &stmt::REVOKE_PACKAGE),
        ("revoke-table", &stmt::REVOKE_TABLE),
        ("rollback", &stmt::ROLLBACK),
        ("set-connection", &stmt::SET_CONNECTION),
        ("set-transaction", &stmt::SET_TRANSACTION),
    ] {
        panel(name, p);
    }
}

/// QSYS2/QSQPTABL's one column, as the captures list it.
fn cols() -> Vec<String> {
    vec!["INTEGER_COLUMN".into()]
}

#[test]
fn insert_values() {
    compare("sql/strsql-prompt-insert-2", &columns::insert(&cols(), &Values::new(), 0, false, None, &words));
}

#[test]
fn update_values() {
    let v: Values = [("TABLE", "QSQPTABL"), ("LIB", "QSYS2")].iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    let first = prompt::render(&prompt::UPDATE, &v, false, None);
    compare("sql/strsql-prompt-update-2", &columns::update(first, &cols(), &Values::new(), 0, false, None, &words));
}

#[test]
fn index_key() {
    compare("sql/strsql-prompt-crtidx-2", &columns::key(&cols(), &Values::new(), 0, false, None, &words));
}

#[test]
fn values_and_key_read_back() {
    let c: Vec<String> = ["A", "B", "C"].iter().map(|s| (*s).to_string()).collect();
    let v: Values = [("VAL1", "1"), ("VAL3", "'x'"), ("SEQ2", "1"), ("SEQ3", "2"), ("ORD3", "d")]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    assert_eq!(columns::insert_values(&c, &v), (vec!["A".to_string(), "C".to_string()], vec!["1".to_string(), "'x'".to_string()]));
    assert_eq!(columns::key_columns(&c, &v), ["B", "C DESC"]);
    // CALL's second page of parameters.
    let s = stmt::render(&stmt::CALL, &Values::new(), 1, false, None, &words);
    let rows = s.text_rows();
    assert!(rows[8].trim_start().starts_with("13"), "{:?}", rows[8]);
    assert!(stmt::names(&stmt::CALL, 3).iter().any(|n| n == "PARM37"));
    assert!(!stmt::names(&stmt::CALL, 3).iter().any(|n| n == "PARM1"));
}

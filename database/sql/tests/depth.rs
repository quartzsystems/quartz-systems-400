// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Nesting deeper than the parser allows is SQL0101, not a stack overflow.

use sql::{Naming, SqlError, parse};

#[test]
fn deep_nesting_is_sql0101() {
    let deep = format!("SELECT {}1{} FROM T", "(".repeat(5000), ")".repeat(5000));
    let e = parse(&deep, Naming::Sql).unwrap_err();
    assert_eq!(e, SqlError::TooComplex);
    assert_eq!(e.msgid(), "SQL0101");
    assert_eq!(e.sqlstate(), "54001");
    let nots = format!("SELECT 1 FROM T WHERE {}A = 1", "NOT ".repeat(5000));
    assert_eq!(parse(&nots, Naming::Sql).unwrap_err(), SqlError::TooComplex);
    let negs = format!("SELECT {}1 FROM T", "- ".repeat(5000));
    assert_eq!(parse(&negs, Naming::Sql).unwrap_err(), SqlError::TooComplex);
    let subs = format!("SELECT 1 FROM T WHERE EXISTS {}SELECT 1 FROM T{}", "(SELECT 1 FROM T WHERE EXISTS (".repeat(200), "))".repeat(200));
    assert!(parse(&subs, Naming::Sql).is_err());
    // Ordinary nesting is fine.
    let ok = format!("SELECT {}1{} FROM T", "(".repeat(20), ")".repeat(20));
    assert!(parse(&ok, Naming::Sql).is_ok());
}

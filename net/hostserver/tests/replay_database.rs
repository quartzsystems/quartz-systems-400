// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database server against the `jdbc` trace, over the Phase 3 SQL
//! engine in memory with an empty `QSTEST` (the trace makes its table).

mod common;

use common::mem::session;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::database::{Database, DbConfig};
use hostserver::{Handler, NoExits};

fn masks() -> Masks {
    Masks {
        // Whether a reply is compressed depends on its size.
        template: vec![(0x2800, 4..8)],
        param: vec![
            // SQLCA: message tokens and the product.
            (0x3807, 16..96),
            // SQLERRD(2) and the statement kind in SQLERRD(6).
            (0x3807, 100..104),
            (0x3807, 116..120),
            // Server attributes: the job number.
            (0x3804, 0..2),
            (0x3804, 110..116),
            // The consistency token of result data; cursor attributes.
            (0x380E, 0..4),
            (0x3814, 0..10),
        ],
        // Message texts are ours.
        skip: vec![0x3802, 0x3803],
    }
}

#[test]
fn jdbc() {
    for s in ["jdbc", "pwdlevel", "svrmap"] {
        let t = trace("database", s);
        let auth = TraceAuth::of(&t);
        let cfg = DbConfig {
            rdb: "S062B175".into(),
            job: ("QZDASOINIT".into(), "QUSER".into(), "007839".into()),
        };
        let diffs = replay(&t, &masks(), &mut || Box::new(Database::new(auth.clone(), session(), NoExits, cfg.clone())) as Box<dyn Handler>);
        accept(&format!("database-{s}"), &diffs);
    }
}

#[test]
#[ignore = "prints how the engine takes the trace's statements"]
fn probe_statements() {
    use sql::embed::{Desc, Embedded, Op, Request};
    use sql::value::Value;
    let mut s = session();
    for q in [
        "SELECT TABLE_NAME, TABLE_TYPE, COLUMN_COUNT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME FETCH FIRST 400 ROWS ONLY",
        "SELECT COUNT(*) FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME = 'QSTEST'",
        "SELECT CAST(NULL AS INTEGER), CAST(1.5 AS DOUBLE), CAST('x' AS CHAR(3)), DATE('2026-01-02'), BIGINT(9) FROM SYSIBM.SYSDUMMY1",
        "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_TEXT FROM QSYS2.SYSTABLES",
    ] {
        let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Prepare, name: "S1".into(), into: true, sql_naming: true, ..Desc::default() }, inputs: vec![Value::Str(q.into())] });
        println!("{} {} {:?} rows={}", r.sqlca.sqlcode, r.sqlca.sqlstate, r.sqlca.sqlerrmc, r.rows.len());
    }
    let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Open, name: "C1".into(), sql_naming: true, text: "SELECT TABLE_SCHEMA, TABLE_NAME FROM QSYS2.SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("open {} {}", r.sqlca.sqlcode, r.sqlca.sqlerrmc);
    let q = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Query, sql_naming: true, text: "SELECT COUNT(*) FROM QSYS2.SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("query {} {} {:?}", q.sqlca.sqlcode, q.sqlca.sqlerrmc, q.rows);
    let q = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Query, sql_naming: false, text: "SELECT COUNT(*) FROM QSYS2/SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("query sys {} {} {:?}", q.sqlca.sqlcode, q.sqlca.sqlerrmc, q.rows);
    let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Fetch, name: "C1".into(), rows: true, sql_naming: true, ..Desc::default() }, inputs: vec![Value::int(1000)] });
    println!("fetch {} {}", r.sqlca.sqlcode, r.rows.len());
    for row in r.rows.iter().take(5) { println!("{row:?}"); }
}

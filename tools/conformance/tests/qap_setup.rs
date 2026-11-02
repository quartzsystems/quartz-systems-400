// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QAP live checklist's setup scripts (`tools/conformance/qap/`) run
//! as `RUNSQLSTM` runs them, over a store in memory: `hlinv.sql` with no
//! error and the rows `qapd::testing` has; `hlinv-types.sql` waits for
//! the engine's new types (ignored until then).

use std::path::Path;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use sql::{Naming, Outcome, RunOpts, SqlContext, prepare, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{JobHandle, ObjectId};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn open() -> (Store, SqlContext) {
    let d = Arc::new(MemBlockDev::with_seed(1 << 16, 3));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
    let mut t = store.begin().unwrap();
    let qsys: ObjectId = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    t.commit().unwrap();
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: None,
            libl: vec![qsys],
        },
    );
    ctx.now = Some(NOW);
    (store, ctx)
}

fn script(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("qap").join(name)).expect("the script")
}

fn run(store: &mut Store, ctx: &mut SqlContext, name: &str) -> sql::RunReport {
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    runsqlstm(store, ctx, &script(name), &opts).unwrap()
}

/// The first value of a one-row query, as text.
fn one(store: &mut Store, ctx: &mut SqlContext, text: &str) -> String {
    let p = prepare(text, Naming::Sql).unwrap_or_else(|e| panic!("{text}: {e}"));
    let mut t = store.begin().unwrap();
    let (ca, o) = p.run(&mut t, ctx, &[]);
    t.abort();
    assert!(!ca.is_error(), "{text}: {}", ca.line());
    match o {
        Some(Outcome::Rows(rs)) => rs.rows[0][0].to_string().trim().to_string(),
        o => panic!("{text}: {o:?}"),
    }
}

#[test]
fn hlinv_sets_up_the_test_systems_database() {
    let (mut store, mut ctx) = open();
    let r = run(&mut store, &mut ctx, "hlinv.sql");
    assert_eq!((r.errors, r.severity), (0, 0), "{}", r.listing);
    let mut q = |s: &str| one(&mut store, &mut ctx, s);
    assert_eq!(q("SELECT COUNT(*) FROM HLINV.ASSETS"), "12");
    assert_eq!(q("SELECT COUNT(*) FROM HLINV.ASSETSACT"), "11");
    assert_eq!(q("SELECT MODEL FROM HLINV.ASSETS WHERE ASSET_TAG = 'QS000302'"), "UR20-FBC-EIP");
    assert_eq!(q("SELECT COUNT(*) FROM HLTEST.ALLTYPES"), "7");
    assert_eq!(q("SELECT C_VARCHAR FROM HLTEST.ALLTYPES WHERE ID = 2"), "na\u{ef}ve caf\u{e9}");
    assert_eq!(q("SELECT COUNT(*) FROM HLTEST.SLOW"), "300");
    assert_eq!(q("SELECT SUM(LENGTH(DATA)) FROM HLTEST.BIG"), "1100000");
    assert_eq!(q("SELECT COUNT(*) FROM HLPAY.SALARIES"), "2");
    assert_eq!(q("SELECT COUNT(*) FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'HLMANY'"), "150");
    assert_eq!(q("SELECT COUNT(*) FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME LIKE 'ARCH%'"), "140");
}

/// Enable once the engine has `DECFLOAT`, the binary and LOB types,
/// `BOOLEAN`, `TIMESTAMP(12)` and runs `CREATE ALIAS`.
#[test]
#[ignore = "the engine's new types are not merged yet"]
fn hlinv_types_runs_after_hlinv() {
    let (mut store, mut ctx) = open();
    let r = run(&mut store, &mut ctx, "hlinv.sql");
    assert_eq!(r.errors, 0, "{}", r.listing);
    let r = run(&mut store, &mut ctx, "hlinv-types.sql");
    assert_eq!((r.errors, r.severity), (0, 0), "{}", r.listing);
    assert_eq!(one(&mut store, &mut ctx, "SELECT COUNT(*) FROM HLTEST.NEWTYPES"), "7");
    assert_eq!(one(&mut store, &mut ctx, "SELECT COUNT(*) FROM HLINV.HW"), "12");
}

#[test]
fn hlinv_types_waits_for_the_engine() {
    // Today the engine refuses the file before running any of it; when
    // this fails, enable the test above.
    let (mut store, mut ctx) = open();
    let r = run(&mut store, &mut ctx, "hlinv-types.sql");
    assert_eq!((r.statements, r.severity), (0, 40), "{}", r.listing);
}

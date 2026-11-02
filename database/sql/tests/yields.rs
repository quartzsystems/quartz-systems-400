// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A long read gives the store back between batches: another job's
//! changes made meanwhile are seen as committed reads see them, the files
//! read cannot be dropped under it, and statements that change data
//! never yield.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
const ROWS: i64 = 1000;

struct Fixture {
    store: Rc<RefCell<Store>>,
    qsys: ObjectId,
    names: NameContext,
}

fn fixture() -> Fixture {
    let d = Arc::new(MemBlockDev::with_seed(32768, 33));
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
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QSTEST").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: "Test library".into(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    t.commit().unwrap();
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![lib],
    };
    let f = Fixture {
        store: Rc::new(RefCell::new(store)),
        qsys,
        names,
    };
    let mut text = String::from(
        "CREATE TABLE QSTEST.T (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);
         CREATE TABLE QSTEST.K (ID INT NOT NULL PRIMARY KEY, NAME VARCHAR(20) NOT NULL);",
    );
    for t in ["T", "K"] {
        for chunk in 0..ROWS / 100 {
            let _ = write!(text, "INSERT INTO QSTEST.{t} VALUES ");
            for i in 0..100 {
                let id = chunk * 100 + i + 1;
                let _ = write!(text, "{}({id}, 'Row {id}')", if i > 0 { ", " } else { "" });
            }
            text.push(';');
        }
    }
    run_script(&f, 2, &text).unwrap();
    f
}

/// A script as another job (`job`) would run it; the listing on errors.
fn run_script(f: &Fixture, job: u64, text: &str) -> Result<(), String> {
    let mut ctx = SqlContext::new(f.qsys, JobHandle(job), f.names.clone());
    ctx.now = Some(NOW);
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let mut st = f.store.borrow_mut();
    let r = runsqlstm(&mut st, &mut ctx, text, &opts).map_err(|e| e.to_string())?;
    if r.errors == 0 { Ok(()) } else { Err(r.listing) }
}

fn session(f: &Fixture) -> Session {
    let mut s = Session::new(Box::new(Rc::clone(&f.store)), f.qsys, JobHandle(1), f.names.clone());
    s.ctx.now = Some(NOW);
    s.ctx.yield_every = Duration::ZERO;
    s
}

fn run(s: &mut Session, op: Op, name: &str, text: &str) -> Reply {
    s.run(&Request {
        scope: "1:Y".into(),
        desc: Desc {
            op,
            name: name.into(),
            text: text.into(),
            sql_naming: true,
            ..Desc::default()
        },
        inputs: Vec::new(),
    })
}

/// Every row of an open cursor.
fn fetch_all(s: &mut Session, name: &str) -> Vec<i64> {
    let mut ids = Vec::new();
    loop {
        let r = run(s, Op::Fetch, name, "");
        if r.sqlca.sqlcode == 100 {
            return ids;
        }
        assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
        ids.push(r.rows[0][0].as_i64().unwrap());
    }
}

/// What the other job did at each yield.
#[derive(Default)]
struct Log {
    yields: Cell<u32>,
    inserted: RefCell<Vec<i64>>,
    drop_refused: Cell<bool>,
}

fn yielder(f: &Fixture, table: &'static str, log: &Rc<Log>) -> sql::exec::Yielder {
    let store = Rc::clone(&f.store);
    let fx = Fixture {
        store,
        qsys: f.qsys,
        names: f.names.clone(),
    };
    let log = Rc::clone(log);
    Rc::new(move || {
        let n = log.yields.get() + 1;
        log.yields.set(n);
        // Another job adds a row past the end (twice: a reader that
        // keeps meeting new rows ahead never ends), changes one already
        // read and tries to drop the table being read.
        if n <= 2 {
            let id = 100_000 + i64::from(n);
            run_script(&fx, 2, &format!("INSERT INTO QSTEST.{table} VALUES ({id}, 'New')")).unwrap();
            log.inserted.borrow_mut().push(id);
        }
        run_script(&fx, 2, &format!("UPDATE QSTEST.{table} SET NAME = 'Changed' WHERE ID = 1")).unwrap();
        if run_script(&fx, 2, &format!("DROP TABLE QSTEST.{table}")).is_err() {
            log.drop_refused.set(true);
        }
        false
    })
}

#[test]
fn a_long_query_yields_and_sees_committed_changes() {
    for table in ["T", "K"] {
        let f = fixture();
        let mut s = session(&f);
        let log = Rc::new(Log::default());
        s.ctx.yielder = Some(yielder(&f, table, &log));
        let r = run(&mut s, Op::Open, "C1", &format!("SELECT ID FROM QSTEST.{table} WHERE ID > 0"));
        assert_eq!(r.sqlca.sqlcode, 0, "{table}: {:?}", r.sqlca);
        let yields = log.yields.get();
        assert!(yields >= 3, "{table}: {yields} yields over {ROWS} rows");
        assert!(log.drop_refused.get(), "{table}: dropped under the read");
        let mut ids = fetch_all(&mut s, "C1");
        ids.sort_unstable();
        let mut want: Vec<i64> = (1..=ROWS).collect();
        // Rows added past the scan's position are read, as a read of
        // committed rows reads them.
        want.extend(log.inserted.borrow().iter());
        assert_eq!(ids, want, "{table}: every row once");
        run(&mut s, Op::Close, "C1", "");
        // The locks went with the statement.
        s.ctx.yielder = None;
        run_script(&f, 2, &format!("DROP TABLE QSTEST.{table}")).unwrap();
    }
}

#[test]
fn changes_and_nested_scans_never_yield() {
    let f = fixture();
    let mut s = session(&f);
    let log = Rc::new(Log::default());
    s.ctx.yielder = Some(yielder(&f, "T", &log));
    let r = run(&mut s, Op::Exec, "", "UPDATE QSTEST.T SET NAME = 'All' WHERE ID > 0");
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(r.sqlca.sqlerrd[2], i32::try_from(ROWS).unwrap());
    assert_eq!(log.yields.get(), 0, "a change does not yield");
    // A correlated subquery's scan runs inside the outer one: only the
    // outer yields, and the answer is the one a single view gives.
    let r = run(
        &mut s,
        Op::Query,
        "",
        "SELECT COUNT(*) FROM QSTEST.T A WHERE ID <= 3 AND EXISTS (SELECT 1 FROM QSTEST.K B WHERE B.ID = A.ID)",
    );
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(r.rows[0][0].as_i64().unwrap(), 3);
    // Under *ALL a read is one view: no yield.
    let d = Desc {
        op: Op::Open,
        commit: Some(db::cmt::LckLvl::All),
        ..Desc::default()
    };
    assert!(!sql::embed::may_yield(&d));
    assert!(sql::embed::may_yield(&Desc { op: Op::Open, ..Desc::default() }));
}

#[test]
fn ending_at_a_yield_cancels_and_releases() {
    let f = fixture();
    let mut s = session(&f);
    s.ctx.yielder = Some(Rc::new(|| true));
    let r = run(&mut s, Op::Open, "C1", "SELECT ID FROM QSTEST.T");
    assert_eq!(r.sqlca.sqlstate, "57014", "{:?}", r.sqlca);
    s.ctx.yielder = None;
    run_script(&f, 2, "DROP TABLE QSTEST.T").unwrap();
}

#[test]
fn a_join_yields_between_its_outer_rows() {
    let f = fixture();
    let mut s = session(&f);
    let n = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&n);
    s.ctx.yielder = Some(Rc::new(move || {
        seen.set(seen.get() + 1);
        false
    }));
    let r = run(&mut s, Op::Query, "", "SELECT COUNT(*) FROM QSTEST.T A, QSTEST.K B WHERE A.ID = B.ID");
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(r.rows[0][0].as_i64().unwrap(), ROWS);
    // The two scans yield a few times each; the join once per outer row.
    assert!(n.get() > u32::try_from(ROWS).unwrap(), "{} yields", n.get());
}

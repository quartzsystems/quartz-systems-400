// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Queries under `*ALL`: each record read is read-locked until the unit
//! of work ends, so the query takes turns with other jobs like any
//! other and its reads stay repeatable. Other jobs read those records
//! (under `*ALL` too) but cannot change them; rows not yet read they
//! change freely.

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use blockdev::{BlockDev, MemBlockDev};
use db::cmt::LckLvl;
use db::odp::SharedLocks;
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
const ROWS: i64 = 1000;
/// `SQL0913`: the row is in use by another job.
const IN_USE: i32 = -913;

#[derive(Clone)]
struct Fixture {
    store: Rc<RefCell<Store>>,
    qsys: ObjectId,
    names: NameContext,
    locks: SharedLocks,
}

fn fixture() -> Fixture {
    let d = Arc::new(MemBlockDev::with_seed(32768, 41));
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
    let f = Fixture {
        store: Rc::new(RefCell::new(store)),
        qsys,
        names: NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
        locks: db::odp::new_shared(),
    };
    // A schema: its tables are journaled, so changes under commitment
    // control work.
    let mut text = String::from(
        "CREATE SCHEMA ALLQ;
         CREATE TABLE ALLQ.T (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);",
    );
    for chunk in 0..ROWS / 100 {
        let _ = write!(text, "INSERT INTO ALLQ.T VALUES ");
        for i in 0..100 {
            let id = chunk * 100 + i + 1;
            let _ = write!(text, "{}({id}, 'Row {id}')", if i > 0 { ", " } else { "" });
        }
        text.push(';');
    }
    let mut ctx = SqlContext::new(f.qsys, JobHandle(9), f.names.clone());
    ctx.now = Some(NOW);
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(&mut f.store.borrow_mut(), &mut ctx, &text, &opts).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    f
}

/// A job's embedded SQL, its record locks in the table every job shares.
fn job(f: &Fixture, n: u64) -> Session {
    let mut ctx = SqlContext::with_locks(f.qsys, JobHandle(n), f.names.clone(), f.locks.clone());
    ctx.now = Some(NOW);
    ctx.yield_every = Duration::ZERO;
    Session::with_ctx(Box::new(Rc::clone(&f.store)), ctx)
}

fn run(s: &mut Session, commit: Option<LckLvl>, op: Op, name: &str, text: &str) -> Reply {
    s.run(&Request {
        scope: "1:A".into(),
        desc: Desc {
            op,
            name: name.into(),
            text: text.into(),
            sql_naming: true,
            commit,
            ..Desc::default()
        },
        inputs: Vec::new(),
    })
}

const ALL: Option<LckLvl> = Some(LckLvl::All);

/// A statement's code.
fn code(s: &mut Session, commit: Option<LckLvl>, text: &str) -> i32 {
    run(s, commit, Op::Exec, "", text).sqlca.sqlcode
}

/// A one-value query's value.
fn value(s: &mut Session, commit: Option<LckLvl>, text: &str) -> String {
    let r = run(s, commit, Op::Query, "", text);
    assert_eq!(r.sqlca.sqlcode, 0, "{text}: {:?}", r.sqlca);
    r.rows[0][0].as_text().trim_end().to_string()
}

/// Every (ID, NAME) of an open cursor.
fn fetch_all(s: &mut Session, name: &str) -> Vec<(i64, String)> {
    let mut rows = Vec::new();
    loop {
        let r = run(s, ALL, Op::Fetch, name, "");
        if r.sqlca.sqlcode == 100 {
            return rows;
        }
        assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
        rows.push((r.rows[0][0].as_i64().unwrap(), r.rows[0][1].as_text().trim_end().to_string()));
    }
}

/// What the other jobs met at the query's yields.
#[derive(Default)]
struct Log {
    yields: Cell<u32>,
    ahead: Cell<Option<i32>>,
    behind: RefCell<Vec<i32>>,
    shared_reads: Cell<u32>,
}

#[test]
fn an_all_query_takes_turns_and_holds_what_it_read() {
    let f = fixture();
    let mut s1 = job(&f, 1);
    let log = Rc::new(Log::default());
    let (fx, lg) = (f.clone(), Rc::clone(&log));
    s1.ctx.yielder = Some(Rc::new(move || {
        let n = lg.yields.get() + 1;
        lg.yields.set(n);
        // Job 2 queries the table and the catalog undelayed, changes a
        // row the query has not read, and cannot change one it has.
        let mut s2 = job(&fx, 2);
        assert_eq!(value(&mut s2, None, "SELECT COUNT(*) FROM ALLQ.T"), ROWS.to_string());
        let tables = value(&mut s2, None, "SELECT COUNT(*) FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'ALLQ'");
        assert!(tables.parse::<i64>().unwrap() >= 1);
        if n == 1 {
            lg.ahead.set(Some(code(&mut s2, None, "UPDATE ALLQ.T SET NAME = 'Ahead' WHERE ID = 1000")));
        }
        lg.behind.borrow_mut().push(code(&mut s2, None, "UPDATE ALLQ.T SET NAME = 'Behind' WHERE ID = 1"));
        // Job 3 reads the rows read under *ALL too: read locks share.
        let mut s3 = job(&fx, 3);
        if value(&mut s3, ALL, "SELECT NAME FROM ALLQ.T WHERE ID = 1") == "Row 1" {
            lg.shared_reads.set(lg.shared_reads.get() + 1);
        }
        assert_eq!(code(&mut s3, ALL, "COMMIT"), 0);
        false
    }));
    let r = run(&mut s1, ALL, Op::Open, "C1", "SELECT ID, NAME FROM ALLQ.T");
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let yields = log.yields.get();
    assert!(yields >= 3, "{yields} yields over {ROWS} rows");
    assert_eq!(log.ahead.get(), Some(0), "a row not yet read is free");
    assert!(log.behind.borrow().iter().all(|&c| c == IN_USE), "{:?}", log.behind.borrow());
    assert_eq!(log.shared_reads.get(), yields);
    let rows = fetch_all(&mut s1, "C1");
    assert_eq!(rows.len(), usize::try_from(ROWS).unwrap());
    assert_eq!(rows[0], (1, "Row 1".to_string()));
    assert_eq!(rows[999], (1000, "Ahead".to_string()));
    run(&mut s1, ALL, Op::Close, "C1", "");
    s1.ctx.yielder = None;
    // The unit of work keeps the locks past the statement: reads repeat.
    let mut s2 = job(&f, 2);
    assert_eq!(code(&mut s2, None, "UPDATE ALLQ.T SET NAME = 'Behind' WHERE ID = 1"), IN_USE);
    assert_eq!(value(&mut s1, ALL, "SELECT NAME FROM ALLQ.T WHERE ID = 1"), "Row 1");
    // Its own change takes the record it read.
    assert_eq!(code(&mut s1, ALL, "UPDATE ALLQ.T SET NAME = 'Mine' WHERE ID = 2"), 0);
    // Rows added meanwhile (phantoms) are allowed at this level.
    assert_eq!(code(&mut s2, None, "INSERT INTO ALLQ.T VALUES (2000, 'New')"), 0);
    // COMMIT releases them.
    assert_eq!(code(&mut s1, ALL, "COMMIT"), 0);
    assert!(f.locks.lock().unwrap().is_empty());
    assert_eq!(code(&mut s2, None, "UPDATE ALLQ.T SET NAME = 'Behind' WHERE ID = 1"), 0);
    assert_eq!(value(&mut s1, ALL, "SELECT NAME FROM ALLQ.T WHERE ID = 2"), "Mine");
    // ROLLBACK releases them too.
    assert_eq!(code(&mut s1, ALL, "ROLLBACK"), 0);
    assert!(f.locks.lock().unwrap().is_empty());
}

#[test]
fn two_all_readers_share_and_hold_off_each_others_changes() {
    let f = fixture();
    let mut s1 = job(&f, 1);
    let mut s2 = job(&f, 2);
    let q = "SELECT COUNT(*) FROM ALLQ.T WHERE NAME LIKE 'Row%'";
    assert_eq!(value(&mut s1, ALL, q), ROWS.to_string());
    assert_eq!(value(&mut s2, ALL, q), ROWS.to_string());
    // Neither may change a row the other read; each waits for the other.
    assert_eq!(code(&mut s1, ALL, "UPDATE ALLQ.T SET NAME = 'One' WHERE ID = 5"), IN_USE);
    assert_eq!(code(&mut s2, ALL, "UPDATE ALLQ.T SET NAME = 'Two' WHERE ID = 5"), IN_USE);
    // A reader at a lower level is not held up.
    let mut s3 = job(&f, 3);
    assert_eq!(value(&mut s3, Some(LckLvl::Cs), "SELECT NAME FROM ALLQ.T WHERE ID = 5"), "Row 5");
    assert_eq!(code(&mut s3, Some(LckLvl::Cs), "COMMIT"), 0);
    // Once job 2's unit ends, job 1 changes it.
    assert_eq!(code(&mut s2, ALL, "ROLLBACK"), 0);
    assert_eq!(code(&mut s1, ALL, "UPDATE ALLQ.T SET NAME = 'One' WHERE ID = 5"), 0);
    // Now job 1 holds it for update: job 2's *ALL read of it waits.
    let r = run(&mut s2, ALL, Op::Query, "", "SELECT NAME FROM ALLQ.T WHERE ID = 5");
    assert_eq!(r.sqlca.sqlcode, IN_USE, "{:?}", r.sqlca);
    assert_eq!(code(&mut s1, ALL, "COMMIT"), 0);
    assert_eq!(value(&mut s2, ALL, "SELECT NAME FROM ALLQ.T WHERE ID = 5"), "One");
    assert_eq!(code(&mut s2, ALL, "COMMIT"), 0);
    assert!(f.locks.lock().unwrap().is_empty());
}

#[test]
fn set_transaction_without_commitment_control_holds_for_the_statement() {
    let f = fixture();
    // A context with no commit definition: the locks last the statement.
    let mut ctx = SqlContext::with_locks(f.qsys, JobHandle(1), f.names.clone(), f.locks.clone());
    ctx.naming = Naming::Sql;
    let mut st = f.store.borrow_mut();
    let mut txn = st.begin().unwrap();
    sql::prepare("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ", Naming::Sql)
        .unwrap()
        .execute(&mut txn, &mut ctx, &[])
        .unwrap();
    let p = sql::prepare("SELECT COUNT(*) FROM ALLQ.T WHERE ID <= 10", Naming::Sql).unwrap();
    p.execute(&mut txn, &mut ctx, &[]).unwrap();
    assert!(f.locks.lock().unwrap().is_empty());
    txn.commit().unwrap();
}

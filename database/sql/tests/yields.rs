// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A long statement gives the store back at its safe points: another
//! job's changes made meanwhile are seen as committed reads see them, the
//! files read cannot be dropped under it, a change commits what it wrote
//! before it yields and tests its condition again after (one with a
//! subquery as of its start), what must be one view never yields, and no
//! other job waits long for any of them.

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

/// The value of a one-row, one-column query run as job 2.
fn count_as_other(f: &Fixture, text: &str) -> i64 {
    let mut ctx = SqlContext::new(f.qsys, JobHandle(2), f.names.clone());
    ctx.now = Some(NOW);
    ctx.naming = Naming::Sql;
    let mut st = f.store.borrow_mut();
    let mut txn = st.begin().unwrap();
    let p = sql::prepare(text, Naming::Sql).unwrap();
    let v = match p.execute(&mut txn, &mut ctx, &[]).unwrap() {
        sql::Outcome::Rows(rs) => rs.rows[0][0].as_i64().unwrap(),
        o => panic!("{o:?}"),
    };
    txn.commit().unwrap();
    v
}

#[test]
fn a_change_takes_turns_between_rows() {
    let f = fixture();
    let mut s = session(&f);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let fx = Fixture {
        store: Rc::clone(&f.store),
        qsys: f.qsys,
        names: f.names.clone(),
    };
    let log = Rc::clone(&seen);
    s.ctx.yielder = Some(Rc::new(move || {
        // What the change wrote so far is committed: another job sees it.
        let done = count_as_other(&fx, "SELECT COUNT(*) FROM QSTEST.T WHERE NAME = 'All'");
        log.borrow_mut().push(done);
        if log.borrow().len() == 1 {
            // A row ahead moves out of the condition.
            run_script(&fx, 2, "UPDATE QSTEST.T SET ID = -999 WHERE ID = 999").unwrap();
        }
        false
    }));
    let r = run(&mut s, Op::Exec, "", "UPDATE QSTEST.T SET NAME = 'All' WHERE ID > 0");
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let seen = seen.borrow();
    assert!(seen.len() > 10, "{} yields", seen.len());
    assert!(seen.iter().any(|&n| n > 0 && n < ROWS), "partial changes seen: {seen:?}");
    // The row that left the condition while it ran is not changed.
    assert_eq!(r.sqlca.sqlerrd[2], i32::try_from(ROWS - 1).unwrap());
    assert_eq!(count_as_other(&f, "SELECT COUNT(*) FROM QSTEST.T WHERE ID = -999 AND NAME = 'All'"), 0);
    // A DELETE takes turns the same way.
    s.ctx.yielder = Some(Rc::new(|| false));
    let r = run(&mut s, Op::Exec, "", "DELETE FROM QSTEST.T WHERE ID > 500");
    // (Row 999 is -999 now.)
    assert_eq!(r.sqlca.sqlerrd[2], 499, "{:?}", r.sqlca);
    assert_eq!(count_as_other(&f, "SELECT COUNT(*) FROM QSTEST.T"), 501);
}

#[test]
fn what_runs_in_one_view_never_yields() {
    let f = fixture();
    let mut s = session(&f);
    let n = Rc::new(Cell::new(0u32));
    let seen = Rc::clone(&n);
    s.ctx.yielder = Some(Rc::new(move || {
        seen.set(seen.get() + 1);
        false
    }));
    // Statements other than queries and changes.
    run(&mut s, Op::Exec, "", "CREATE TABLE QSTEST.C AS (SELECT * FROM QSTEST.T) WITH DATA");
    assert_eq!(n.get(), 0, "no yield");
    // Under *ALL a read is one view: no yield, by the program's level or
    // the unit's.
    let all = Desc {
        op: Op::Open,
        commit: Some(db::cmt::LckLvl::All),
        ..Desc::default()
    };
    assert!(!sql::embed::may_yield(&all));
    assert!(sql::embed::may_yield(&Desc { op: Op::Open, ..Desc::default() }));
    assert!(sql::embed::may_yield(&Desc { op: Op::ExecImm, ..Desc::default() }));
    run(&mut s, Op::Exec, "", "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE");
    let r = run(&mut s, Op::Query, "", "SELECT COUNT(*) FROM QSTEST.T");
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(n.get(), 0, "no yield under *ALL");
}

/// The longest wait any other job's request had: the gaps between the
/// statement's start, its yields and its end. While it yields the
/// yielder runs another job's query, a catalog list and a change, as
/// the service would answer them in its turns.
fn longest_wait(f: &Fixture, op: Op, text: &str) -> (Duration, Duration, u32, Reply) {
    let mut s = session(f);
    s.ctx.yield_every = sql::exec::YIELD_EVERY;
    let marks = Rc::new(RefCell::new(vec![std::time::Instant::now()]));
    let fx = Fixture {
        store: Rc::clone(&f.store),
        qsys: f.qsys,
        names: f.names.clone(),
    };
    let log = Rc::clone(&marks);
    s.ctx.yielder = Some(Rc::new(move || {
        log.borrow_mut().push(std::time::Instant::now());
        assert_eq!(count_as_other(&fx, "SELECT COUNT(*) FROM QSTEST.K"), ROWS);
        assert!(count_as_other(&fx, "SELECT COUNT(*) FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST'") >= 2);
        run_script(&fx, 2, "UPDATE QSTEST.K SET NAME = NAME WHERE ID = 1").unwrap();
        false
    }));
    let start = std::time::Instant::now();
    let r = run(&mut s, op, "C1", text);
    let total = start.elapsed();
    let mut marks = marks.borrow().clone();
    marks.push(std::time::Instant::now());
    let gap = marks.windows(2).map(|w| w[1] - w[0]).max().unwrap();
    let yields = u32::try_from(marks.len() - 2).unwrap();
    (gap, total, yields, r)
}

/// No wait past this, debug build included.
const BOUND: Duration = Duration::from_millis(250);

#[test]
fn an_aggregate_over_exists_never_holds_others_up() {
    let f = fixture();
    // The one row's subquery reads 300 rows of K, each with a subquery
    // reading all of K (no path over NAME): the grouping step's input
    // is long to make, all of it inside one outer row.
    let (gap, total, yields, r) = longest_wait(
        &f,
        Op::Query,
        "SELECT COUNT(*) FROM QSTEST.T A WHERE A.ID <= 1 AND EXISTS (SELECT 1 FROM QSTEST.K B WHERE B.ID <= 300 \
         AND EXISTS (SELECT 1 FROM QSTEST.K C WHERE C.NAME = B.NAME AND C.ID = -A.ID))",
    );
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(r.rows[0][0].as_i64().unwrap(), 0);
    assert!(yields >= 10, "{yields} yields in {total:?}");
    assert!(gap < BOUND, "others waited {gap:?} (statement {total:?}, {yields} yields)");
}

#[test]
fn a_slow_select_never_holds_others_up() {
    let f = fixture();
    // A select list that is slow to make for each row.
    let (gap, total, yields, r) = longest_wait(
        &f,
        Op::Open,
        "SELECT A.ID, (SELECT COUNT(*) FROM QSTEST.K B WHERE B.NAME >= A.NAME) FROM QSTEST.T A WHERE A.ID <= 300",
    );
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert!(yields >= 10, "{yields} yields in {total:?}");
    assert!(gap < BOUND, "others waited {gap:?} (statement {total:?}, {yields} yields)");
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

/// Every ID of `table` in order, read as job 2.
fn ids_as_other(f: &Fixture, table: &str) -> Vec<i64> {
    let mut ctx = SqlContext::new(f.qsys, JobHandle(2), f.names.clone());
    ctx.now = Some(NOW);
    ctx.naming = Naming::Sql;
    let mut st = f.store.borrow_mut();
    let mut txn = st.begin().unwrap();
    let p = sql::prepare(&format!("SELECT ID FROM QSTEST.{table} ORDER BY ID"), Naming::Sql).unwrap();
    let ids = match p.execute(&mut txn, &mut ctx, &[]).unwrap() {
        sql::Outcome::Rows(rs) => rs.rows.iter().map(|r| r[0].as_i64().unwrap()).collect(),
        o => panic!("{o:?}"),
    };
    txn.commit().unwrap();
    ids
}

/// Changes whose condition reads the table they change, each of which
/// would decide later rows differently if it saw its own changes.
const SUBQUERY_CHANGES: &[(&str, &str, i32)] = &[
    // Its own updates raise the average past the rows still to come.
    ("T", "UPDATE QSTEST.T SET ID = ID + 2000 WHERE ID > (SELECT AVG(ID) FROM QSTEST.T)", 500),
    // Its own deletes take out the IDs later rows are matched by.
    ("T", "DELETE FROM QSTEST.T WHERE ID IN (SELECT ID + 1 FROM QSTEST.T WHERE ID < 600)", 599),
    // The same through a correlated subquery (by key).
    ("K", "DELETE FROM QSTEST.K A WHERE EXISTS (SELECT 1 FROM QSTEST.K B WHERE B.ID = A.ID - 1)", 999),
    ("T", "UPDATE QSTEST.T SET ID = ID + 1 WHERE ID < (SELECT AVG(ID) FROM QSTEST.T)", 500),
];

#[test]
fn a_subquery_change_decides_as_of_its_start() {
    for &(table, text, want) in SUBQUERY_CHANGES {
        // Without turns, as the rules decide it.
        let plain = fixture();
        let mut s = session(&plain);
        let r = run(&mut s, Op::Exec, "", text);
        assert_eq!(r.sqlca.sqlerrd[2], want, "{text}: {:?}", r.sqlca);
        let expected = ids_as_other(&plain, table);

        // Taking turns, with another job changing the rows still to come
        // (not in what the condition reads), so each is tested again.
        let f = fixture();
        let mut s = session(&f);
        let turns = Rc::new(Cell::new(0u32));
        let fx = Fixture {
            store: Rc::clone(&f.store),
            qsys: f.qsys,
            names: f.names.clone(),
        };
        let seen = Rc::clone(&turns);
        s.ctx.yielder = Some(Rc::new(move || {
            let turn = seen.get() + 1;
            seen.set(turn);
            if turn % 100 == 1 && turn < 1000 {
                let text = format!("UPDATE QSTEST.{table} SET NAME = 'Other {turn}' WHERE ID BETWEEN 400 AND 600 OR ID > 900");
                run_script(&fx, 2, &text).unwrap();
            }
            false
        }));
        let r = run(&mut s, Op::Exec, "", text);
        assert_eq!(r.sqlca.sqlerrd[2], want, "{text}: {:?}", r.sqlca);
        assert!(turns.get() > 10, "{text}: {} yields", turns.get());
        assert_eq!(ids_as_other(&f, table), expected, "{text}: the same rows as without turns");
    }
}

#[test]
fn a_subquery_change_never_holds_others_up() {
    let f = fixture();
    let (gap, total, yields, r) = longest_wait(
        &f,
        Op::Exec,
        "UPDATE QSTEST.T SET NAME = 'Sub' WHERE ID IN (SELECT ID FROM QSTEST.K WHERE ID <= 900)",
    );
    assert_eq!(r.sqlca.sqlerrd[2], 900, "{:?}", r.sqlca);
    assert!(yields >= 10, "{yields} yields in {total:?}");
    assert!(gap < BOUND, "others waited {gap:?} (statement {total:?}, {yields} yields)");
}

/// Merges whose `ON` reads the table they change, which would match
/// later rows differently if it saw its own changes, and one that
/// inserts and updates.
const MERGES: &[(&str, &str, &str, i32)] = &[
    (
        "K",
        "SELECT COUNT(*) FROM QSTEST.K WHERE ID > 2000",
        "MERGE INTO QSTEST.K A USING (SELECT ID FROM QSTEST.T) B ON A.ID = B.ID AND A.ID > (SELECT AVG(ID) FROM QSTEST.K) \
         WHEN MATCHED THEN UPDATE SET ID = A.ID + 2000",
        500,
    ),
    (
        "K",
        "SELECT COUNT(*) FROM QSTEST.K WHERE ID > 1000",
        "MERGE INTO QSTEST.K A USING (SELECT ID + 50 AS ID, NAME FROM QSTEST.T WHERE ID > 900) B ON A.ID = B.ID \
         WHEN MATCHED AND A.ID > 990 THEN DELETE WHEN MATCHED THEN UPDATE SET NAME = 'Merged' \
         WHEN NOT MATCHED THEN INSERT VALUES (B.ID, 'Inserted')",
        100,
    ),
];

#[test]
fn a_merge_takes_turns_as_of_its_start() {
    for &(table, probe, text, want) in MERGES {
        let plain = fixture();
        let mut s = session(&plain);
        let r = run(&mut s, Op::Exec, "", text);
        assert_eq!(r.sqlca.sqlerrd[2], want, "{text}: {:?}", r.sqlca);
        let expected = ids_as_other(&plain, table);

        let f = fixture();
        let mut s = session(&f);
        let turns = Rc::new(Cell::new(0u32));
        let fx = Fixture {
            store: Rc::clone(&f.store),
            qsys: f.qsys,
            names: f.names.clone(),
        };
        let seen = Rc::clone(&turns);
        let fired = Rc::new(Cell::new(0u32));
        let shot = Rc::clone(&fired);
        s.ctx.yielder = Some(Rc::new(move || {
            let turn = seen.get() + 1;
            seen.set(turn);
            // Once it writes (its first changes are seen), another job
            // changes the rows still to come, a few times.
            if turn.is_multiple_of(20) && shot.get() < 3 && count_as_other(&fx, probe) > 0 {
                shot.set(shot.get() + 1);
                let text = format!("UPDATE QSTEST.{table} SET NAME = 'Other {turn}' WHERE ID BETWEEN 600 AND 1000");
                run_script(&fx, 2, &text).unwrap();
            }
            false
        }));
        let r = run(&mut s, Op::Exec, "", text);
        assert_eq!(r.sqlca.sqlerrd[2], want, "{text}: {:?}", r.sqlca);
        assert!(turns.get() > 10, "{text}: {} yields", turns.get());
        assert!(fired.get() > 0, "{text}: no changes by the other job while it wrote");
        assert_eq!(ids_as_other(&f, table), expected, "{text}: the same rows as without turns");
    }
}

#[test]
fn a_merge_never_holds_others_up() {
    let f = fixture();
    let (gap, total, yields, r) = longest_wait(
        &f,
        Op::Exec,
        "MERGE INTO QSTEST.T A USING (SELECT * FROM QSTEST.K WHERE ID <= 200) B ON A.ID = B.ID \
         WHEN MATCHED THEN UPDATE SET NAME = B.NAME",
    );
    assert_eq!(r.sqlca.sqlerrd[2], 200, "{:?}", r.sqlca);
    assert!(yields >= 10, "{yields} yields in {total:?}");
    assert!(gap < BOUND, "others waited {gap:?} (statement {total:?}, {yields} yields)");
}

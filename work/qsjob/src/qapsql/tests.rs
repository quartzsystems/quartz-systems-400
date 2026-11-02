// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lane and the catalog over the engine itself, a session over a
//! store in memory, as the storage service runs it for a job.

#![allow(clippy::many_single_char_names)] // short names in the checks

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qap_proto::ops::sql::{
    CommitLevel, Execute, Fetch, ListColumns, ListSchemas, ListTables, Naming, SetOptions, SqlValue, TableKind,
};
use qap_proto::{ServerMessage, Status};
use qapd::sql::SqlStop;
use qsname::Name;
use sql::embed::{Embedded, Reply, Request, Session};
use sql::{RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

use super::{Engine, Lane, catalog};

const NOW: u64 = 1_767_225_600;

struct Mem(Session);

impl Engine for Mem {
    fn run(&mut self, req: &Request) -> Reply {
        self.0.run(req)
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        ServerMessage {
            id: id.into(),
            severity: 30,
            data: data.join(" ").into_bytes(),
            text: id.into(),
            help: String::new(),
        }
    }
}

fn engine() -> Mem {
    let d = Arc::new(MemBlockDev::with_seed(16384, 21));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
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
    let mut ctx = SqlContext::new(qsys, JobHandle(1), names.clone());
    ctx.now = Some(NOW);
    let text = "CREATE TABLE QSTEST.T (ID INT NOT NULL PRIMARY KEY, NAME VARCHAR(20), D DATE, TS TIMESTAMP, C DECIMAL(9, 2), F DOUBLE);
        INSERT INTO QSTEST.T VALUES (1, 'Ann', '2026-11-02', '2026-11-02-09.14.03.123456', 10.50, 4.5), (2, NULL, NULL, NULL, NULL, NULL);";
    let opts = RunOpts {
        naming: sql::Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(&mut store, &mut ctx, text, &opts).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    Mem(Session::new(Box::new(Rc::new(RefCell::new(store))), qsys, JobHandle(1), names))
}

fn never() -> bool {
    false
}

fn failed<T: std::fmt::Debug>(r: Result<T, SqlStop<T>>) -> (String, i32, Option<u32>) {
    match r {
        Err(SqlStop::Failed(f)) => (f.failure.sqlstate, f.failure.sqlcode, f.failure.row),
        other => panic!("not an SqlError: {other:?}"),
    }
}

#[test]
fn a_query_in_batches_with_its_types() {
    let mut e = engine();
    let mut lane = Lane::default();
    let ins = lane.prepare(&mut e, "INSERT INTO QSTEST.T (ID, NAME) VALUES (?, ?)").unwrap();
    assert_eq!(ins.parameters.len(), 2);
    let rows: Vec<Vec<SqlValue>> = (3..=252).map(|i| vec![SqlValue::Integer { value: i }, SqlValue::Text { text: format!("n{i}") }]).collect();
    let x = lane.execute(&mut e, &Execute { statement: ins.statement, rows }, &never).unwrap();
    assert_eq!((x.updated, x.cursor), (250, None));
    let q = lane.prepare(&mut e, "SELECT * FROM QSTEST.T").unwrap();
    let types: Vec<&str> = q.columns.iter().map(|c| c.sql_type.as_str()).collect();
    assert_eq!(types, ["INTEGER", "VARCHAR", "DATE", "TIMESTAMP", "DECIMAL", "DOUBLE"]);
    assert_eq!((q.columns[4].length, q.columns[4].scale, q.columns[3].scale), (9, 2, 6));
    let cursor = lane.execute(&mut e, &Execute { statement: q.statement, rows: Vec::new() }, &never).unwrap().cursor.unwrap();
    let first = lane.fetch(&mut e, &Fetch { cursor, max_rows: 1 }, &never).unwrap();
    assert!(!first.end);
    let r = &first.rows[0];
    assert_eq!(r[0], SqlValue::Integer { value: 1 });
    assert_eq!(r[1], SqlValue::Text { text: "Ann".into() });
    assert_eq!(r[2], SqlValue::Date { ms: 1_793_577_600_000 });
    assert_eq!(r[3], SqlValue::Timestamp { ms: 1_793_610_843_123, picos: 456_000_000 });
    assert_eq!(r[4], SqlValue::Decimal { text: "10.50".into() });
    // The engine keeps a DOUBLE to the scale of its field (0): 4, not 4.5.
    assert_eq!(r[5], SqlValue::Float { text: "4".into() });
    let rest = lane.fetch(&mut e, &Fetch { cursor, max_rows: 0 }, &never).unwrap();
    assert_eq!((rest.rows.len(), rest.end), (251, true));
    assert!(rest.rows[0][1..].iter().all(|v| *v == SqlValue::Null));
    lane.close_cursor(&mut e, cursor).unwrap();
    assert_eq!(lane.close_cursor(&mut e, cursor).unwrap_err().status, Status::NotFound);
    assert!(matches!(lane.fetch(&mut e, &Fetch { cursor, max_rows: 0 }, &never), Err(SqlStop::Refused(r)) if r.status == Status::NotFound));
    lane.release(&mut e, q.statement).unwrap();
    assert_eq!(lane.release(&mut e, q.statement).unwrap_err().status, Status::NotFound);
}

#[test]
fn failures_are_the_engines() {
    let mut e = engine();
    let mut lane = Lane::default();
    assert_eq!(failed(lane.prepare(&mut e, "SELEC 1")).1, -104);
    assert_eq!(failed(lane.prepare(&mut e, "SELECT * FROM QSTEST.NOSUCH")).0, "42704");
    // A duplicate key at the third row; the two before it stay.
    let ins = lane.prepare(&mut e, "INSERT INTO QSTEST.T (ID) VALUES (?)").unwrap();
    let rows = [10, 11, 1, 12].iter().map(|i| vec![SqlValue::Integer { value: *i }]).collect();
    let r = lane.execute(&mut e, &Execute { statement: ins.statement, rows }, &never);
    assert_eq!(failed(r), ("23505".into(), -803, Some(2)));
    let n = lane.prepare(&mut e, "SELECT ID FROM QSTEST.T WHERE ID >= 10").unwrap();
    let c = lane.execute(&mut e, &Execute { statement: n.statement, rows: Vec::new() }, &never).unwrap().cursor.unwrap();
    assert_eq!(lane.fetch(&mut e, &Fetch { cursor: c, max_rows: 0 }, &never).unwrap().rows.len(), 2);
    // Too few parameters, and one the engine cannot take.
    let r = lane.execute(&mut e, &Execute { statement: ins.statement, rows: vec![Vec::new()] }, &never);
    assert_eq!(failed(r).0, "07001");
    let r = lane.execute(&mut e, &Execute { statement: ins.statement, rows: vec![vec![SqlValue::Decimal { text: "sNaN".into() }]] }, &never);
    assert_eq!(failed(r), ("22018".into(), -302, Some(0)));
}

#[test]
fn an_unjournaled_table_under_commitment_control() {
    let mut e = engine();
    let mut lane = Lane::default();
    let o = SetOptions {
        commit: CommitLevel::ReadCommitted,
        naming: Naming::Sql,
        default_schema: Some("QSTEST".into()),
    };
    lane.set_options(&mut e, &o).unwrap();
    let ins = lane.prepare(&mut e, "INSERT INTO T (ID) VALUES (?)").unwrap();
    let r = lane.execute(&mut e, &Execute { statement: ins.statement, rows: vec![vec![SqlValue::Integer { value: 5 }]] }, &never);
    let (state, code, row) = failed(r);
    assert_eq!((state.as_str(), code, row), ("55019", -7008, Some(0)));
    // The statement opened a unit of work: options wait for its end.
    assert_eq!(lane.set_options(&mut e, &SetOptions::default()).unwrap_err().status, Status::InUse);
    lane.end_unit(&mut e, false).unwrap();
    lane.set_options(&mut e, &SetOptions::default()).unwrap();
    lane.end_unit(&mut e, true).unwrap();
}

#[test]
fn set_options_changes_the_level_between_units() {
    let mut e = engine();
    let mut lane = Lane::default();
    let level = |e: &Mem| e.0.ctx.session.commit_def().map(|d| d.borrow().lcklvl().name());
    let at = |commit| SetOptions {
        commit,
        ..SetOptions::default()
    };
    lane.set_options(&mut e, &at(CommitLevel::ReadUncommitted)).unwrap();
    let q = lane.prepare(&mut e, "SELECT ID FROM QSTEST.T").unwrap();
    let c = lane.execute(&mut e, &Execute { statement: q.statement, rows: Vec::new() }, &never).unwrap().cursor.unwrap();
    assert_eq!(level(&e), Some("*CHG"));
    lane.set_options(&mut e, &at(CommitLevel::RepeatableRead)).unwrap();
    lane.fetch(&mut e, &Fetch { cursor: c, max_rows: 1 }, &never).unwrap();
    assert_eq!(level(&e), Some("*ALL"));
}

#[test]
fn a_default_schema_under_system_naming() {
    let mut e = engine();
    let mut lane = Lane::default();
    let sys = |schema: Option<&str>| SetOptions {
        commit: CommitLevel::None,
        naming: Naming::System,
        default_schema: schema.map(str::to_string),
    };
    lane.set_options(&mut e, &sys(None)).unwrap();
    lane.prepare(&mut e, "SELECT * FROM T").unwrap();
    lane.set_options(&mut e, &sys(Some("QSYS"))).unwrap();
    assert_eq!(failed(lane.prepare(&mut e, "SELECT * FROM T")).0, "42704");
    lane.set_options(&mut e, &sys(Some("QSTEST"))).unwrap();
    lane.prepare(&mut e, "SELECT * FROM T").unwrap();
}

#[test]
fn a_cancel_ends_the_running_statement() {
    let mut e = engine();
    let mut lane = Lane::default();
    let q = lane.prepare(&mut e, "SELECT * FROM QSTEST.T").unwrap();
    e.0.ctx.interrupt = Some(Rc::new(|| true));
    let r = lane.execute(&mut e, &Execute { statement: q.statement, rows: Vec::new() }, &never);
    match r {
        Err(SqlStop::Cancelled(x, m)) => {
            assert_eq!((x.updated, x.cursor), (0, None));
            assert_eq!(m.id, "SQL0952");
        }
        other => panic!("{other:?}"),
    }
    e.0.ctx.interrupt = None;
    // Between parameter rows and between batches, the lane looks too.
    let ins = lane.prepare(&mut e, "INSERT INTO QSTEST.T (ID) VALUES (?)").unwrap();
    let rows = (20..30).map(|i| vec![SqlValue::Integer { value: i }]).collect();
    let calls = std::cell::Cell::new(0);
    let after_three = || {
        calls.set(calls.get() + 1);
        calls.get() > 3
    };
    match lane.execute(&mut e, &Execute { statement: ins.statement, rows }, &after_three) {
        Err(SqlStop::Cancelled(x, _)) => assert_eq!(x.updated, 3),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_end_rolls_back_and_forgets() {
    let mut e = engine();
    let mut lane = Lane::default();
    let q = lane.prepare(&mut e, "SELECT * FROM QSTEST.T").unwrap();
    let c = lane.execute(&mut e, &Execute { statement: q.statement, rows: Vec::new() }, &never).unwrap().cursor.unwrap();
    lane.end(&mut e);
    assert!(lane.fetch(&mut e, &Fetch { cursor: c, max_rows: 1 }, &never).is_err());
    assert!(lane.execute(&mut e, &Execute { statement: q.statement, rows: Vec::new() }, &never).is_err());
}

#[test]
fn the_catalog() {
    let mut e = engine();
    let s = catalog::list_schemas(&mut e, &ListSchemas { pattern: "QS%".into(), start: 0, max: 0 }).unwrap();
    let names: Vec<&str> = s.schemas.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"QSTEST"), "{names:?}");
    let t = catalog::list_tables(
        &mut e,
        &ListTables {
            schema: "QSTEST".into(),
            pattern: "%".into(),
            kinds: Vec::new(),
            start: 0,
            max: 0,
        },
    )
    .unwrap();
    assert_eq!(t.total, 1);
    assert_eq!((t.tables[0].name.as_str(), t.tables[0].kind), ("T", TableKind::Table));
    let missing = ListTables {
        schema: "NOSUCH".into(),
        ..ListTables::default()
    };
    assert_eq!(catalog::list_tables(&mut e, &missing).unwrap_err().status, Status::NotFound);
    let c = catalog::list_columns(
        &mut e,
        &ListColumns {
            schema: "QSTEST".into(),
            table: "T".into(),
        },
    )
    .unwrap();
    let names: Vec<&str> = c.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["ID", "NAME", "D", "TS", "C", "F"]);
    assert!(!c.columns[0].nullable && c.columns[1].nullable);
    let none = ListColumns {
        schema: "QSTEST".into(),
        table: "NOSUCH".into(),
    };
    assert_eq!(catalog::list_columns(&mut e, &none).unwrap_err().status, Status::NotFound);
}

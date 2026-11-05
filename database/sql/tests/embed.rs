// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Embedded SQL sessions: `DESCRIBE` of prepared statements and the end
//! of a scope or an activation group closing cursors.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, RunOpts, SqlContext, Value, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn session() -> Session {
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
    let text = "CREATE TABLE QSTEST.T (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL, DEPT CHAR(3), SAL DECIMAL(9, 2));
        INSERT INTO QSTEST.T VALUES (1, 'Ann', 'ACC', 10.00), (2, 'Bob', 'DEV', NULL);";
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(&mut store, &mut ctx, text, &opts).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    // T is journaled, for commitment control.
    let mut t = store.begin().unwrap();
    let rcv = jrn::rcv::crtjrnrcv(&mut t, lib, Name::parse("TRCV").unwrap(), &jrn::rcv::CrtJrnRcvOpts::default()).unwrap();
    let jid = jrn::journal::crtjrn(&mut t, lib, Name::parse("TJRN").unwrap(), rcv, &jrn::journal::CrtJrnOpts::default()).unwrap();
    let tid = ctx.resolve_file(&mut t, &sql::ast::ObjectName::new("T")).unwrap();
    db::journal::strjrnpf(&mut t, JobHandle(1), tid, jid, false, true, 1).unwrap();
    t.commit().unwrap();
    Session::new(Box::new(Rc::new(RefCell::new(store))), qsys, JobHandle(1), names)
}

fn run(s: &mut Session, scope: &str, desc: Desc, inputs: Vec<Value>) -> Reply {
    s.run(&Request {
        scope: scope.into(),
        desc,
        inputs,
    })
}

fn op(op: Op, name: &str, text: &str) -> Desc {
    Desc {
        op,
        name: name.into(),
        text: text.into(),
        ..Desc::default()
    }
}

/// The entries of a reply: SQLTYPE, SQLLEN, SQLNAME.
fn entries(r: &Reply) -> Vec<(i64, i64, String)> {
    r.rows
        .iter()
        .map(|row| (row[0].as_i64().unwrap(), row[1].as_i64().unwrap(), row[2].as_text()))
        .collect()
}

#[test]
fn describe_gives_the_columns_and_the_markers() {
    let mut s = session();
    let r = run(&mut s, "1:P", op(Op::Prepare, "S1", "SELECT ID, NAME, DEPT, SAL, ID * 2 AS TWICE, COUNT(*) AS N FROM T GROUP BY ID, NAME, DEPT, SAL"), vec![]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let r = run(&mut s, "1:P", op(Op::Describe, "S1", ""), vec![]);
    assert_eq!(
        entries(&r),
        [
            (496, 4, "ID".into()),
            (448, 20, "NAME".into()),
            (453, 3, "DEPT".into()),
            (485, 9 * 256 + 2, "SAL".into()),
            (496, 4, "TWICE".into()),
            (496, 4, "N".into()),
        ]
    );
    run(&mut s, "1:P", op(Op::Prepare, "S2", "SELECT NAME FROM T WHERE ID = ? AND DEPT = ?"), vec![]);
    let mut d = op(Op::Describe, "S2", "");
    d.orient = "INPUT".into();
    let r = run(&mut s, "1:P", d, vec![]);
    assert_eq!(entries(&r), [(497, 4, "00001".into()), (453, 3, "00002".into())]);
    // Not a query: no entries; never prepared: -516.
    run(&mut s, "1:P", op(Op::Prepare, "S3", "DELETE FROM T WHERE ID = 99"), vec![]);
    assert!(run(&mut s, "1:P", op(Op::Describe, "S3", ""), vec![]).rows.is_empty());
    assert_eq!(run(&mut s, "1:P", op(Op::Describe, "S9", ""), vec![]).sqlca.sqlcode, -516);
}

#[test]
fn scopes_end_with_the_module_or_the_group() {
    let mut s = session();
    let open = op(Op::Open, "C1", "SELECT ID FROM T ORDER BY ID");
    for scope in ["1:A", "1:B", "2:A"] {
        assert_eq!(run(&mut s, scope, open.clone(), vec![]).sqlca.sqlcode, 0);
    }
    // The module's end closes its cursor in that group only.
    run(&mut s, "1:A", op(Op::End, "", ""), vec![]);
    assert_eq!(run(&mut s, "1:A", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "2:A", open.clone(), vec![]).sqlca.sqlcode, -502);
    // The group's end closes all of its scopes.
    run(&mut s, "1:", op(Op::End, "", ""), vec![]);
    assert_eq!(run(&mut s, "1:A", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "1:B", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "2:A", open, vec![]).sqlca.sqlcode, -502);
}

/// The lock level of the session's commitment definition.
fn level(s: &Session) -> Option<db::cmt::LckLvl> {
    s.ctx.session.commit_def().map(|d| d.borrow().lcklvl())
}

fn under(text: &str, commit: Option<db::cmt::LckLvl>) -> Desc {
    Desc {
        op: Op::Exec,
        text: text.into(),
        sql_naming: true,
        commit,
        ..Desc::default()
    }
}

#[test]
fn the_lock_level_changes_between_units_of_work() {
    use db::cmt::LckLvl::{All, Chg, Cs};
    let mut s = session();
    let upd = "UPDATE QSTEST.T SET SAL = 1 WHERE ID = 1";
    assert_eq!(run(&mut s, "1:P", under(upd, Some(Chg)), vec![]).sqlca.sqlcode, 0);
    assert_eq!(level(&s), Some(Chg));
    // A change is pending: the unit keeps its level.
    assert_eq!(run(&mut s, "1:P", under(upd, Some(All)), vec![]).sqlca.sqlcode, 0);
    assert_eq!(level(&s), Some(Chg));
    let r = run(&mut s, "1:P", under("SET TRANSACTION ISOLATION LEVEL READ COMMITTED", Some(All)), vec![]);
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (-428, "25001"));
    run(&mut s, "1:P", under("COMMIT", Some(All)), vec![]);
    // The next unit takes the statement's level.
    assert_eq!(run(&mut s, "1:P", under(upd, Some(All)), vec![]).sqlca.sqlcode, 0);
    assert_eq!(level(&s), Some(All));
    run(&mut s, "1:P", under("ROLLBACK", Some(All)), vec![]);
    // SET TRANSACTION holds for its unit, over the statements' level.
    let r = run(&mut s, "1:P", under("SET TRANSACTION ISOLATION LEVEL READ COMMITTED", Some(All)), vec![]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(level(&s), Some(Cs));
    assert_eq!(run(&mut s, "1:P", under(upd, Some(All)), vec![]).sqlca.sqlcode, 0);
    assert_eq!(level(&s), Some(Cs));
    run(&mut s, "1:P", under("COMMIT", Some(All)), vec![]);
    run(&mut s, "1:P", under(upd, Some(All)), vec![]);
    assert_eq!(level(&s), Some(All));
    run(&mut s, "1:P", under("COMMIT", Some(All)), vec![]);
}

#[test]
fn a_default_schema_applies_under_system_naming() {
    let mut s = session();
    let sys = |text: &str| Desc {
        op: Op::Exec,
        text: text.into(),
        ..Desc::default()
    };
    let count = |s: &mut Session| run(s, "1:P", op(Op::Query, "", "SELECT COUNT(*) FROM T"), vec![]).sqlca.sqlcode;
    // The library list finds T in QSTEST.
    assert_eq!(count(&mut s), 0);
    // A default schema replaces the library list for table names.
    assert_eq!(run(&mut s, "1:P", sys("SET SCHEMA QSYS"), vec![]).sqlca.sqlcode, 0);
    assert_eq!(count(&mut s), -204);
    run(&mut s, "1:P", sys("SET SCHEMA DEFAULT"), vec![]);
    assert_eq!(count(&mut s), 0);
}

#[test]
fn a_prepared_statement_can_be_deallocated() {
    let mut s = session();
    assert_eq!(run(&mut s, "1:P", op(Op::Prepare, "S1", "SELECT ID FROM T"), vec![]).sqlca.sqlcode, 0);
    let mut open = op(Op::Open, "C1", "");
    open.stmt = "S1".into();
    assert_eq!(run(&mut s, "1:P", open, vec![]).sqlca.sqlcode, 0);
    // Another scope's statement of that name is not this one.
    assert_eq!(run(&mut s, "1:Q", op(Op::Deallocate, "S1", ""), vec![]).sqlca.sqlcode, -514);
    assert_eq!(run(&mut s, "1:P", op(Op::Deallocate, "S1", ""), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "1:P", op(Op::Deallocate, "S1", ""), vec![]).sqlca.sqlcode, -514);
    assert_eq!(run(&mut s, "1:P", op(Op::Execute, "S1", ""), vec![]).sqlca.sqlcode, -518);
    // The cursor opened from it stays open.
    assert_eq!(run(&mut s, "1:P", op(Op::Fetch, "C1", ""), vec![]).sqlca.sqlcode, 0);
}

#[test]
fn describe_gives_sql_names_and_labels() {
    let mut s = session();
    let sql = |text: &str| Desc {
        op: Op::Exec,
        text: text.into(),
        sql_naming: true,
        ..Desc::default()
    };
    for text in [
        "CREATE TABLE QSTEST.L (CUSTOMER_NAME FOR COLUMN CUSNAM VARCHAR(20), ID INT)",
        "LABEL ON COLUMN QSTEST.L.CUSTOMER_NAME IS 'Customer            Name'",
    ] {
        let r = run(&mut s, "1:P", sql(text), vec![]);
        // SQL7905: QSTEST is not a schema.
        assert!(r.sqlca.sqlcode >= 0, "{text}: {:?}", r.sqlca);
    }
    let named = |s: &mut Session, text: &str| {
        let mut p = op(Op::Prepare, "S1", text);
        p.into = true;
        let r = run(s, "1:P", p, vec![]);
        assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
        r.rows.iter().map(|row| (row[2].as_text(), row[4].as_text())).collect::<Vec<_>>()
    };
    let label = "Customer            Name".to_string();
    assert_eq!(
        named(&mut s, "SELECT * FROM L"),
        [("CUSTOMER_NAME".to_string(), label.clone()), ("ID".into(), String::new())]
    );
    // By either name, and under a new name the label stays.
    assert_eq!(
        named(&mut s, "SELECT CUSNAM, CUSTOMER_NAME AS WHO, ID + 1 AS N FROM L"),
        [("CUSTOMER_NAME".to_string(), label.clone()), ("WHO".into(), label), ("N".into(), String::new())]
    );
}

#[test]
#[allow(clippy::many_single_char_names)] // handles a and b, the query q
fn locators_stand_for_values_until_the_unit_ends() {
    use db::cmt::LckLvl::Chg;
    let mut s = session();
    let q = Desc { op: Op::Query, locs: "O0".into(), ..under("SELECT NAME FROM QSTEST.T WHERE ID = 1", Some(Chg)) };
    let r = run(&mut s, "1:P", q.clone(), vec![]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let h = r.rows[0][0].clone();
    let len = Desc { op: Op::Query, locs: "I0".into(), ..under("VALUES LENGTH(?)", Some(Chg)) };
    let r = run(&mut s, "1:P", len.clone(), vec![h.clone()]);
    assert_eq!((r.sqlca.sqlcode, r.rows[0][0].as_i64().unwrap()), (0, 3));
    assert_eq!(run(&mut s, "1:P", under("FREE LOCATOR ?", Some(Chg)), vec![h.clone()]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "1:P", len.clone(), vec![h.clone()]).sqlca.sqlcode, -423);
    assert_eq!(run(&mut s, "1:P", under("FREE LOCATOR ?", Some(Chg)), vec![h]).sqlca.sqlcode, -423);
    // Held across COMMIT; the others go.
    let a = run(&mut s, "1:P", q.clone(), vec![]).rows[0][0].clone();
    let b = run(&mut s, "1:P", q, vec![]).rows[0][0].clone();
    assert_eq!(run(&mut s, "1:P", under("HOLD LOCATOR ?", Some(Chg)), vec![a.clone()]).sqlca.sqlcode, 0);
    run(&mut s, "1:P", under("COMMIT", Some(Chg)), vec![]);
    assert_eq!(run(&mut s, "1:P", len.clone(), vec![a]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "1:P", len, vec![b]).sqlca.sqlcode, -423);
    // Without commitment control there are none.
    let q = Desc { op: Op::Query, locs: "O0".into(), ..under("SELECT NAME FROM QSTEST.T WHERE ID = 1", None) };
    assert_eq!(run(&mut s, "2:P", q, vec![]).sqlca.sqlcode, -7034);
}

#[test]
fn a_blocked_insert_runs_once_per_row() {
    let mut s = session();
    let d = Desc { rows: true, ..under("INSERT INTO QSTEST.T (ID, NAME) VALUES (?, ?)", None) };
    let r = run(&mut s, "1:P", d.clone(), vec![Value::int(7), Value::Str("Kim".into()), Value::int(8), Value::Str("Lou".into()), Value::int(2)]);
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlerrd[2]), (0, 2));
    let diag = run(&mut s, "1:P", Desc { op: Op::Diag, text: "ROW_COUNT,COMMAND_FUNCTION".into(), ..Desc::default() }, vec![]);
    assert_eq!(diag.rows[0], vec![Value::int(2), Value::Str("INSERT".into())]);
    // A failing row stops the run; the rows before it stay.
    let r = run(&mut s, "1:P", d, vec![Value::int(9), Value::Str("Max".into()), Value::int(10), Value::Null, Value::int(2)]);
    assert_eq!((r.sqlca.sqlcode < 0, r.sqlca.sqlerrd[2]), (true, 1));
}

#[test]
fn a_sensitive_cursor_is_refused() {
    let mut s = session();
    let d = Desc { sensitive: true, ..op(Op::Open, "C1", "SELECT ID FROM QSTEST.T") };
    let r = run(&mut s, "1:P", d, vec![]);
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (-243, "36001"));
}

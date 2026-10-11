// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog views, the SQLCA, cursors with positioned changes,
//! `MERGE`, views, labels and the statement-file runner.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{CommitOpt, Naming, Outcome, RunOpts, SqlContext, Value, prepare, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

fn open_new() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 13));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let (qsys, lib) = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
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
                now: 1_767_225_600,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        (qsys, lib)
    };
    (store, qsys, lib)
}

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let (store, qsys, lib) = open_new();
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        ctx.now = Some(1_767_225_600);
        Self { store, ctx }
    }

    /// Run through the SQLCA path.
    fn run(&mut self, text: &str) -> (sql::Sqlca, Option<Outcome>) {
        let p = prepare(text, Naming::Sys).unwrap_or_else(|e| panic!("{text}: {e}"));
        let mut t = self.store.begin().unwrap();
        let r = p.run(&mut t, &mut self.ctx, &[]);
        if r.0.is_error() {
            t.abort();
        } else {
            t.commit().unwrap();
        }
        r
    }

    fn ok(&mut self, text: &str) -> Outcome {
        let (ca, o) = self.run(text);
        assert!(!ca.is_error(), "{text}: {}", ca.line());
        o.unwrap()
    }

    fn rows(&mut self, text: &str) -> Vec<Vec<String>> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|v| {
                            if v.is_null() {
                                "-".into()
                            } else {
                                v.to_string().trim_end().to_string()
                            }
                        })
                        .collect()
                })
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, text: &str) -> i32 {
        self.run(text).0.sqlcode
    }
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

fn setup(db: &mut Db) {
    db.ok("CREATE TABLE qstest/cust (cusno DECIMAL(7, 0) NOT NULL PRIMARY KEY, name CHAR(30) NOT NULL, bal NUMERIC(9, 2) NOT NULL DEFAULT 0, since DATE, note VARCHAR(100))");
    db.ok("CREATE TABLE qstest/orders (ordno INTEGER NOT NULL PRIMARY KEY, cusno DECIMAL(7, 0) NOT NULL, amount DECIMAL(9, 2) NOT NULL)");
    db.ok("CREATE INDEX qstest/cusbyname ON cust (name)");
    db.ok("INSERT INTO cust (cusno, name, bal, since) VALUES (1, 'Alice', 10.50, '2026-01-15'), (2, 'Bob', 0, NULL), (3, 'Carol', 250, NULL), (4, 'Dave', 0, NULL)");
}

#[test]
fn catalogs() {
    let mut db = Db::new();
    setup(&mut db);
    assert_eq!(
        db.rows("SELECT TABLE_NAME, TABLE_TYPE, COLUMN_COUNT, ROW_LENGTH, FILE_TYPE FROM qsys2/systables WHERE TABLE_SCHEMA = 'QSTEST' ORDER BY TABLE_NAME"),
        [
            row(&["CUSBYNAME", "I", "5", "155", "D"]),
            row(&["CUST", "T", "5", "155", "D"]),
            row(&["ORDERS", "T", "3", "13", "D"]),
        ]
    );
    // Unqualified works too, and the catalog has its own columns described.
    let Outcome::Rows(rs) = db.ok("SELECT * FROM systables WHERE TABLE_NAME = 'CUST'") else {
        panic!()
    };
    assert_eq!(rs.columns[0].name, "TABLE_NAME");
    assert_eq!(rs.columns[0].data_type, "CHAR(128)");
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(
        db.rows("SELECT COLUMN_NAME, ORDINAL_POSITION, DATA_TYPE, LENGTH, NUMERIC_SCALE, IS_NULLABLE, HAS_DEFAULT FROM qsys2/syscolumns WHERE TABLE_NAME = 'CUST' AND TABLE_SCHEMA = 'QSTEST' ORDER BY ORDINAL_POSITION"),
        [
            row(&["CUSNO", "1", "DECIMAL", "7", "0", "N", "N"]),
            row(&["NAME", "2", "CHAR", "30", "-", "N", "N"]),
            row(&["BAL", "3", "NUMERIC", "9", "2", "N", "Y"]),
            row(&["SINCE", "4", "DATE", "10", "-", "Y", "N"]),
            row(&["NOTE", "5", "VARCHAR", "100", "-", "Y", "N"]),
        ]
    );
    assert_eq!(
        db.rows("SELECT INDEX_NAME, TABLE_NAME, IS_UNIQUE, COLUMN_COUNT FROM qsys2/sysindexes WHERE INDEX_SCHEMA = 'QSTEST'"),
        [row(&["CUSBYNAME", "CUST", "D", "1"])]
    );
    assert_eq!(
        db.rows("SELECT k.COLUMN_NAME, k.ORDERING FROM qsys2/sysindexes i JOIN qsys2/syskeys k ON i.INDEX_NAME = k.INDEX_NAME WHERE i.TABLE_NAME = 'CUST'"),
        [row(&["NAME", "A"])]
    );
    assert_eq!(
        db.rows("SELECT CONSTRAINT_NAME, CONSTRAINT_TYPE, TABLE_NAME, CONSTRAINT_KEYS FROM qsys2/syscst WHERE TABLE_SCHEMA = 'QSTEST' ORDER BY TABLE_NAME"),
        [
            row(&["Q_QSTEST_CUST_CUSNO_00001", "PRIMARY KEY", "CUST", "1"]),
            row(&["Q_QSTEST_ORDERS_ORDNO_00001", "PRIMARY KEY", "ORDERS", "1"]),
        ]
    );
    assert_eq!(
        db.rows(
            "SELECT COLUMN_NAME, ORDINAL_POSITION FROM qsys2/syskeycst WHERE TABLE_NAME = 'CUST'"
        ),
        [row(&["CUSNO", "1"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM qsys2/sysrefcst"),
        [row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT SCHEMA_NAME FROM qsys2/sysschemas ORDER BY SCHEMA_NAME"),
        [row(&["QSTEST"]), row(&["QSYS"])]
    );
    assert_eq!(
        db.rows("SELECT NUMBER_ROWS, NUMBER_DELETED_ROWS, NUMBER_PARTITIONS FROM qsys2/systablestat WHERE TABLE_NAME = 'CUST'"),
        [row(&["4", "0", "1"])]
    );
    assert_eq!(
        db.rows("SELECT TABLE_PARTITION, NUMBER_ROWS FROM qsys2/syspartitionstat WHERE TABLE_NAME = 'ORDERS'"),
        [row(&["ORDERS", "0"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM qsys2/syscolumnstat WHERE TABLE_NAME = 'CUST'"),
        [row(&["5"])]
    );
    // Labels show up in the catalog.
    db.ok("LABEL ON TABLE cust IS 'Customers'");
    db.ok("LABEL ON COLUMN cust.name IS 'Customer name'");
    db.ok("COMMENT ON COLUMN cust.bal IS 'Balance owed'");
    db.ok("LABEL ON TABLE cust (cusno IS 'Customer number', note TEXT IS 'Free text')");
    assert_eq!(
        db.rows("SELECT TABLE_TEXT FROM qsys2/systables WHERE TABLE_NAME = 'CUST'"),
        [row(&["Customers"])]
    );
    assert_eq!(
        db.rows("SELECT COLUMN_NAME, COLUMN_HEADING, COLUMN_TEXT FROM qsys2/syscolumns WHERE TABLE_NAME = 'CUST' AND (COLUMN_HEADING IS NOT NULL OR COLUMN_TEXT IS NOT NULL) ORDER BY ORDINAL_POSITION"),
        [
            row(&["CUSNO", "Customer number", "-"]),
            row(&["NAME", "Customer name", "-"]),
            row(&["BAL", "-", "Balance owed"]),
            row(&["NOTE", "-", "Free text"]),
        ]
    );
    // A view: a logical file with select/omit, listed with its dependency.
    db.ok("CREATE VIEW qstest/active (c, n, b) AS SELECT cusno, name, bal FROM cust WHERE bal > 0");
    assert_eq!(
        db.rows("SELECT c, n FROM active ORDER BY c"),
        [row(&["1", "Alice"]), row(&["3", "Carol"])]
    );
    assert_eq!(
        db.rows("SELECT VIEW_NAME, IS_UPDATABLE FROM qsys2/sysviews"),
        [row(&["ACTIVE", "Y"])]
    );
    assert_eq!(
        db.rows("SELECT OBJECT_NAME, OBJECT_TYPE FROM qsys2/sysviewdep WHERE VIEW_NAME = 'ACTIVE'"),
        [row(&["CUST", "TABLE"])]
    );
    assert_eq!(
        db.rows("SELECT TABLE_TYPE FROM qsys2/systables WHERE TABLE_NAME = 'ACTIVE'"),
        [row(&["V"])]
    );
    db.ok("CREATE VIEW bigs AS SELECT * FROM cust WHERE bal BETWEEN 100 AND 1000 AND name IN ('Carol', 'Zed')");
    assert_eq!(db.rows("SELECT name FROM bigs"), [row(&["Carol"])]);
    assert_eq!(
        db.code("CREATE VIEW bad AS SELECT c.cusno FROM cust c JOIN orders o ON c.cusno = o.cusno"),
        -84
    );
    assert_eq!(
        db.code("CREATE VIEW bad AS SELECT cusno + 1 FROM cust"),
        -84
    );
    assert_eq!(
        db.code("CREATE VIEW nosel AS SELECT cusno FROM cust WHERE bal > 0"),
        -84
    );
    db.ok("DROP VIEW active");
    assert_eq!(db.code("SELECT * FROM active"), -204);
}

#[test]
fn cursors_and_sqlca() {
    let mut db = Db::new();
    setup(&mut db);
    // The SQLCA on success, no rows, and an error.
    let (ca, _) = db.run("SELECT * FROM cust WHERE cusno = 1");
    assert_eq!(
        (ca.sqlcode, ca.sqlstate.as_str(), ca.sqlerrd[2]),
        (0, "00000", 1)
    );
    let (ca, _) = db.run("SELECT * FROM cust WHERE cusno = 99");
    assert_eq!(
        (ca.sqlcode, ca.sqlstate.as_str(), ca.msgid().as_str()),
        (100, "02000", "SQL0100")
    );
    let (ca, _) = db.run("UPDATE cust SET bal = 1 WHERE cusno = 99");
    assert_eq!(ca.sqlcode, 100);
    let (ca, _) = db.run("UPDATE cust SET bal = bal WHERE cusno <= 2");
    assert_eq!((ca.sqlcode, ca.sqlerrd[2]), (0, 2));
    let (ca, o) = db.run("SELECT nosuch FROM cust");
    assert!(ca.is_error() && o.is_none());
    assert_eq!(
        (ca.sqlcode, ca.sqlstate.as_str(), ca.msgid().as_str()),
        (-206, "42703", "SQL0206")
    );
    assert!(ca.sqlerrmc.contains("NOSUCH"));
    assert!(ca.line().starts_with("SQL0206 SQLCODE -206 SQLSTATE 42703"));
    // A forward cursor.
    db.ok("DECLARE c1 CURSOR FOR SELECT cusno, name FROM cust ORDER BY cusno");
    assert_eq!(db.code("FETCH c1"), -501);
    db.ok("OPEN c1");
    assert_eq!(db.code("OPEN c1"), -501);
    assert_eq!(db.rows("FETCH c1"), [row(&["1", "Alice"])]);
    assert_eq!(db.rows("FETCH NEXT FROM c1"), [row(&["2", "Bob"])]);
    assert_eq!(db.code("FETCH PRIOR FROM c1"), -501);
    db.rows("FETCH c1");
    db.rows("FETCH c1");
    let (ca, o) = db.run("FETCH c1");
    assert_eq!(ca.sqlcode, 100);
    assert!(matches!(o, Some(Outcome::Rows(rs)) if rs.rows.is_empty() && rs.columns.len() == 2));
    db.ok("CLOSE c1");
    assert_eq!(db.code("CLOSE c1"), -501);
    // A scroll cursor.
    db.ok("DECLARE c2 SCROLL CURSOR WITH HOLD FOR SELECT cusno FROM cust ORDER BY cusno");
    db.ok("OPEN c2");
    assert_eq!(db.rows("FETCH LAST FROM c2"), [row(&["4"])]);
    assert_eq!(db.rows("FETCH PRIOR FROM c2"), [row(&["3"])]);
    assert_eq!(db.rows("FETCH ABSOLUTE 2 FROM c2"), [row(&["2"])]);
    assert_eq!(db.rows("FETCH RELATIVE -1 FROM c2"), [row(&["1"])]);
    assert_eq!(db.rows("FETCH ABSOLUTE -1 FROM c2"), [row(&["4"])]);
    assert_eq!(db.code("FETCH NEXT FROM c2"), 100);
    assert_eq!(db.rows("FETCH FIRST FROM c2"), [row(&["1"])]);
    // COMMIT closes cursors without HOLD and keeps those with it.
    db.ok("DECLARE c3 CURSOR FOR SELECT cusno FROM cust");
    db.ok("OPEN c3");
    db.ok("COMMIT");
    assert_eq!(db.code("FETCH c3"), -501);
    assert_eq!(db.rows("FETCH NEXT FROM c2"), [row(&["2"])]);
    db.ok("CLOSE c2");
    // Positioned update and delete.
    db.ok("DECLARE c4 CURSOR FOR SELECT cusno, name FROM cust WHERE bal = 0 ORDER BY cusno FOR UPDATE");
    db.ok("OPEN c4");
    assert_eq!(db.code("UPDATE cust SET bal = 5 WHERE CURRENT OF c4"), -501);
    assert_eq!(db.rows("FETCH c4"), [row(&["2", "Bob"])]);
    assert_eq!(
        db.ok("UPDATE cust SET bal = 5, note = 'positioned' WHERE CURRENT OF c4"),
        Outcome::Count(1)
    );
    assert_eq!(db.rows("FETCH c4"), [row(&["4", "Dave"])]);
    assert_eq!(
        db.ok("DELETE FROM cust WHERE CURRENT OF c4"),
        Outcome::Count(1)
    );
    assert_eq!(db.code("DELETE FROM orders WHERE CURRENT OF c4"), -501);
    db.ok("CLOSE c4");
    assert_eq!(
        db.rows("SELECT cusno, bal, note FROM cust ORDER BY cusno"),
        [
            row(&["1", "10.50", "-"]),
            row(&["2", "5.00", "positioned"]),
            row(&["3", "250.00", "-"])
        ]
    );
    // A cursor over a join is not updatable.
    db.ok("DECLARE c5 CURSOR FOR SELECT c.cusno FROM cust c JOIN orders o ON c.cusno = o.cusno");
    db.ok("OPEN c5");
    db.ok("INSERT INTO orders VALUES (1, 1, 5)");
    db.ok("CLOSE c5");
    db.ok("OPEN c5");
    db.rows("FETCH c5");
    assert_eq!(db.code("DELETE FROM cust WHERE CURRENT OF c5"), -501);
    // MERGE.
    db.ok("CREATE TABLE qstest/newcust (cusno DECIMAL(7, 0) NOT NULL, name CHAR(30) NOT NULL, bal NUMERIC(9, 2) NOT NULL)");
    db.ok("INSERT INTO newcust VALUES (1, 'Alice', -1), (2, 'Bob', 77), (9, 'Zed', 3)");
    assert_eq!(
        db.ok("MERGE INTO cust c USING (SELECT * FROM newcust) n ON c.cusno = n.cusno WHEN MATCHED AND n.bal < 0 THEN DELETE WHEN MATCHED THEN UPDATE SET bal = n.bal WHEN NOT MATCHED THEN INSERT (cusno, name, bal) VALUES (n.cusno, n.name, n.bal)"),
        Outcome::Count(3)
    );
    assert_eq!(
        db.rows("SELECT cusno, name, bal FROM cust ORDER BY cusno"),
        [
            row(&["2", "Bob", "77.00"]),
            row(&["3", "Carol", "250.00"]),
            row(&["9", "Zed", "3.00"])
        ]
    );
}

#[test]
fn statement_files() {
    let mut db = Db::new();
    let script = "
        -- a comment
        CREATE TABLE qstest/t1 (k INT NOT NULL PRIMARY KEY, v CHAR(5));
        INSERT INTO t1 VALUES (1, 'a'), (2, 'b');
        SELECT * FROM t1 ORDER BY k;
        INSERT INTO t1 VALUES (1, 'dup');
        UPDATE t1 SET v = 'c' WHERE k = 2;
    ";
    let opts = RunOpts {
        errlvl: 30,
        show_rows: 10,
        ..RunOpts::default()
    };
    let report = runsqlstm(&mut db.store, &mut db.ctx, script, &opts).unwrap();
    assert_eq!(
        (report.statements, report.errors, report.severity),
        (5, 1, 30)
    );
    assert!(
        report.listing.contains("CREATE TABLE: SQLCODE 0"),
        "{}",
        report.listing
    );
    assert!(report.listing.contains("INSERT: SQLCODE 0  rows 2"));
    assert!(report.listing.contains("K  V\n1  a\n2  b"));
    assert!(
        report
            .listing
            .contains("INSERT: SQL0803 SQLCODE -803 SQLSTATE 23505")
    );
    assert!(report.listing.contains("UPDATE: SQLCODE 0  rows 1"));
    assert!(!report.listing.contains("Run ended"));
    assert_eq!(db.rows("SELECT v FROM t1 WHERE k = 2"), [row(&["c"])]);
    // ERRLVL(0): the first error stops the run.
    let report = runsqlstm(&mut db.store, &mut db.ctx, "INSERT INTO t1 VALUES (3, 'x'); INSERT INTO t1 VALUES (3, 'y'); INSERT INTO t1 VALUES (4, 'z');", &RunOpts::default()).unwrap();
    assert_eq!(
        (report.statements, report.errors, report.severity),
        (2, 1, 40)
    );
    assert!(
        report
            .listing
            .contains("Run ended: the error exceeds ERRLVL")
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM t1"), [row(&["3"])]);
    // A syntax error in the file is reported before anything runs.
    let report = runsqlstm(&mut db.store, &mut db.ctx, "SELEC 1", &RunOpts::default()).unwrap();
    assert_eq!(
        (report.statements, report.errors, report.severity),
        (0, 1, 40)
    );
    assert!(report.listing.starts_with("SQL0104"));
    // COMMIT(*CHG) over a journaled table: a stopping error rolls the run back.
    let mut t = db.store.begin().unwrap();
    let lib = db.ctx.names.curlib.unwrap();
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        lib,
        Name::parse("T1RCV").unwrap(),
        &jrn::rcv::CrtJrnRcvOpts::default(),
    )
    .unwrap();
    let jid = jrn::journal::crtjrn(
        &mut t,
        lib,
        Name::parse("T1JRN").unwrap(),
        rcv,
        &jrn::journal::CrtJrnOpts::default(),
    )
    .unwrap();
    let t1 = db
        .ctx
        .resolve_file(&mut t, &sql::ast::ObjectName::new("T1"))
        .unwrap();
    db::journal::strjrnpf(&mut t, JobHandle(1), t1, jid, false, true, 1).unwrap();
    t.commit().unwrap();
    let chg = RunOpts {
        commit: CommitOpt::Chg,
        ..RunOpts::default()
    };
    let report = runsqlstm(
        &mut db.store,
        &mut db.ctx,
        "INSERT INTO t1 VALUES (5, 'e'); INSERT INTO t1 VALUES (5, 'again');",
        &chg,
    )
    .unwrap();
    assert!(report.listing.ends_with("ROLLBACK\n"), "{}", report.listing);
    assert_eq!(db.rows("SELECT COUNT(*) FROM t1"), [row(&["3"])]);
    let report = runsqlstm(
        &mut db.store,
        &mut db.ctx,
        "INSERT INTO t1 VALUES (5, 'e'); DELETE FROM t1 WHERE k = 1;",
        &chg,
    )
    .unwrap();
    assert!(report.listing.ends_with("COMMIT\n"), "{}", report.listing);
    assert_eq!(
        db.rows("SELECT k FROM t1 ORDER BY k"),
        [row(&["2"]), row(&["3"]), row(&["5"])]
    );
    // DFTRDBCOL names the schema for unqualified names under *SQL.
    let sqln = RunOpts {
        naming: Naming::Sql,
        dftrdbcol: Some("QSTEST".into()),
        show_rows: 5,
        ..RunOpts::default()
    };
    let report = runsqlstm(
        &mut db.store,
        &mut db.ctx,
        "SELECT k FROM t1 WHERE k = 5",
        &sqln,
    )
    .unwrap();
    assert!(
        report.listing.contains("SELECT: SQLCODE 0  rows 1"),
        "{}",
        report.listing
    );
    let _ = Value::Null;
}

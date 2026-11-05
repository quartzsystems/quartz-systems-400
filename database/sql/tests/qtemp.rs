// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QTEMP for SQL: declared temporary tables and objects in QTEMP
//! (`tools/conformance/sql/cases/ddl/DGTTOPT`, `QTEMPOBJ`).

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(16384, 11));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: 1_767_225_600,
        };
        let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        let lib = ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse("QSTEST").unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: String::new(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: 1_767_225_600,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
        ctx.now = Some(1_767_225_600);
        ctx.user = "QSECOFR".into();
        Self { store, ctx }
    }

    fn run(&mut self, text: &str) -> Result<Outcome, SqlError> {
        let p = prepare(text, Naming::Sql)?;
        let mut t = self.store.begin().unwrap();
        let r = p.execute(&mut t, &mut self.ctx, &[]);
        match &r {
            Ok(_) => t.commit().unwrap(),
            Err(_) => t.abort(),
        }
        r
    }

    fn ok(&mut self, text: &str) -> Outcome {
        self.run(text).unwrap_or_else(|e| panic!("{text}: {e} ({})", e.sqlcode()))
    }

    fn rows(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs.rows.iter().map(|r| r.iter().map(|v| v.to_string().trim_end().to_string()).collect::<Vec<_>>().join("|")).collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, text: &str) -> (i32, String) {
        match self.run(text) {
            Err(e) => (e.sqlcode(), e.sqlstate().to_string()),
            Ok(o) => panic!("{text} succeeded: {o:?}"),
        }
    }
}


fn under_commit(db: &mut Db) {
    let mut t = db.store.begin().unwrap();
    db.ctx.session.strcmtctl(&mut t, db::cmt::LckLvl::Chg, None, 1_767_225_600).unwrap();
    t.commit().unwrap();
}

#[test]
fn declared_tables_live_in_the_jobs_qtemp() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST.SRC (K INT NOT NULL, V CHAR(3))");
    db.ok("INSERT INTO QSTEST.SRC VALUES (1, 'a'), (2, 'b')");
    db.ok("DECLARE GLOBAL TEMPORARY TABLE T1 AS (SELECT * FROM QSTEST.SRC) WITH DATA ON COMMIT PRESERVE ROWS");
    assert_eq!(db.rows("SELECT K, V FROM SESSION.T1 ORDER BY K"), ["1|a", "2|b"]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM QTEMP.T1"), ["2"]);
    // Mixed: QTEMP and the permanent library in one statement.
    db.ok("INSERT INTO QSTEST.SRC SELECT K + 10, V FROM SESSION.T1");
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSTEST.SRC"), ["4"]);
    // Neither the catalog nor the library list of QSYS shows it.
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QTEMP'"), ["0"]);
    assert_eq!(db.code("DECLARE GLOBAL TEMPORARY TABLE QTEMP.T2 (A INT)"), (-79, "58020".into()));
    assert_eq!(db.code("DECLARE GLOBAL TEMPORARY TABLE T1 (A INT)"), (-601, "42710".into()));
    assert_eq!(db.code("DECLARE GLOBAL TEMPORARY TABLE T3 (A INT PRIMARY KEY)"), (-526, "42995".into()));
    assert_eq!(db.code("DECLARE GLOBAL TEMPORARY TABLE T3 (A INT) LOGGED"), (-199, "42601".into()));
    db.ok("DECLARE GLOBAL TEMPORARY TABLE T1 (Z INT) WITH REPLACE");
    assert!(db.rows("SELECT * FROM SESSION.T1").is_empty());
    assert_eq!(db.code("LABEL ON TABLE SESSION.T1 IS 'x'"), (-526, "42995".into()));
    db.ok("DROP TABLE SESSION.T1");
    assert_eq!(db.code("SELECT * FROM SESSION.T1"), (-204, "42704".into()));
}

#[test]
fn objects_in_qtemp() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QTEMP.P (K INT NOT NULL)");
    db.ok("INSERT INTO QTEMP.P VALUES (3), (1)");
    db.ok("CREATE INDEX QTEMP.PIX ON QTEMP.P (K)");
    db.ok("CREATE VIEW QTEMP.PV AS SELECT K * 2 AS K2 FROM QTEMP.P");
    assert_eq!(db.rows("SELECT K2 FROM QTEMP.PV ORDER BY K2"), ["2", "6"]);
    // SESSION finds only what a plain CREATE TABLE did not make.
    assert_eq!(db.code("SELECT * FROM SESSION.P"), (-204, "42704".into()));
    assert_eq!(db.code("CREATE TABLE SESSION.X (A INT)"), (-204, "42704".into()));
    assert_eq!(db.code("CREATE VIEW QSTEST.BAD AS SELECT * FROM QTEMP.P"), (-7008, "55019".into()));
    assert_eq!(db.code("CREATE INDEX QSTEST.BADIX ON QTEMP.P (K)"), (-156, "42809".into()));
    assert_eq!(db.code("CREATE SEQUENCE QTEMP.S"), (-707, "42939".into()));
    assert_eq!(db.code("CREATE TABLE QTEMP.P (A INT)"), (-601, "42710".into()));
    db.ok("DROP TABLE QTEMP.P");
    assert_eq!(db.code("SELECT * FROM QTEMP.PV"), (-204, "42704".into()));
}

#[test]
fn on_commit_and_rollback() {
    let mut db = Db::new();
    under_commit(&mut db);
    db.ok("DECLARE GLOBAL TEMPORARY TABLE DEL (A INT)");
    db.ok("DECLARE GLOBAL TEMPORARY TABLE KEEP (A INT) ON COMMIT PRESERVE ROWS");
    db.ok("DECLARE GLOBAL TEMPORARY TABLE NL (A INT) ON COMMIT PRESERVE ROWS NOT LOGGED");
    db.ok("INSERT INTO SESSION.DEL VALUES (1)");
    db.ok("INSERT INTO SESSION.KEEP VALUES (1)");
    db.ok("INSERT INTO SESSION.NL VALUES (1)");
    db.ok("COMMIT");
    assert_eq!(db.rows("SELECT COUNT(*) FROM SESSION.DEL"), ["0"]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM SESSION.KEEP"), ["1"]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM SESSION.NL"), ["1"]);
    // A logged table's change is rolled back; a NOT LOGGED one changed in
    // the unit is cleared (ON ROLLBACK DELETE ROWS).
    db.ok("INSERT INTO SESSION.KEEP VALUES (2)");
    db.ok("INSERT INTO SESSION.NL VALUES (2)");
    db.ok("ROLLBACK");
    assert_eq!(db.rows("SELECT A FROM SESSION.KEEP"), ["1"]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM SESSION.NL"), ["0"]);
}

#[test]
fn the_jobs_end_and_the_next_start_drop_it() {
    let mut db = Db::new();
    db.ok("DECLARE GLOBAL TEMPORARY TABLE T (A INT) ON COMMIT PRESERVE ROWS");
    db.ok("CREATE TABLE QTEMP.U (A INT)");
    db.ok("CREATE VIEW QTEMP.UV AS SELECT * FROM QTEMP.U");
    let mut t = db.store.begin().unwrap();
    let lib = db.ctx.qtemp_library(&mut t).unwrap();
    let qsys = db.ctx.qsys;
    // A second job's QTEMP is its own.
    let mut other = SqlContext::new(qsys, JobHandle(2), NameContext::default());
    other.now = Some(1_767_225_600);
    assert!(other.resolve_file(&mut t, &sql::ast::ObjectName { schema: Some(sql::ast::Ident::new("QTEMP")), name: sql::ast::Ident::new("U") }).is_err());
    let second = other.make_qtemp(&mut t).unwrap();
    assert_ne!(lib, second);
    t.commit().unwrap();
    let mut t = db.store.begin().unwrap();
    sql::exec::qtemp::drop_library(&mut t, qsys, JobHandle(1), db.ctx.qtemp.take().unwrap()).unwrap();
    assert!(ops::read_header(&mut t, lib).is_err());
    // What a stopped system left.
    assert_eq!(ops::temporary_roots(&mut t).unwrap(), [second]);
    assert_eq!(sql::exec::qtemp::purge(&mut t, qsys, JobHandle(0)).unwrap(), 1);
    assert!(ops::temporary_roots(&mut t).unwrap().is_empty());
    t.commit().unwrap();
}

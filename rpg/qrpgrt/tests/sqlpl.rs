// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL procedures end to end: `CREATE PROCEDURE` compiles the body to a
//! `*PGM` in the store; `CALL` from a job runs it, its statements going
//! back to the database through the embedded SQL runtime.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ile::{PgmKind, ProgramObject};
use mi::act::ObjectSource;
use qsname::Name;
use sql::embed::{Desc, Op, Reply, Request, Session};
use sql::{Naming, SqlContext, Value};
use store::obj::library::Library;
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

/// Programs from the store.
struct StoreSource {
    store: Rc<RefCell<Store>>,
    lib: ObjectId,
}

impl ObjectSource for StoreSource {
    fn program(&mut self, name: &str, _lib: &str, kind: PgmKind) -> Option<(Arc<ProgramObject>, String)> {
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().ok()?;
        let ty = if kind == PgmKind::Pgm { types::PGM } else { types::SRVPGM };
        let id = Library::load(&mut txn, self.lib).ok()?.lookup(&mut txn, ty, Name::parse(name).ok()?).ok()??;
        let p = ile::store::load_program(&mut txn, id).ok()?;
        txn.abort();
        Some((Arc::new(p), "QSTEST".into()))
    }
}

struct Env {
    job: Box<mi::Job>,
}

fn env() -> Env {
    let d = Arc::new(MemBlockDev::with_seed(65536, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
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
    let store = Rc::new(RefCell::new(store));
    let names = NameContext { curlib: Some(lib), libl: vec![lib] };
    let mut ctx = SqlContext::new(qsys, JobHandle(1), names);
    ctx.now = Some(NOW);
    ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
    let session = Session::with_ctx(Box::new(Rc::clone(&store)), ctx);
    let mut job = mi::Job::boxed(Box::new(StoreSource { store, lib }));
    qrpgrt::install(&mut job);
    job.sql = Some(Box::new(session));
    Env { job }
}

impl Env {
    /// One statement from the job's command line, `*SQL` naming.
    fn run(&mut self, text: &str) -> Reply {
        self.run_with(text, Vec::new()).0
    }

    fn run_with(&mut self, text: &str, inputs: Vec<Value>) -> (Reply, Vec<(usize, Value)>) {
        let req = Request {
            scope: "1:TEST".into(),
            desc: Desc { op: Op::Exec, sql_naming: true, text: text.into(), ..Desc::default() },
            inputs,
        };
        qrpgrt::sqlcall::run_request(&mut self.job, req)
    }

    fn ok(&mut self, text: &str) -> Reply {
        let r = self.run(text);
        assert!(r.sqlca.sqlcode >= 0, "{text}: {:?}", r.sqlca);
        r
    }

    /// A query's rows.
    fn rows(&mut self, text: &str) -> Vec<Vec<Value>> {
        let req = Request {
            scope: "1:TEST".into(),
            desc: Desc { op: Op::Open, name: "Q".into(), sql_naming: true, text: text.into(), ..Desc::default() },
            inputs: Vec::new(),
        };
        let s = self.job.sql.as_mut().unwrap();
        let r = s.run(&req);
        assert!(!r.sqlca.is_error(), "{text}: {:?}", r.sqlca);
        let mut out = Vec::new();
        loop {
            let f = s.run(&Request {
                scope: "1:TEST".into(),
                desc: Desc { op: Op::Fetch, name: "Q".into(), sql_naming: true, ..Desc::default() },
                inputs: Vec::new(),
            });
            if f.sqlca.sqlcode != 0 {
                break;
            }
            out.extend(f.rows);
        }
        s.run(&Request {
            scope: "1:TEST".into(),
            desc: Desc { op: Op::Close, name: "Q".into(), sql_naming: true, ..Desc::default() },
            inputs: Vec::new(),
        });
        out
    }
}

fn text(v: &Value) -> String {
    v.as_text().trim_end().to_string()
}

#[test]
fn a_procedure_changes_a_table() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.T (ID INTEGER NOT NULL, NAME VARCHAR(20), AMT DECIMAL(9, 2))");
    e.ok(
        "CREATE PROCEDURE QSTEST.ADD_ROW (IN P_ID INTEGER, IN P_NAME VARCHAR(20))
         LANGUAGE SQL
         BEGIN
           DECLARE V_AMT DECIMAL(9, 2) DEFAULT 1.50;
           SET V_AMT = V_AMT * P_ID;
           INSERT INTO QSTEST.T VALUES (P_ID, P_NAME, V_AMT);
         END",
    );
    e.ok("CALL QSTEST.ADD_ROW(2, 'two')");
    e.ok("CALL QSTEST.ADD_ROW(3, 'three')");
    let rows = e.rows("SELECT ID, NAME, AMT FROM QSTEST.T ORDER BY ID");
    let got: Vec<String> = rows.iter().map(|r| r.iter().map(text).collect::<Vec<_>>().join(",")).collect();
    assert_eq!(got, ["2,two,3.00", "3,three,4.50"]);
}

#[test]
fn control_flow_and_out_parameters() {
    let mut e = env();
    e.ok(
        "CREATE PROCEDURE QSTEST.FACT (IN N INTEGER, OUT F BIGINT, OUT MSG VARCHAR(30))
         LANGUAGE SQL
         BEGIN
           DECLARE I INTEGER DEFAULT 1;
           SET F = 1;
           WHILE I <= N DO
             SET F = F * I;
             SET I = I + 1;
           END WHILE;
           IF F > 100 THEN
             SET MSG = 'big';
           ELSEIF F > 5 THEN
             SET MSG = 'medium';
           ELSE
             SET MSG = 'small';
           END IF;
         END",
    );
    let (r, outs) = e.run_with("CALL QSTEST.FACT(5, ?, ?)", vec![Value::Null, Value::Null]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let outs: Vec<String> = outs.iter().map(|(_, v)| text(v)).collect();
    assert_eq!(outs, ["120", "big"]);
    let (_, outs) = e.run_with("CALL QSTEST.FACT(3, ?, ?)", vec![Value::Null, Value::Null]);
    assert_eq!(outs.iter().map(|(_, v)| text(v)).collect::<Vec<_>>(), ["6", "medium"]);
}

#[test]
fn handlers_and_signals() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.K (ID INTEGER NOT NULL PRIMARY KEY)");
    e.ok(
        "CREATE PROCEDURE QSTEST.ADDK (IN P INTEGER, OUT DUPS INTEGER)
         LANGUAGE SQL
         BEGIN
           DECLARE CONTINUE HANDLER FOR SQLSTATE '23505' SET DUPS = DUPS + 1;
           SET DUPS = 0;
           INSERT INTO QSTEST.K VALUES (P);
           INSERT INTO QSTEST.K VALUES (P);
           INSERT INTO QSTEST.K VALUES (P);
         END",
    );
    let (r, outs) = e.run_with("CALL QSTEST.ADDK(7, ?)", vec![Value::Null]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(text(&outs[0].1), "2");
    e.ok(
        "CREATE PROCEDURE QSTEST.FAILS (IN P INTEGER)
         LANGUAGE SQL
         BEGIN
           IF P < 0 THEN
             SIGNAL SQLSTATE '75001' SET MESSAGE_TEXT = 'negative';
           END IF;
         END",
    );
    e.ok("CALL QSTEST.FAILS(1)");
    let r = e.run("CALL QSTEST.FAILS(-1)");
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (-438, "75001"), "{:?}", r.sqlca);
    assert_eq!(r.sqlca.sqlerrmc, "negative");
    // An exception no handler takes ends the procedure with it.
    let r = e.run("CALL QSTEST.ADDK(1, 2)");
    assert_eq!(r.sqlca.sqlcode, -469, "{:?}", r.sqlca);
}

#[test]
fn result_sets_go_to_the_caller() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.R (ID INTEGER NOT NULL, NAME VARCHAR(10))");
    e.ok("INSERT INTO QSTEST.R VALUES (1, 'a'), (2, 'b'), (3, 'c')");
    e.ok(
        "CREATE PROCEDURE QSTEST.LIST (IN P_MIN INTEGER)
         LANGUAGE SQL DYNAMIC RESULT SETS 1
         BEGIN
           DECLARE C1 CURSOR WITH RETURN FOR SELECT ID, NAME FROM QSTEST.R WHERE ID >= P_MIN ORDER BY ID;
           OPEN C1;
         END",
    );
    let r = e.run("CALL QSTEST.LIST(2)");
    assert_eq!(r.sqlca.sqlcode, 466, "{:?}", r.sqlca);
    assert_eq!(text(&r.rows[0][0]), "RS#1");
    let s = e.job.sql.as_mut().unwrap();
    let mut got = Vec::new();
    loop {
        let f = s.run(&Request {
            scope: "1:TEST".into(),
            desc: Desc { op: Op::Fetch, name: "RS#1".into(), sql_naming: true, ..Desc::default() },
            inputs: Vec::new(),
        });
        if f.sqlca.sqlcode != 0 {
            break;
        }
        got.push(text(&f.rows[0][1]));
    }
    assert_eq!(got, ["b", "c"]);
}

#[test]
fn loops_cursors_and_catalogs() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.S (N INTEGER NOT NULL)");
    e.ok("INSERT INTO QSTEST.S VALUES (1), (2), (3), (4)");
    e.ok(
        "CREATE PROCEDURE QSTEST.SUMS (OUT TOTAL INTEGER, OUT EVENS INTEGER, OUT FETCHED INTEGER)
         LANGUAGE SQL
         BEGIN
           DECLARE V INTEGER;
           DECLARE AT_END INTEGER DEFAULT 0;
           DECLARE C CURSOR FOR SELECT N FROM QSTEST.S ORDER BY N;
           DECLARE CONTINUE HANDLER FOR NOT FOUND SET AT_END = 1;
           SET (TOTAL, EVENS, FETCHED) = (0, 0, 0);
           FOR R AS SELECT N AS X FROM QSTEST.S DO
             SET TOTAL = TOTAL + X;
           END FOR;
           OPEN C;
           L1: LOOP
             FETCH C INTO V;
             IF AT_END = 1 THEN
               LEAVE L1;
             END IF;
             SET FETCHED = FETCHED + 1;
             CASE MOD(V, 2)
               WHEN 0 THEN SET EVENS = EVENS + 1;
               ELSE ITERATE L1;
             END CASE;
           END LOOP L1;
           CLOSE C;
         END",
    );
    let (r, outs) = e.run_with("CALL QSTEST.SUMS(?, ?, ?)", vec![Value::Null, Value::Null, Value::Null]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(outs.iter().map(|(_, v)| text(v)).collect::<Vec<_>>(), ["10", "2", "4"]);
    let rows = e.rows("SELECT ROUTINE_NAME, ROUTINE_TYPE, EXTERNAL_NAME, OUT_PARMS FROM QSYS2.SYSROUTINES WHERE ROUTINE_SCHEMA = 'QSTEST'");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(text(&rows[0][2]), "QSTEST/SUMS");
    e.ok("DROP PROCEDURE QSTEST.SUMS");
    let r = e.run("CALL QSTEST.SUMS(?, ?, ?)");
    assert_eq!(r.sqlca.sqlcode, -204, "{:?}", r.sqlca);
    let _ = Naming::Sql;
}

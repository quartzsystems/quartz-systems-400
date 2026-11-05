// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL triggers end to end: `CREATE TRIGGER` compiles the body to a
//! `*PGM` over the trigger buffer; an SQL change calls it in the middle
//! of the statement through the context's mid caller (in the storage
//! service, the job's answer resumes the statement's fiber; here a
//! second job of the same store makes the call), and a record write
//! calls it as any trigger program. Outcomes are the primary's
//! (`tools/conformance/sql/oracle/trigger/p5.out`, `p6.out`).

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ile::{PgmKind, ProgramObject};
use mi::act::ObjectSource;
use mi::io::trigger::{TriggerCall, call};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::exec::routine::{CallPlan, midcall};
use sql::{SqlContext, Value};
use store::obj::library::Library;
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

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
    /// The session whose statements fire triggers.
    sql: Session,
    /// The job the trigger programs run in.
    job: Rc<RefCell<Box<mi::Job>>>,
    store: Rc<RefCell<Store>>,
    lib: ObjectId,
}

fn context(qsys: ObjectId, lib: ObjectId, locks: &db::odp::SharedLocks) -> SqlContext {
    let mut ctx = SqlContext::with_locks(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] }, locks.clone());
    ctx.now = Some(NOW);
    ctx.user = "QSECOFR".into();
    ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
    ctx
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
    let locks = db::odp::new_shared();
    let mut job = mi::Job::boxed(Box::new(StoreSource { store: Rc::clone(&store), lib }));
    qrpgrt::install(&mut job);
    job.sql = Some(Box::new(Session::with_ctx(Box::new(Rc::clone(&store)), context(qsys, lib, &locks))));
    let job = Rc::new(RefCell::new(job));
    // The job's side of a call in the middle of a statement.
    let j = Rc::clone(&job);
    let mid: midcall::MidCaller = Rc::new(move |plan: CallPlan| {
        let plan = CallPlan::decode(&plan.encode()).expect("the plan crosses");
        let mut c = TriggerCall {
            trigger: plan.routine.clone(),
            lib: plan.library.clone(),
            pgm: plan.program.clone(),
            buffer: midcall::plan_buffer(&plan),
            ..TriggerCall::default()
        };
        let failed = call(&mut j.borrow_mut(), &mut c).err().map(|f| db::trg::Failure { msgid: f.msgid, text: f.text, not_found: f.not_found });
        Some(midcall::trigger_inputs(&c.buffer, failed.as_ref()))
    });
    let mut ctx = context(qsys, lib, &locks);
    ctx.midcall = Some(mid);
    Env { sql: Session::with_ctx(Box::new(Rc::clone(&store)), ctx), job, store, lib }
}

fn text(v: &Value) -> String {
    v.as_text().trim_end().to_string()
}

impl Env {
    fn run(&mut self, text: &str) -> Reply {
        self.sql.run(&Request {
            scope: "1:TEST".into(),
            desc: Desc { op: Op::Exec, sql_naming: true, text: text.into(), ..Desc::default() },
            inputs: Vec::new(),
        })
    }

    fn ok(&mut self, text: &str) -> Reply {
        let r = self.run(text);
        assert!(r.sqlca.sqlcode >= 0, "{text}: {:?}", r.sqlca);
        r
    }

    fn code(&mut self, text: &str) -> (i32, String) {
        let r = self.run(text);
        (r.sqlca.sqlcode, r.sqlca.sqlstate)
    }

    fn rows(&mut self, q: &str) -> Vec<String> {
        let s = &mut self.sql;
        let req = |op, text: &str| Request {
            scope: "1:TEST".into(),
            desc: Desc { op, name: "Q".into(), sql_naming: true, text: text.into(), ..Desc::default() },
            inputs: Vec::new(),
        };
        let r = s.run(&req(Op::Open, q));
        assert!(!r.sqlca.is_error(), "{q}: {:?}", r.sqlca);
        let mut out = Vec::new();
        loop {
            let f = s.run(&req(Op::Fetch, ""));
            if f.sqlca.sqlcode != 0 {
                break;
            }
            out.extend(f.rows.iter().map(|r| r.iter().map(text).collect::<Vec<_>>().join("|")));
        }
        s.run(&req(Op::Close, ""));
        out
    }

    fn program_exists(&self, name: &str) -> bool {
        let mut st = self.store.borrow_mut();
        let mut t = st.begin().unwrap();
        let found = Library::load(&mut t, self.lib).unwrap().lookup(&mut t, types::PGM, Name::parse(name).unwrap()).unwrap().is_some();
        t.abort();
        found
    }
}

#[test]
fn sql_changes_fire_sql_triggers_as_the_primary() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.STRG1 (ID INT NOT NULL, NAME CHAR(10), AMT DECIMAL(7, 2), D DATE)");
    e.ok("CREATE TABLE QSTEST.STRGLOG (TXT VARCHAR(100), N INT)");
    e.ok("CREATE TRIGGER QSTEST.TRB1 BEFORE INSERT ON QSTEST.STRG1 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW BEGIN ATOMIC SET N.NAME = UPPER(N.NAME); SET N.AMT = COALESCE(N.AMT, 0) + 1; END");
    e.ok("CREATE TRIGGER QSTEST.TRIGGER_WITH_A_LONG_NAME_A1 AFTER INSERT ON QSTEST.STRG1 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW INSERT INTO QSTEST.STRGLOG VALUES ('ROW ' CONCAT N.NAME, (SELECT COUNT(*) FROM QSTEST.STRG1))");
    e.ok("CREATE TRIGGER QSTEST.TRA2 AFTER INSERT ON QSTEST.STRG1 REFERENCING NEW AS N FOR EACH ROW INSERT INTO QSTEST.STRGLOG VALUES ('SQL ' CONCAT N.NAME, (SELECT COUNT(*) FROM QSTEST.STRG1))");
    e.ok("CREATE TRIGGER QSTEST.TRB4 BEFORE UPDATE OF AMT ON QSTEST.STRG1 REFERENCING OLD AS O NEW AS N FOR EACH ROW MODE DB2ROW WHEN (N.AMT < 0) SIGNAL SQLSTATE '75001' SET MESSAGE_TEXT = 'negative amount'");
    e.ok("CREATE TRIGGER QSTEST.TRB5 BEFORE DELETE ON QSTEST.STRG1 REFERENCING OLD AS O FOR EACH ROW MODE DB2ROW WHEN (O.ID = 99) BEGIN DECLARE X INT; SET X = 1 / (O.ID - 99); END");
    assert!(e.program_exists("TRB1") && e.program_exists("TRIGG00001"), "programs named as the primary names them");
    e.ok("INSERT INTO QSTEST.STRG1 VALUES (1, 'abc', 5, '2026-01-01'), (2, 'def', NULL, NULL), (3, 'ghi', 1, NULL)");
    assert_eq!(e.rows("SELECT ID, NAME, AMT, D FROM QSTEST.STRG1 ORDER BY ID"), ["1|ABC|6.00|2026-01-01", "2|DEF|1.00|NULL", "3|GHI|2.00|NULL"]);
    // DB2ROW after triggers see each row as it comes; DB2SQL ones see them all.
    assert_eq!(
        e.rows("SELECT TXT, N FROM QSTEST.STRGLOG"),
        ["ROW ABC|1", "ROW DEF|2", "ROW GHI|3", "SQL ABC|3", "SQL DEF|3", "SQL GHI|3"]
    );
    let r = e.run("UPDATE QSTEST.STRG1 SET AMT = -1 WHERE ID = 1");
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (-723, "09000"), "{:?}", r.sqlca);
    assert!(r.sqlca.sqlerrmc.contains("TRB4") && r.sqlca.sqlerrmc.contains("-438") && r.sqlca.sqlerrmc.contains("75001"), "{}", r.sqlca.sqlerrmc);
    e.ok("UPDATE QSTEST.STRG1 SET NAME = 'X' WHERE ID = 1");
    e.ok("INSERT INTO QSTEST.STRG1 VALUES (99, 'zz', 0, NULL)");
    let r = e.run("DELETE FROM QSTEST.STRG1 WHERE ID = 99");
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (-723, "09000"), "{:?}", r.sqlca);
    assert!(r.sqlca.sqlerrmc.contains("-802"), "{}", r.sqlca.sqlerrmc);
    assert_eq!(e.rows("SELECT ID, NAME, AMT FROM QSTEST.STRG1 ORDER BY ID"), ["1|X|6.00", "2|DEF|1.00", "3|GHI|2.00", "99|ZZ|1.00"]);
    // The catalogs, in EBCDIC order as the primary's.
    assert_eq!(
        e.rows("SELECT TRIGGER_NAME, EVENT_MANIPULATION, ACTION_ORDER, ACTION_CONDITION, ACTION_STATEMENT, ACTION_TIMING, TRIGGER_MODE, ACTION_REFERENCE_OLD_ROW, ACTION_REFERENCE_NEW_ROW, TRIGGER_PROGRAM_NAME, ALLOW_REPEATED_CHANGE, TRIGGER_UPDATE_CONDITION FROM QSYS2.SYSTRIGGERS WHERE TRIGGER_NAME = 'TRB4'"),
        ["TRB4|UPDATE|4|( N . AMT < 0 )|SIGNAL SQLSTATE '75001' SET MESSAGE_TEXT = 'negative amount'|BEFORE|DB2ROW|O|N|TRB4|YES|ALWAYS"]
    );
    assert_eq!(e.rows("SELECT TRIGGER_NAME, TRIGGER_MODE FROM QSYS2.SYSTRIGGERS WHERE TRIGGER_NAME = 'TRA2'"), ["TRA2|DB2SQL"]);
    assert_eq!(
        e.rows("SELECT TRIGGER_NAME, TABLE_NAME, COLUMN_NAME, OBJECT_TYPE FROM QSYS2.SYSTRIGCOL WHERE TRIGGER_SCHEMA = 'QSTEST' ORDER BY TRIGGER_NAME, TABLE_NAME, COLUMN_NAME"),
        [
            "TRA2|STRGLOG|N|TABLE",
            "TRA2|STRGLOG|TXT|TABLE",
            "TRA2|STRG1|NAME|TABLE",
            "TRB1|STRG1|AMT|TABLE",
            "TRB1|STRG1|NAME|TABLE",
            "TRB4|STRG1|AMT|TABLE",
            "TRB5|STRG1|ID|TABLE",
            "TRB5|STRG1|ID|TABLE",
            "TRIGGER_WITH_A_LONG_NAME_A1|STRGLOG|N|TABLE",
            "TRIGGER_WITH_A_LONG_NAME_A1|STRGLOG|TXT|TABLE",
            "TRIGGER_WITH_A_LONG_NAME_A1|STRG1|NAME|TABLE",
        ]
    );
    assert_eq!(
        e.rows("SELECT TRIGGER_NAME, OBJECT_NAME, OBJECT_TYPE FROM QSYS2.SYSTRIGDEP WHERE TRIGGER_SCHEMA = 'QSTEST' ORDER BY TRIGGER_NAME, OBJECT_NAME"),
        ["TRA2|STRGLOG|TABLE", "TRA2|STRG1|TABLE", "TRIGGER_WITH_A_LONG_NAME_A1|STRGLOG|TABLE", "TRIGGER_WITH_A_LONG_NAME_A1|STRG1|TABLE"]
    );
    assert_eq!(e.rows("SELECT TRIGGER_NAME, TRIGGERED_UPDATE_COLUMNS FROM QSYS2.SYSTRIGUPD"), ["TRB4|AMT"]);
    // DROP TRIGGER takes its program; DROP TABLE takes them all.
    e.ok("DROP TRIGGER QSTEST.TRB5");
    assert!(!e.program_exists("TRB5"));
    assert_eq!(e.code("DROP TRIGGER QSTEST.TRB5"), (-204, "42704".into()));
    e.ok("DROP TABLE QSTEST.STRG1");
    assert!(!e.program_exists("TRB1") && !e.program_exists("TRIGG00001"));
    assert!(e.rows("SELECT TRIGGER_NAME FROM QSYS2.SYSTRIGGERS").is_empty());
}

#[test]
fn definitions_the_primary_refuses() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.STRG2 (ID INT NOT NULL, NAME CHAR(10), AMT DECIMAL(7, 2))");
    e.ok("CREATE VIEW QSTEST.STRGV AS SELECT ID, NAME FROM QSTEST.STRG2");
    let cases = [
        ("CREATE TRIGGER QSTEST.T1 BEFORE INSERT ON QSTEST.STRG2 REFERENCING OLD AS O FOR EACH ROW MODE DB2ROW SET O.ID = 1", -696, "42898"),
        ("CREATE TRIGGER QSTEST.T2 BEFORE INSERT ON QSTEST.STRG2 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW INSERT INTO QSTEST.STRG2 VALUES (1, 'X', 1)", -751, "0W000"),
        ("CREATE TRIGGER QSTEST.T3 BEFORE INSERT ON QSTEST.STRG2 FOR EACH STATEMENT MODE DB2SQL SET CURRENT SCHEMA = QSTEST", -628, "42613"),
        ("CREATE TRIGGER QSTEST.T4 INSTEAD OF INSERT ON QSTEST.STRG2 REFERENCING NEW AS N FOR EACH ROW MODE DB2SQL INSERT INTO QSTEST.STRG2 VALUES (1, 'X', 1)", -159, "42809"),
        ("CREATE TRIGGER QSTEST.T5 AFTER INSERT ON QSTEST.STRG2 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW SET N.NAME = 'X'", -312, "42618"),
        ("CREATE TRIGGER QSTEST.T6 AFTER INSERT ON QSTEST.STRG2 REFERENCING NEW TABLE AS NT FOR EACH ROW MODE DB2ROW SET CURRENT SCHEMA = QSTEST", -696, "42898"),
        ("CREATE TRIGGER QSTEST.T7 AFTER INSERT ON QSTEST.STRG2 REFERENCING NEW TABLE AS NT FOR EACH STATEMENT MODE DB2ROW SET CURRENT SCHEMA = QSTEST", -628, "42613"),
        // Not built yet: refused, not ignored.
        ("CREATE TRIGGER QSTEST.T8 AFTER INSERT ON QSTEST.STRG2 REFERENCING NEW TABLE AS NT FOR EACH STATEMENT INSERT INTO QSTEST.STRG2 VALUES (1, 'X', 1)", -84, "42612"),
        ("CREATE TRIGGER QSTEST.T9 INSTEAD OF INSERT ON QSTEST.STRGV REFERENCING NEW AS N FOR EACH ROW INSERT INTO QSTEST.STRG2 VALUES (N.ID, N.NAME, 0)", -84, "42612"),
        ("CREATE TRIGGER QSTEST.T10 AFTER INSERT OR DELETE ON QSTEST.STRG2 FOR EACH ROW SET CURRENT SCHEMA = QSTEST", -84, "42612"),
    ];
    for (text, code, state) in cases {
        assert_eq!(e.code(text), (code, state.into()), "{text}");
    }
    // A BEFORE trigger with no mode is DB2ROW, with the warning.
    let r = e.ok("CREATE TRIGGER QSTEST.TRC1 BEFORE UPDATE OF NAME, AMT ON QSTEST.STRG2 REFERENCING NEW AS N FOR EACH ROW SET N.NAME = UPPER(N.NAME)");
    assert_eq!((r.sqlca.sqlcode, r.sqlca.sqlstate.as_str()), (7051, "01647"));
    assert_eq!(e.code("CREATE TRIGGER QSTEST.TRC1 AFTER DELETE ON QSTEST.STRG2 FOR EACH ROW SET CURRENT SCHEMA = QSTEST"), (-601, "42710".into()));
    // OR REPLACE: the new program gets a name of its own while the old one is there.
    e.ok("CREATE OR REPLACE TRIGGER QSTEST.TRC1 BEFORE UPDATE OF NAME ON QSTEST.STRG2 REFERENCING NEW AS N FOR EACH ROW SET N.NAME = LOWER(N.NAME)");
    assert_eq!(e.rows("SELECT TRIGGER_NAME, ACTION_ORDER, TRIGGER_PROGRAM_NAME FROM QSYS2.SYSTRIGGERS"), ["TRC1|1|TRC100001"]);
    assert!(!e.program_exists("TRC1"));
    e.ok("INSERT INTO QSTEST.STRG2 VALUES (1, 'a', 1), (2, 'b', 2)");
    e.ok("UPDATE QSTEST.STRG2 SET NAME = 'NEW' WHERE ID = 1");
    e.ok("UPDATE QSTEST.STRG2 SET AMT = 9 WHERE ID = 2");
    assert_eq!(e.rows("SELECT ID, NAME, AMT FROM QSTEST.STRG2 ORDER BY ID"), ["1|new|1.00", "2|b|9.00"]);
    e.ok("LABEL ON TRIGGER QSTEST.TRC1 IS 'lower names'");
    e.ok("COMMENT ON TRIGGER QSTEST.TRC1 IS 'a long comment'");
    assert_eq!(e.rows("SELECT TRIGGER_TEXT, LONG_COMMENT FROM QSYS2.SYSTRIGGERS"), ["lower names|a long comment"]);
    assert_eq!(e.rows("SELECT TRIGGER_NAME, TRIGGERED_UPDATE_COLUMNS FROM QSYS2.SYSTRIGUPD"), ["TRC1|NAME"]);
}

#[test]
fn a_record_write_fires_an_sql_trigger() {
    let mut e = env();
    e.ok("CREATE TABLE QSTEST.RT (ID INT NOT NULL, NAME CHAR(10) NOT NULL)");
    e.ok("CREATE TABLE QSTEST.RLOG (TXT VARCHAR(40))");
    e.ok("CREATE TRIGGER QSTEST.RTB BEFORE INSERT ON QSTEST.RT REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW SET N.NAME = UPPER(N.NAME)");
    e.ok("CREATE TRIGGER QSTEST.RTA AFTER INSERT ON QSTEST.RT REFERENCING NEW AS N FOR EACH ROW INSERT INTO QSTEST.RLOG VALUES ('after ' CONCAT N.NAME)");
    // The job writes a record as RPG's WRITE does.
    let file = {
        let mut st = e.store.borrow_mut();
        let mut t = st.begin().unwrap();
        let f = Library::load(&mut t, e.lib).unwrap().lookup(&mut t, types::FILE, Name::parse("RT").unwrap()).unwrap().unwrap();
        t.abort();
        f
    };
    let mut job = e.job.borrow_mut();
    let resolve = db::odp::session::resolve_in(Name::parse("QSTEST").unwrap(), vec![(Name::parse("RT").unwrap(), file)]);
    job.io = Some(Box::new(mi::io::store::StoreIo::new(Box::new(Rc::clone(&e.store)), JobHandle(1), Box::new(resolve))));
    let spec = mi::io::OpenSpec {
        triggers: true,
        file: "RT".into(),
        lib: "QSTEST".into(),
        usage: qir::FileUsage::All,
        keyed: false,
        lvlchk: None,
        member: None,
        actgrp: String::new(),
        commit: false,
    };
    let (h, _) = job.io.as_mut().unwrap().open(&spec).unwrap();
    let mut rec = 7i32.to_be_bytes().to_vec();
    rec.extend(ccsid::to_ebcdic("rpg       ").unwrap());
    let req = mi::io::IoRequest { op: mi::io::IoOp::Write, record: Some(&rec), key: mi::io::KeyRef::None, format: None, nolock: false, nulls: None, resume: None };
    let r = mi::io::trigger::run(&mut job, &req, &mut |j, r| j.io.as_mut().unwrap().io(h, r));
    assert_eq!(r.status, 0, "{r:?}");
    drop(job);
    assert_eq!(e.rows("SELECT ID, NAME FROM QSTEST.RT"), ["7|RPG"]);
    assert_eq!(e.rows("SELECT TXT FROM QSTEST.RLOG"), ["after RPG"]);
}

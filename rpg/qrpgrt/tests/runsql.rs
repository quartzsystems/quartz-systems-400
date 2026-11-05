// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RUNSQLSTM`'s statements calling procedures and firing triggers: the
//! run's context has a mid caller (in the storage service, the job of
//! the command answers through `run_cmd_calling`; here a second job of
//! the same store makes the calls).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ile::{PgmKind, ProgramObject};
use mi::act::ObjectSource;
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Request, Session};
use sql::exec::routine::{CallPlan, midcall};
use sql::{RunOpts, SqlContext};
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

fn context(qsys: ObjectId, lib: ObjectId, locks: &db::odp::SharedLocks) -> SqlContext {
    let mut ctx = SqlContext::with_locks(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] }, locks.clone());
    ctx.now = Some(NOW);
    ctx.user = "QSECOFR".into();
    ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
    ctx
}

struct Env {
    store: Rc<RefCell<Store>>,
    /// The command's context, as `RUNSQLSTM` makes it.
    ctx: SqlContext,
    /// The job's own embedded SQL, for the definitions and the checks.
    sql: Session,
}

fn env(calls: bool) -> Env {
    let d = Arc::new(MemBlockDev::with_seed(65536, 9));
    let params = FormatParams { asp_number: 1, asp_uuid: [4; 16], unit_uuids: vec![[5; 16]], unit_serials: Vec::new(), wal_pages: 512, now: NOW };
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
            text: String::new(),
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
    let mut ctx = context(qsys, lib, &locks);
    if calls {
        // The job's side, as `command::call_for_service` answers.
        let j = Rc::clone(&job);
        let mid: midcall::MidCaller = Rc::new(move |plan: CallPlan| {
            let plan = CallPlan::decode(&plan.encode()).expect("the plan crosses");
            let out = qrpgrt::sqlcall::perform(&mut j.borrow_mut(), &plan);
            Some(out.answer.unwrap_or_else(|| out.called.inputs()))
        });
        ctx.midcall = Some(mid);
    }
    let sql = Session::with_ctx(Box::new(Rc::clone(&store)), context(qsys, lib, &locks));
    Env { store, ctx, sql }
}

impl Env {
    fn ok(&mut self, text: &str) {
        let r = self.sql.run(&Request { scope: "1:T".into(), desc: Desc { op: Op::Exec, sql_naming: true, text: text.into(), ..Desc::default() }, inputs: Vec::new() });
        assert!(r.sqlca.sqlcode >= 0, "{text}: {:?}", r.sqlca);
    }

    fn count(&mut self, table: &str) -> String {
        let r = self.sql.run(&Request {
            scope: "1:T".into(),
            desc: Desc { op: Op::Query, sql_naming: true, text: format!("SELECT COUNT(*) FROM {table}"), ..Desc::default() },
            inputs: Vec::new(),
        });
        r.rows[0][0].as_text()
    }

    fn runsqlstm(&mut self, text: &str) -> sql::RunReport {
        let opts = RunOpts { naming: sql::Naming::Sql, errlvl: 10, ..RunOpts::default() };
        sql::runsqlstm_lent(&self.store, &mut self.ctx, text, &opts).unwrap()
    }
}

#[test]
fn runsqlstm_calls_procedures_and_fires_triggers() {
    let mut e = env(true);
    e.ok("CREATE TABLE QSTEST.RT1 (ID INT NOT NULL, NAME CHAR(10))");
    e.ok("CREATE TABLE QSTEST.RTLOG (TXT VARCHAR(40))");
    e.ok("CREATE TRIGGER QSTEST.RTA1 AFTER INSERT ON QSTEST.RT1 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW INSERT INTO QSTEST.RTLOG VALUES ('ROW ' CONCAT N.NAME)");
    e.ok("CREATE PROCEDURE QSTEST.RTP1 (IN K INT) LANGUAGE SQL BEGIN INSERT INTO QSTEST.RTLOG VALUES ('PROC ' CONCAT CHAR(K)); END");
    e.ok("CREATE PROCEDURE QSTEST.RTP2 () LANGUAGE SQL BEGIN SIGNAL SQLSTATE '75002' SET MESSAGE_TEXT = 'no'; END");
    let r = e.runsqlstm("INSERT INTO QSTEST.RT1 VALUES (1, 'a');\nCALL QSTEST.RTP1(2);\n");
    assert_eq!((r.statements, r.errors), (2, 0), "{}", r.listing);
    assert_eq!(e.count("QSTEST.RT1"), "1");
    // The row fired the trigger; the procedure wrote its own row.
    assert_eq!(e.count("QSTEST.RTLOG"), "2");
    let r = e.runsqlstm("CALL QSTEST.RTP2();\n");
    assert_eq!(r.errors, 1, "{}", r.listing);
    assert!(r.listing.contains("-438"), "{}", r.listing);
}

#[test]
fn without_a_caller_they_are_refused() {
    let mut e = env(false);
    e.ok("CREATE TABLE QSTEST.RT1 (ID INT NOT NULL)");
    e.ok("CREATE PROCEDURE QSTEST.RTP1 () LANGUAGE SQL BEGIN END");
    let r = e.runsqlstm("CALL QSTEST.RTP1();\n");
    assert_eq!(r.errors, 1);
    assert!(r.listing.contains("-84"), "{}", r.listing);
}

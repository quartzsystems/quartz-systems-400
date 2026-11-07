// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! External functions end to end: `CREATE FUNCTION ... EXTERNAL NAME`
//! over a service program procedure of `PARAMETER STYLE SQL`; a query
//! calls it in the middle of the statement through the context's mid
//! caller (here a second job of the same store makes the call), with
//! its scratchpad and call types for `FINAL CALL`.

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ile::{PgmKind, ProgramObject};
use mi::act::ObjectSource;
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
    fn program(
        &mut self,
        name: &str,
        _lib: &str,
        kind: PgmKind,
    ) -> Option<(Arc<ProgramObject>, String)> {
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().ok()?;
        let ty = if kind == PgmKind::Pgm {
            types::PGM
        } else {
            types::SRVPGM
        };
        let id = Library::load(&mut txn, self.lib)
            .ok()?
            .lookup(&mut txn, ty, Name::parse(name).ok()?)
            .ok()??;
        let p = ile::store::load_program(&mut txn, id).ok()?;
        txn.abort();
        Some((Arc::new(p), "QSTEST".into()))
    }
}

struct Env {
    /// The session whose statements call the functions.
    sql: Session,
    store: Rc<RefCell<Store>>,
    lib: ObjectId,
    qsys: ObjectId,
}

fn context(qsys: ObjectId, lib: ObjectId, locks: &db::odp::SharedLocks) -> SqlContext {
    let mut ctx = SqlContext::with_locks(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
        locks.clone(),
    );
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
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
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
    let mut job = mi::Job::boxed(Box::new(StoreSource {
        store: Rc::clone(&store),
        lib,
    }));
    qrpgrt::install(&mut job);
    job.sql = Some(Box::new(Session::with_ctx(
        Box::new(Rc::clone(&store)),
        context(qsys, lib, &locks),
    )));
    let job = Rc::new(RefCell::new(job));
    // The job's side of a call in the middle of a statement.
    let j = Rc::clone(&job);
    let mid: midcall::MidCaller = Rc::new(move |plan: CallPlan| {
        let plan = CallPlan::decode(&plan.encode()).expect("the plan crosses");
        Some(mi::io::trigger::answer_mid(&mut j.borrow_mut(), &plan))
    });
    let mut ctx = context(qsys, lib, &locks);
    ctx.midcall = Some(mid);
    Env {
        sql: Session::with_ctx(Box::new(Rc::clone(&store)), ctx),
        store,
        lib,
        qsys,
    }
}

fn text(v: &Value) -> String {
    v.as_text().trim_end().to_string()
}

impl Env {
    fn run(&mut self, text: &str) -> Reply {
        self.sql.run(&Request {
            scope: "1:TEST".into(),
            desc: Desc {
                op: Op::Exec,
                sql_naming: true,
                text: text.into(),
                ..Desc::default()
            },
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
            desc: Desc {
                op,
                name: "Q".into(),
                sql_naming: true,
                text: text.into(),
                ..Desc::default()
            },
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
            out.extend(
                f.rows
                    .iter()
                    .map(|r| r.iter().map(text).collect::<Vec<_>>().join("|")),
            );
        }
        s.run(&req(Op::Close, ""));
        out
    }

    /// A service program made from IR text in QSTEST.
    fn srvpgm(&mut self, name: &str, text: &str) {
        let module = qir::parse(text).unwrap();
        qir::verify(&module).unwrap();
        let object = ile::ModuleObject::compile(module, ile::CreateData::default(), NOW).unwrap();
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().unwrap();
        let c = ile::store::Ctx {
            qsys: self.qsys,
            names: NameContext {
                curlib: Some(self.lib),
                libl: vec![self.lib],
            },
            now: NOW,
            owner: "QSECOFR".into(),
            system: "QS400".into(),
        };
        let n = Name::parse(name).unwrap();
        let m = ile::store::crtmod(&mut txn, &c, self.lib, n, &object, "").unwrap();
        let spec = ile::store::ObjSpec {
            lib: qsname::LibSpec::Name(Name::parse("QSTEST").unwrap()),
            name: n,
        };
        ile::store::crtpgm(
            &mut txn,
            &c,
            self.lib,
            &[spec],
            &[],
            ile::BindOptions::new(name, PgmKind::SrvPgm),
            "",
        )
        .unwrap();
        store::obj::ops::delete(&mut txn, JobHandle(0), m).unwrap();
        txn.commit().unwrap();
    }
}

/// `TWICE(a)`: twice its argument, null for null, SQLSTATE 38001 for a
/// negative one; `COUNTER()`: 1, 2, 3 ... from its scratchpad, its call
/// type added to the hundreds (`FINAL CALL`).
const FUNCS: &str = "module QFX lang(QIR) ccsid(37) actgrp(*CALLER) nomain
proc TWICE($a: i32, $r: i32, $ai: i16, $ri: i16, $st: char(5), $fn: varchar(517), $sn: varchar(128), $msg: varchar(70)) export {
L0:
  %n = load $ai
  %isnull = cmp.lt %n, 0
  brif %isnull, L1, L2
L1:
  store $ri, -1
  ret
L2:
  %a = load $a
  %neg = cmp.lt %a, 0
  brif %neg, L3, L4
L3:
  mov $st, \"38001\"
  mov $msg, \"negative\"
  ret
L4:
  %r = mul %a, 2
  store $r, %r
  store $ri, 0
  ret
}
proc COUNTER($r: i32, $ri: i16, $st: char(5), $fn: varchar(517), $sn: varchar(128), $msg: varchar(70), $pad: bytes(8), $ct: i32) export {
L0:
  %p = addr $pad
  %c = load [%p +4]:i32
  %c1 = add %c, 1
  store [%p +4]:i32, %c1
  %t = load $ct
  %h = mul %t, 100
  %v = add %c1, %h
  store $r, %v
  store $ri, 0
  ret
}
";

#[test]
fn an_external_function_runs_in_the_middle_of_a_query() {
    let mut e = env();
    e.srvpgm("QFX", FUNCS);
    e.ok("CREATE FUNCTION QSTEST.TWICE (A INTEGER) RETURNS INTEGER LANGUAGE RPGLE EXTERNAL NAME 'QSTEST/QFX(TWICE)' PARAMETER STYLE SQL CALLED ON NULL INPUT");
    e.ok("CREATE TABLE QSTEST.FX (N INTEGER)");
    e.ok("INSERT INTO QSTEST.FX VALUES (1), (2), (NULL)");
    assert_eq!(
        e.rows("SELECT TWICE(N) FROM QSTEST.FX ORDER BY N"),
        ["2", "4", "NULL"]
    );
    assert_eq!(e.rows("VALUES QSTEST.TWICE(21)"), ["42"]);
    let (code, state) = e.code("VALUES QSTEST.TWICE(-1)");
    assert_eq!((code, state.as_str()), (-443, "38001"));
    e.ok("CREATE FUNCTION QSTEST.COUNTER () RETURNS INTEGER LANGUAGE RPGLE EXTERNAL NAME 'QSTEST/QFX(COUNTER)' PARAMETER STYLE SQL SCRATCHPAD 4 FINAL CALL DISALLOW PARALLEL NOT DETERMINISTIC");
    // The first call's type is -1, the others' 0; the scratchpad counts.
    assert_eq!(e.rows("SELECT COUNTER() FROM QSTEST.FX"), ["-99", "2", "3"]);
    // A new statement: a new scratchpad.
    assert_eq!(e.rows("SELECT COUNTER() FROM QSTEST.FX"), ["-99", "2", "3"]);
    let (code, _) = e.code("CREATE FUNCTION QSTEST.GEN (A INTEGER) RETURNS INTEGER LANGUAGE RPGLE EXTERNAL NAME 'QSTEST/QFX(TWICE)' PARAMETER STYLE GENERAL");
    assert_eq!(code, -84);
}

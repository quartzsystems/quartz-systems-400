// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database server against the `jdbc` trace, over the Phase 3 SQL
//! engine in memory with an empty `QSTEST` (the trace makes its table).

mod common;

use common::mem::session;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::database::{Database, DbConfig};
use hostserver::{Handler, NoExits};

fn masks() -> Masks {
    Masks {
        // Whether a reply is compressed depends on its size.
        template: vec![(0x2800, 4..8)],
        param: vec![
            // SQLCA: message tokens and the product.
            (0x3807, 16..96),
            // SQLERRD(2) and the statement kind in SQLERRD(6).
            (0x3807, 100..104),
            (0x3807, 116..120),
            // Server attributes: the job number.
            (0x3804, 0..2),
            (0x3804, 110..116),
            // The consistency token of result data; cursor attributes.
            (0x380E, 0..4),
            (0x3814, 0..10),
        ],
        // Message texts are ours.
        skip: vec![0x3802, 0x3803],
    }
}

#[test]
fn jdbc() {
    for s in ["jdbc", "metadata", "pwdlevel", "svrmap"] {
        let t = trace("database", s);
        let auth = TraceAuth::of(&t);
        let cfg = DbConfig {
            rdb: "S062B175".into(),
            job: ("QZDASOINIT".into(), "QUSER".into(), "007839".into()),
        };
        let diffs = replay(&t, &masks(), &mut || Box::new(Database::new(auth.clone(), session(), NoExits, cfg.clone())) as Box<dyn Handler>);
        accept(&format!("database-{s}"), &diffs);
    }
}

/// The engine with a job that makes the calls it plans: `HSPROC1`, the
/// SQL procedure of the `callproc` trace, stands in as an external
/// procedure that gives `WORD` = `DONE`, doubles `AMT` and returns
/// its two result sets.
struct Calling {
    s: sql::embed::Session,
    outs: Vec<(usize, sql::value::Value)>,
    made: bool,
}

impl sql::embed::Embedded for Calling {
    fn run(&mut self, req: &sql::embed::Request) -> sql::embed::Reply {
        use sql::embed::{Desc, Op, Request};
        use sql::value::Value;
        let scope = |s: &str| s.to_string();
        if !self.made {
            self.made = true;
            let ddl = "CREATE PROCEDURE QSTEST.HSPROC1 (IN N INTEGER, OUT WORD VARCHAR(10), INOUT AMT DECIMAL(7, 2))                 DYNAMIC RESULT SETS 2 EXTERNAL NAME QSTEST.RSPGM PARAMETER STYLE GENERAL";
            let d = Desc { op: Op::Exec, sql_naming: true, text: ddl.into(), ..Desc::default() };
            assert_eq!(self.s.run(&Request { scope: scope("MAKE:"), desc: d, inputs: Vec::new() }).sqlca.sqlcode, 0);
        }
        if req.desc.op == Op::ExecImm && req.inputs.first().is_some_and(|t| t.as_text().contains("PROCEDURE QSTEST.HSPROC1 (")) {
            // The SQL body is the stand-in made above.
            return sql::embed::Reply::default();
        }
        let mut req = req.clone();
        req.desc.calls = true;
        let r = self.s.run(&req);
        let Some(plan) = r.call else { return r };
        let n = plan.parms[0].value.as_text();
        let amt = plan.parms[2].value.as_text().parse::<f64>().unwrap_or(0.0) * 2.0;
        self.outs = vec![(1, Value::Str("DONE".into())), (2, Value::Num(qdecimal::Decimal::parse(&format!("{amt:.2}")).unwrap()))];
        for (c, q) in [
            ("C1", format!("SELECT ID, NAME FROM (VALUES (1, 'ANN'), (2, 'BOB'), (3, 'CY')) AS T (ID, NAME) WHERE ID <= {n} ORDER BY ID")),
            ("C2", format!("SELECT {n} * 10 AS TEN FROM SYSIBM.SYSDUMMY1")),
        ] {
            let d = Desc { op: Op::Open, name: c.into(), sql_naming: true, text: q, returns: Some(sql::ast::ReturnTo::Caller), ..Desc::default() };
            let o = self.s.run(&Request { scope: scope("9:HSPROC1"), desc: d, inputs: Vec::new() });
            assert_eq!(o.sqlca.sqlcode, 0, "{:?}", o.sqlca);
        }
        let called = sql::exec::routine::Called {
            seq: plan.seq,
            max: plan.max_results,
            sqlstate: "00000".into(),
            routine: plan.routine.clone(),
            ..Default::default()
        };
        self.s.run(&Request { scope: req.scope, desc: Desc { op: Op::Called, ..Desc::default() }, inputs: called.inputs() })
    }

    fn take_outs(&mut self) -> Vec<(usize, sql::value::Value)> {
        std::mem::take(&mut self.outs)
    }
}

#[test]
fn callproc() {
    let t = trace("database", "callproc");
    let auth = TraceAuth::of(&t);
    let cfg = DbConfig {
        rdb: "S062B175".into(),
        job: ("QZDASOINIT".into(), "QUSER".into(), "007839".into()),
    };
    let diffs = replay(&t, &masks(), &mut || {
        let e = Calling { s: session(), outs: Vec::new(), made: false };
        Box::new(Database::new(auth.clone(), e, NoExits, cfg.clone())) as Box<dyn Handler>
    });
    accept("database-callproc", &diffs);
}

#[test]
#[ignore = "prints how the engine takes the trace's statements"]
fn probe_statements() {
    use sql::embed::{Desc, Embedded, Op, Request};
    use sql::value::Value;
    let mut s = session();
    for q in [
        "SELECT TABLE_NAME, TABLE_TYPE, COLUMN_COUNT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME FETCH FIRST 400 ROWS ONLY",
        "SELECT COUNT(*) FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME = 'QSTEST'",
        "SELECT CAST(NULL AS INTEGER), CAST(1.5 AS DOUBLE), CAST('x' AS CHAR(3)), DATE('2026-01-02'), BIGINT(9) FROM SYSIBM.SYSDUMMY1",
        "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_TEXT FROM QSYS2.SYSTABLES",
    ] {
        let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Prepare, name: "S1".into(), into: true, sql_naming: true, ..Desc::default() }, inputs: vec![Value::Str(q.into())] });
        println!("{} {} {:?} rows={}", r.sqlca.sqlcode, r.sqlca.sqlstate, r.sqlca.sqlerrmc, r.rows.len());
    }
    let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Open, name: "C1".into(), sql_naming: true, text: "SELECT TABLE_SCHEMA, TABLE_NAME FROM QSYS2.SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("open {} {}", r.sqlca.sqlcode, r.sqlca.sqlerrmc);
    let q = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Query, sql_naming: true, text: "SELECT COUNT(*) FROM QSYS2.SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("query {} {} {:?}", q.sqlca.sqlcode, q.sqlca.sqlerrmc, q.rows);
    let q = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Query, sql_naming: false, text: "SELECT COUNT(*) FROM QSYS2/SYSTABLES".into(), ..Desc::default() }, inputs: vec![] });
    println!("query sys {} {} {:?}", q.sqlca.sqlcode, q.sqlca.sqlerrmc, q.rows);
    let r = s.run(&Request { scope: "X:".into(), desc: Desc { op: Op::Fetch, name: "C1".into(), rows: true, sql_naming: true, ..Desc::default() }, inputs: vec![Value::int(1000)] });
    println!("fetch {} {}", r.sqlca.sqlcode, r.rows.len());
    for row in r.rows.iter().take(5) { println!("{row:?}"); }
}

/// Extended dynamic SQL (the `sqlpkg` trace): every connection over one
/// store, so the package one connection fills is the next one's.
#[test]
fn sqlpkg() {
    let t = trace("database", "sqlpkg");
    let auth = TraceAuth::of(&t);
    let cfg = DbConfig {
        rdb: "S062B175".into(),
        job: ("QZDASOINIT".into(), "QUSER".into(), "007839".into()),
    };
    let (store, qsys, names) = common::mem::store();
    let ours = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    // A statement's name is made from the package's token, the time it
    // was made: not the oracle's, and not the same from run to run.
    let mut m = masks();
    m.param.push((0x380B, 45..63));
    let diffs = replay(&t, &m, &mut || {
        let s = sql::embed::Session::new(Box::new(store.clone()), qsys, store::obj::JobHandle(1), names.clone());
        let db = Box::new(Database::new(auth.clone(), s, NoExits, cfg.clone()));
        Box::new(Renamer { inner: db, ours: ours.clone() }) as Box<dyn Handler>
    });
    accept("database-sqlpkg", &diffs);
    assert_eq!(ours.borrow().len(), 1, "one statement in the package");
}

/// Our package's statements are named by our token, the trace's by the
/// primary's: a request naming a package statement (`0x3806`, `QZ...`)
/// gets the name our return package gave.
struct Renamer {
    inner: Box<dyn Handler>,
    ours: std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>,
}

impl Handler for Renamer {
    fn handle(&mut self, frame: &[u8]) -> hostserver::Outcome {
        let mut f = hostsvr::frame::Frame::decode(frame).unwrap();
        if let Some(name) = self.ours.borrow().first() {
            for p in &mut f.params {
                if p.cp == 0x3806 && p.data.len() == 22 && p.data[4..6] == [0xD8, 0xE9] {
                    p.data[4..].copy_from_slice(name);
                }
            }
        }
        let o = self.inner.handle(&f.encode());
        for r in &o.replies {
            let f = hostsvr::frame::Frame::decode(r).unwrap();
            if let Some(p) = f.params.iter().find(|p| p.cp == 0x380B && p.data.len() > 63) {
                let name = p.data[45..63].to_vec();
                let mut ours = self.ours.borrow_mut();
                if !ours.contains(&name) {
                    ours.push(name);
                }
            }
        }
        o
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL changes fire external triggers (`ADDPFTRG`) as RPG's do: through
//! the context's caller, with the SQLCODEs the primary gives
//! (`tools/conformance/sql/oracle/trigger/primary-facts.md`).

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::trg::ops::NewTrg;
use db::trg::{Call, Caller, Event, Failure, Time};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::txn::Txn;
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

    fn add(&mut self, table: &str, n: &NewTrg) {
        let mut t = self.store.begin().unwrap();
        let f = ops::resolve(&mut t, self.ctx.qsys, &self.ctx.names, qsname::LibSpec::Libl, Name::parse(table).unwrap(), types::FILE).unwrap().id;
        db::trg::ops::add(&mut t, f, n, 1).unwrap();
        t.commit().unwrap();
    }
}

/// The probe's program: a new record starting X is refused, one starting
/// R gets a Z; a program named MISSING is not found.
#[derive(Default)]
struct Probe {
    calls: Vec<(String, u8, u8)>,
}

impl Caller for Probe {
    fn call(&mut self, _txn: &mut Txn<'_>, c: &mut Call) -> Result<(), Failure> {
        self.calls.push((c.trigger.clone(), c.buffer[30], c.buffer[31]));
        if c.program.1 == Name::parse("MISSING").unwrap() {
            return Err(Failure { msgid: "CPF4160".into(), text: "MISSING in QSTEST type *PGM not found".into(), not_found: true });
        }
        let off = u32::from_be_bytes(c.buffer[64..68].try_into().unwrap()) as usize;
        match c.buffer[off] {
            0xE7 if c.event != Event::Delete => {
                return Err(Failure { msgid: "CPF9898".into(), text: "refused".into(), not_found: false });
            }
            0xD9 => c.buffer[off] = 0xE9,
            _ => {}
        }
        Ok(())
    }
}

fn pgm(n: &str) -> (Name, Name) {
    (Name::parse("QSTEST").unwrap(), Name::parse(n).unwrap())
}

fn trg(name: &str, time: Time, event: Event, alw: bool) -> NewTrg {
    NewTrg { name: Some(name.into()), alwrepchg: alw, ..NewTrg::new(time, event, pgm("TRGDUMP")) }
}

#[test]
fn sql_changes_fire_external_triggers() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST.TRGC (C1 CHAR(5) NOT NULL, N INTEGER)");
    db.ok("CREATE VIEW QSTEST.TRGV AS SELECT C1 FROM QSTEST.TRGC");
    db.add("TRGC", &trg("BEFINS", Time::Before, Event::Insert, true));
    db.add("TRGC", &trg("AFTINS", Time::After, Event::Insert, false));
    db.add("TRGC", &trg("AFTUPD", Time::After, Event::Update, false));
    db.add("TRGC", &trg("AFTDLT", Time::After, Event::Delete, false));
    // Without a caller: refused, SQL0084.
    assert_eq!(db.code("INSERT INTO QSTEST.TRGC VALUES ('aaaaa', 1)"), (-84, "42612".to_string()));
    let probe = Rc::new(RefCell::new(Probe::default()));
    db.ctx.triggers = Some(probe.clone());
    db.ok("INSERT INTO QSTEST.TRGC VALUES ('Raaaa', 1)");
    assert_eq!(probe.borrow().calls, [("BEFINS".into(), 0xF1, 0xF2), ("AFTINS".into(), 0xF1, 0xF1)]);
    assert_eq!(db.rows("SELECT C1 FROM QSTEST.TRGC"), ["Zaaaa"]);
    // The primary: SQL0443, SQLSTATE 38501.
    assert_eq!(db.code("INSERT INTO QSTEST.TRGC VALUES ('Xbbbb', 2)"), (-443, "38501".to_string()));
    // Through a view: the table's triggers.
    probe.borrow_mut().calls.clear();
    db.ok("INSERT INTO QSTEST.TRGV VALUES ('Rcccc')");
    assert_eq!(probe.borrow().calls.len(), 2);
    assert_eq!(db.rows("SELECT C1 FROM QSTEST.TRGC ORDER BY C1"), ["Zaaaa", "Zcccc"]);
    probe.borrow_mut().calls.clear();
    db.ok("UPDATE QSTEST.TRGC SET N = 5");
    db.ok("DELETE FROM QSTEST.TRGC WHERE C1 = 'Zcccc'");
    let names: Vec<String> = probe.borrow().calls.iter().map(|c| c.0.clone()).collect();
    assert_eq!(names, ["AFTUPD", "AFTUPD", "AFTDLT"]);
    // A program not found: SQL0204, as the primary.
    db.add("TRGC", &NewTrg { name: Some("GONE".into()), ..NewTrg::new(Time::Before, Event::Update, pgm("MISSING")) });
    db.ctx.triggers = Some(probe.clone());
    assert_eq!(db.code("UPDATE QSTEST.TRGC SET N = 6"), (-204, "42704".to_string()));
    // Read triggers: with no caller a query is refused.
    db.add("TRGC", &trg("RD", Time::After, Event::Read, false));
    db.ctx.triggers = None;
    assert_eq!(db.code("SELECT * FROM QSTEST.TRGC").0, -84);
}

/// The rows a query's reads fire for, as the primary
/// (`tools/conformance/sql/oracle/trigger/p7.out`).
#[test]
fn read_triggers_fire_for_the_rows_a_scan_keeps() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST.RDT (K INT NOT NULL, V CHAR(5))");
    db.ok("CREATE TABLE QSTEST.RDT2 (K INT NOT NULL, V CHAR(5))");
    db.ok("INSERT INTO QSTEST.RDT VALUES (1, 'aaaaa'), (2, 'bbbbb'), (3, 'ccccc')");
    db.add("RDT", &trg("RD1", Time::After, Event::Read, false));
    let probe = Rc::new(RefCell::new(Probe::default()));
    db.ctx.triggers = Some(probe.clone());
    let fired = |db: &mut Db, sql: &str| {
        probe.borrow_mut().calls.clear();
        db.ok(sql);
        let calls = probe.borrow().calls.clone();
        assert!(calls.iter().all(|c| c.0 == "RD1" && c.1 == 0xF4 && c.2 == 0xF1), "{sql}: {calls:?}");
        calls.len()
    };
    assert_eq!(fired(&mut db, "SELECT K, V FROM QSTEST.RDT ORDER BY K"), 3);
    assert_eq!(fired(&mut db, "SELECT V FROM QSTEST.RDT WHERE K = 2"), 1);
    assert_eq!(fired(&mut db, "SELECT COUNT(*) FROM QSTEST.RDT"), 3);
    // The primary fires 4: the inner table's rows only as the join reads
    // them (BLOCKED(rel) in exec::reads); every row its scan keeps here.
    assert_eq!(fired(&mut db, "SELECT A.K FROM QSTEST.RDT A JOIN QSTEST.RDT B ON A.K = B.K WHERE A.K < 3 ORDER BY 1"), 5);
    db.ok("CREATE INDEX QSTEST.RDTIX ON QSTEST.RDT (K)");
    assert_eq!(fired(&mut db, "SELECT V FROM QSTEST.RDT WHERE K = 3"), 1);
    db.ok("CREATE VIEW QSTEST.RDTV AS SELECT K, V FROM QSTEST.RDT WHERE K > 1");
    assert_eq!(fired(&mut db, "SELECT K FROM QSTEST.RDTV ORDER BY K"), 2);
    assert_eq!(fired(&mut db, "UPDATE QSTEST.RDT SET V = 'uuuuu' WHERE K = 1"), 0);
    assert_eq!(fired(&mut db, "DELETE FROM QSTEST.RDT WHERE K = 99"), 0);
    assert_eq!(fired(&mut db, "INSERT INTO QSTEST.RDT2 SELECT K, V FROM QSTEST.RDT WHERE K > 2"), 1);
}

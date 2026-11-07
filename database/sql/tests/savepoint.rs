// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Savepoints: `SAVEPOINT`, `RELEASE SAVEPOINT`, `ROLLBACK TO
//! SAVEPOINT` under commitment control and without it, as the primary
//! answers them (tools/conformance/sql/oracle/probes/sqlpl/sp1.out).

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::cmt::LckLvl;
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, prepare};
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

fn ids(db: &mut Db) -> Vec<String> {
    db.rows("SELECT id FROM spsch/t ORDER BY id")
        .into_iter()
        .map(|r| r[0].clone())
        .collect()
}

fn under_commit(db: &mut Db) {
    let mut t = db.store.begin().unwrap();
    db.ctx
        .session
        .strcmtctl(&mut t, LckLvl::Chg, None, 1_767_225_600)
        .unwrap();
    t.commit().unwrap();
}

#[test]
fn savepoints_as_the_primary() {
    let mut db = Db::new();
    db.ok("CREATE SCHEMA spsch");
    db.ok("CREATE TABLE spsch/t (id INT NOT NULL)");
    // Without commitment control a savepoint is kept by name and marks
    // nothing (tools/conformance/sql/expected/sqlpl/SAVEPT.out).
    db.ok("SAVEPOINT a ON ROLLBACK RETAIN CURSORS");
    db.ok("SAVEPOINT b UNIQUE ON ROLLBACK RETAIN CURSORS");
    db.ok("INSERT INTO spsch/t VALUES 9");
    db.ok("ROLLBACK TO SAVEPOINT b");
    assert_eq!(ids(&mut db), ["9"]);
    assert_eq!(db.code("SAVEPOINT b ON ROLLBACK RETAIN CURSORS"), -881);
    db.ok("ROLLBACK TO SAVEPOINT a");
    assert_eq!(db.code("RELEASE SAVEPOINT b"), -880);
    db.ok("RELEASE SAVEPOINT a");
    assert_eq!(db.code("ROLLBACK TO SAVEPOINT"), -882);
    db.ok("DELETE FROM spsch/t");
    under_commit(&mut db);
    db.ok("SAVEPOINT a ON ROLLBACK RETAIN CURSORS");
    db.ok("INSERT INTO spsch/t VALUES 1");
    db.ok("SAVEPOINT b UNIQUE ON ROLLBACK RETAIN CURSORS ON ROLLBACK RETAIN LOCKS");
    db.ok("INSERT INTO spsch/t VALUES 2");
    assert_eq!(
        db.run("SAVEPOINT b ON ROLLBACK RETAIN CURSORS").0.sqlstate,
        "3B501"
    );
    db.ok("SAVEPOINT a ON ROLLBACK RETAIN CURSORS");
    db.ok("INSERT INTO spsch/t VALUES 3");
    assert_eq!(ids(&mut db), ["1", "2", "3"]);
    db.ok("ROLLBACK TO SAVEPOINT b");
    assert_eq!(ids(&mut db), ["1"]);
    // A replaced its namesake and came after B: gone with the rollback.
    assert_eq!(db.run("ROLLBACK TO SAVEPOINT a").0.sqlstate, "3B001");
    db.ok("ROLLBACK TO SAVEPOINT b");
    db.ok("ROLLBACK TO SAVEPOINT");
    db.ok("ROLLBACK WORK TO SAVEPOINT");
    assert_eq!(ids(&mut db), ["1"]);
    db.ok("RELEASE SAVEPOINT b");
    assert_eq!(db.code("RELEASE TO SAVEPOINT b"), -880);
    assert_eq!(db.code("ROLLBACK TO SAVEPOINT x"), -880);
    db.ok("SAVEPOINT c ON ROLLBACK RETAIN CURSORS");
    db.ok("INSERT INTO spsch/t VALUES 4");
    db.ok("COMMIT");
    assert_eq!(db.code("ROLLBACK TO SAVEPOINT c"), -880);
    assert_eq!(db.run("ROLLBACK TO SAVEPOINT").0.sqlstate, "3B502");
    assert_eq!(ids(&mut db), ["1", "4"]);
    db.ok("SAVEPOINT d ON ROLLBACK RETAIN CURSORS");
    db.ok("INSERT INTO spsch/t VALUES 5");
    db.ok("ROLLBACK");
    assert_eq!(db.code("RELEASE SAVEPOINT d"), -880);
    assert_eq!(ids(&mut db), ["1", "4"]);
    db.ok("SAVEPOINT \"lower\" ON ROLLBACK RETAIN CURSORS");
    assert_eq!(db.code("RELEASE SAVEPOINT lower"), -880);
    db.ok("RELEASE SAVEPOINT \"lower\"");
    db.ok("COMMIT");
}

#[test]
fn savepoint_syntax() {
    for (text, code) in [
        ("SAVEPOINT a", -104),
        ("SAVEPOINT e ON ROLLBACK RETAIN LOCKS", -104),
        (
            "SAVEPOINT f ON ROLLBACK RETAIN CURSORS ON ROLLBACK RETAIN CURSORS",
            -104,
        ),
        ("SAVEPOINT g UNIQUE UNIQUE ON ROLLBACK RETAIN CURSORS", -199),
    ] {
        let e = prepare(text, Naming::Sys).unwrap_err();
        assert_eq!(sql::Sqlca::from_error(&e).sqlcode, code, "{text}");
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Joins keep only the rows that match: a three-way join of large
//! tables runs inside a small working-storage limit, and a statement
//! that would hold more than its limit ends with SQL0904.

use std::fmt::Write as _;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, RunOpts, SqlContext, SqlError, prepare, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

struct Db {
    store: Store,
    ctx: SqlContext,
}

/// Tables A, B and C of `n` rows: A.BID names a row of B, B.CID one of C.
fn db(n: i64) -> Db {
    let d = Arc::new(MemBlockDev::with_seed(32768, 21));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
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
    let mut ctx = SqlContext::new(qsys, JobHandle(1), names);
    ctx.now = Some(NOW);
    let mut text = String::from(
        "CREATE TABLE QSTEST.A (ID INT NOT NULL, BID INT NOT NULL);
         CREATE TABLE QSTEST.B (ID INT NOT NULL, CID INT NOT NULL);
         CREATE TABLE QSTEST.C (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);",
    );
    for (t, mul) in [("A", 7), ("B", 13)] {
        for chunk in 0..n / 100 {
            let _ = write!(text, "INSERT INTO QSTEST.{t} VALUES ");
            for i in 0..100 {
                let id = chunk * 100 + i + 1;
                let to = (id * mul) % n + 1;
                let _ = write!(text, "{}({id}, {to})", if i > 0 { ", " } else { "" });
            }
            text.push(';');
        }
    }
    for chunk in 0..n / 100 {
        let _ = write!(text, "INSERT INTO QSTEST.C VALUES ");
        for i in 0..100 {
            let id = chunk * 100 + i + 1;
            let _ = write!(text, "{}({id}, 'C{id}')", if i > 0 { ", " } else { "" });
        }
        text.push(';');
    }
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(&mut store, &mut ctx, &text, &opts).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    Db { store, ctx }
}

impl Db {
    fn run(&mut self, text: &str) -> Result<Vec<Vec<String>>, SqlError> {
        let p = prepare(text, Naming::Sql)?;
        let mut t = self.store.begin().unwrap();
        let r = p.execute(&mut t, &mut self.ctx, &[]);
        t.abort();
        match r? {
            Outcome::Rows(rs) => Ok(rs
                .rows
                .iter()
                .map(|r| r.iter().map(|v| v.to_string().trim_end().to_string()).collect())
                .collect()),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn one(&mut self, text: &str) -> String {
        self.run(text).unwrap_or_else(|e| panic!("{text}: {e}"))[0][0].clone()
    }
}

/// What the joins should give for row `id` of A.
fn expected(n: i64, id: i64) -> (i64, String) {
    let b = (id * 7) % n + 1;
    let c = (b * 13) % n + 1;
    (b, format!("C{c}"))
}

fn three_way(n: i64) {
    let mut db = db(n);
    // Far less than the cross product would need.
    db.ctx.memory_limit = 8 << 20;
    let count = n.to_string();
    assert_eq!(
        db.one("SELECT COUNT(*) FROM QSTEST.A, QSTEST.B, QSTEST.C WHERE A.BID = B.ID AND B.CID = C.ID"),
        count
    );
    assert_eq!(
        db.one(
            "SELECT COUNT(*) FROM QSTEST.A JOIN QSTEST.B ON A.BID = B.ID JOIN QSTEST.C ON C.ID = B.CID"
        ),
        count
    );
    // The rows themselves, in the outer table's order.
    let rows = db
        .run(
            "SELECT A.ID, B.ID, C.NAME FROM QSTEST.A, QSTEST.B, QSTEST.C \
             WHERE A.BID = B.ID AND B.CID = C.ID AND A.ID <= 5",
        )
        .unwrap();
    let want: Vec<Vec<String>> = (1..=5)
        .map(|id| {
            let (b, c) = expected(n, id);
            vec![id.to_string(), b.to_string(), c]
        })
        .collect();
    assert_eq!(rows, want);
    let rows = db
        .run(
            "SELECT A.ID, C.NAME FROM QSTEST.A LEFT JOIN QSTEST.B ON A.BID = B.ID \
             JOIN QSTEST.C ON B.CID = C.ID ORDER BY A.ID DESC FETCH FIRST 2 ROWS ONLY",
        )
        .unwrap();
    assert_eq!(rows, [[n.to_string(), expected(n, n).1], [(n - 1).to_string(), expected(n, n - 1).1]]);
    // A condition no key serves: tried row by row, the same answer.
    assert_eq!(
        db.one(
            "SELECT COUNT(*) FROM QSTEST.A, QSTEST.B, QSTEST.C \
             WHERE A.BID = B.ID AND B.CID = C.ID + 0 AND A.ID <= 20"
        ),
        "20"
    );
}

#[test]
fn three_way_join_of_400_rows() {
    three_way(400);
}

#[test]
fn three_way_join_of_2000_rows() {
    three_way(2000);
}

#[test]
fn a_statement_past_its_limit_ends_with_sql0904() {
    let mut db = db(400);
    // 64 million rows: the default limit stops it.
    let e = db
        .run("SELECT A.ID, B.ID, C.ID FROM QSTEST.A, QSTEST.B, QSTEST.C")
        .unwrap_err();
    assert_eq!((e.sqlcode(), e.sqlstate()), (-904, "57011"), "{e}");
    // Grouping and DISTINCT are bounded too.
    db.ctx.memory_limit = 1 << 20;
    for q in [
        "SELECT A.ID, B.ID, COUNT(*) FROM QSTEST.A, QSTEST.B GROUP BY A.ID, B.ID",
        "SELECT DISTINCT A.ID, B.ID FROM QSTEST.A, QSTEST.B",
        "SELECT A.ID FROM QSTEST.A JOIN QSTEST.B ON A.ID <> B.ID",
    ] {
        let e = db.run(q).unwrap_err();
        assert_eq!(e.sqlcode(), -904, "{q}: {e}");
    }
    // What the failed statements held was given back.
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST.A, QSTEST.B WHERE A.BID = B.ID"), "400");
}

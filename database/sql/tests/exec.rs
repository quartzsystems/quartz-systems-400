// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements run against a store: tables and indexes created from SQL,
//! inserts, queries with every clause, access path choice, joins,
//! grouping, functions, updates and deletes, and the SQLCODEs of the
//! errors.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, Value, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

fn open_new() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 11));
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

    fn run(&mut self, text: &str) -> Result<Outcome, SqlError> {
        self.run_with(text, &[])
    }

    fn run_with(&mut self, text: &str, params: &[Value]) -> Result<Outcome, SqlError> {
        let p = prepare(text, Naming::Sys)?;
        let mut t = self.store.begin().unwrap();
        let r = p.execute(&mut t, &mut self.ctx, params);
        match &r {
            Ok(_) => t.commit().unwrap(),
            Err(_) => t.abort(),
        }
        r
    }

    fn ok(&mut self, text: &str) -> Outcome {
        self.run(text).unwrap_or_else(|e| panic!("{text}: {e}"))
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

    fn count(&mut self, text: &str) -> u64 {
        match self.ok(text) {
            Outcome::Count(n) => n,
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, text: &str) -> i32 {
        match self.run(text) {
            Err(e) => e.sqlcode(),
            Ok(o) => panic!("{text} succeeded: {o:?}"),
        }
    }

    fn plan(&mut self, text: &str) -> String {
        self.rows(&format!("EXPLAIN {text}"))
            .iter()
            .map(|r| r[0].clone())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

fn setup(db: &mut Db) {
    db.ok("CREATE TABLE qstest/cust (cusno DECIMAL(7, 0) NOT NULL PRIMARY KEY, name CHAR(30) NOT NULL, bal NUMERIC(9, 2) NOT NULL DEFAULT 0, since DATE, note VARCHAR(100)) RCDFMT cusrec");
    db.ok("CREATE TABLE qstest/orders (ordno INTEGER NOT NULL PRIMARY KEY, cusno DECIMAL(7, 0) NOT NULL, amount DECIMAL(9, 2) NOT NULL, placed TIMESTAMP)");
    assert_eq!(
        db.count("INSERT INTO cust (cusno, name, bal, since, note) VALUES (1, 'Alice', 10.50, '2026-01-15', 'first'), (2, 'Bob', 0, DATE '2025-12-31', NULL), (3, 'Carol', 250, NULL, 'vip')"),
        3
    );
    assert_eq!(
        db.count("INSERT INTO cust (cusno, name) VALUES (4, 'Dave')"),
        1
    );
    assert_eq!(
        db.count("INSERT INTO orders VALUES (100, 1, 20, TIMESTAMP '2026-02-01 09:30:00'), (101, 1, 5.25, NULL), (102, 3, 1000, '2026-03-01 00:00:00'), (103, 9, 1, NULL)"),
        4
    );
}

#[test]
fn tables_inserts_and_selects() {
    let mut db = Db::new();
    setup(&mut db);
    // Defaults and nulls landed.
    assert_eq!(
        db.rows("SELECT cusno, name, bal, since, note FROM cust ORDER BY cusno"),
        [
            row(&["1", "Alice", "10.50", "2026-01-15", "first"]),
            row(&["2", "Bob", "0.00", "2025-12-31", "-"]),
            row(&["3", "Carol", "250.00", "-", "vip"]),
            row(&["4", "Dave", "0.00", "-", "-"]),
        ]
    );
    // The described columns.
    let Outcome::Rows(rs) = db.ok("SELECT * FROM cust WHERE 1 = 0") else {
        panic!()
    };
    let names: Vec<&str> = rs.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["CUSNO", "NAME", "BAL", "SINCE", "NOTE"]);
    assert_eq!(rs.columns[0].data_type, "DECIMAL(7, 0)");
    assert_eq!(rs.columns[2].data_type, "NUMERIC(9, 2)");
    assert_eq!(rs.columns[4].data_type, "VARCHAR(100)");
    assert!(rs.columns[3].nullable && !rs.columns[1].nullable);
    // Expressions, aliases, predicates.
    assert_eq!(
        db.rows("SELECT name AS who, bal * 2, bal + 0.5 FROM cust WHERE bal > 5 AND name LIKE '%l%' ORDER BY who DESC"),
        [row(&["Carol", "500.00", "250.50"]), row(&["Alice", "21.00", "11.00"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE since IS NULL ORDER BY 1"),
        [row(&["3"]), row(&["4"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE note IS NOT NULL AND cusno IN (1, 3, 5)"),
        [row(&["1"]), row(&["3"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE bal BETWEEN 1 AND 100"),
        [row(&["1"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE NOT (bal = 0 OR since IS NULL)"),
        [row(&["1"])]
    );
    // Unnamed expressions get positional names.
    let Outcome::Rows(rs) = db.ok("SELECT cusno, bal + 1 FROM cust") else {
        panic!()
    };
    assert_eq!(rs.columns[1].name, "00002");
    // CASE, CAST, functions, registers, dates.
    assert_eq!(
        db.rows("SELECT CASE WHEN bal = 0 THEN 'zero' WHEN bal > 100 THEN 'big' ELSE 'some' END, CAST(bal AS INTEGER), CAST(cusno AS CHAR(3)), SUBSTR(name, 1, 3), LENGTH(TRIM(name)), UPPER(RTRIM(name)) || '!' FROM cust ORDER BY cusno"),
        [
            row(&["some", "10", "1", "Ali", "5", "ALICE!"]),
            row(&["zero", "0", "2", "Bob", "3", "BOB!"]),
            row(&["big", "250", "3", "Car", "5", "CAROL!"]),
            row(&["zero", "0", "4", "Dav", "4", "DAVE!"]),
        ]
    );
    assert_eq!(
        db.rows("SELECT since + 30 DAYS, YEAR(since), MONTH(since), DAYS(since) - DAYS(DATE '2026-01-01'), CHAR(since, USA), DAYNAME(since), since - DATE '2025-01-01' FROM cust WHERE cusno = 1"),
        [row(&["2026-02-14", "2026", "1", "14", "01/15/2026", "Thursday", "10014"])]
    );
    assert_eq!(
        db.rows("SELECT CURRENT DATE, CURRENT USER, COALESCE(note, 'none'), ROUND(bal, -1), MOD(cusno, 2), DIGITS(cusno), HEX(name) FROM cust WHERE cusno = 3"),
        [row(&["2026-01-01", "QSECOFR", "vip", "250", "1", "0000003", &format!("C381999693{}", "40".repeat(25))])]
    );
    assert_eq!(
        db.rows("SELECT 1 + 2 * 3, 7 / 2, 'a' CONCAT 'b', -bal FROM cust WHERE cusno = 1"),
        [row(&["7", "3.500000", "ab", "-10.50"])]
    );
    // Parameters.
    let p = prepare(
        "SELECT name FROM cust WHERE cusno = ? OR name = ?",
        Naming::Sys,
    )
    .unwrap();
    let mut t = db.store.begin().unwrap();
    let Outcome::Rows(rs) = p
        .execute(
            &mut t,
            &mut db.ctx,
            &[Value::int(2), Value::Str("Dave".into())],
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(rs.rows.len(), 2);
    drop(t);
    // DISTINCT, UNION, FETCH FIRST, OFFSET, VALUES.
    assert_eq!(
        db.rows("SELECT DISTINCT bal FROM cust ORDER BY bal"),
        [row(&["0.00"]), row(&["10.50"]), row(&["250.00"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE cusno < 2 UNION ALL SELECT cusno FROM orders WHERE cusno = 1 UNION SELECT 99 FROM cust WHERE cusno = 1 ORDER BY 1"),
        [row(&["1"]), row(&["99"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust ORDER BY cusno DESC OFFSET 1 ROW FETCH FIRST 2 ROWS ONLY"),
        [row(&["3"]), row(&["2"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust ORDER BY cusno LIMIT 1"),
        [row(&["1"])]
    );
    assert_eq!(
        db.rows("VALUES (1, 'x'), (2, 'y')"),
        [row(&["1", "x"]), row(&["2", "y"])]
    );
    assert_eq!(
        db.rows("SELECT * FROM (SELECT cusno AS c, bal FROM cust) AS t WHERE c > 2 ORDER BY c"),
        [row(&["3", "250.00"]), row(&["4", "0.00"])]
    );
    // RRN and a nulls-first ordering.
    assert_eq!(
        db.rows("SELECT RRN(cust), cusno FROM cust WHERE cusno = 3"),
        [row(&["3", "3"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust ORDER BY since DESC, cusno"),
        [row(&["3"]), row(&["4"]), row(&["1"]), row(&["2"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust ORDER BY since NULLS LAST, cusno"),
        [row(&["2"]), row(&["1"]), row(&["3"]), row(&["4"])]
    );
}

#[test]
fn joins_groups_and_subqueries() {
    let mut db = Db::new();
    setup(&mut db);
    assert_eq!(
        db.rows(
            "SELECT c.name, o.ordno FROM cust c JOIN orders o ON c.cusno = o.cusno ORDER BY o.ordno"
        ),
        [
            row(&["Alice", "100"]),
            row(&["Alice", "101"]),
            row(&["Carol", "102"])
        ]
    );
    assert_eq!(
        db.rows("SELECT c.cusno, COUNT(o.ordno), SUM(o.amount) FROM cust c LEFT JOIN orders o ON c.cusno = o.cusno GROUP BY c.cusno ORDER BY 1"),
        [row(&["1", "2", "25.25"]), row(&["2", "0", "-"]), row(&["3", "1", "1000.00"]), row(&["4", "0", "-"])]
    );
    assert_eq!(
        db.rows("SELECT o.ordno FROM orders o LEFT JOIN cust c ON c.cusno = o.cusno WHERE c.cusno IS NULL"),
        [row(&["103"])]
    );
    assert_eq!(
        db.rows(
            "SELECT c.cusno FROM cust c EXCEPTION JOIN orders o ON c.cusno = o.cusno ORDER BY 1"
        ),
        [row(&["2"]), row(&["4"])]
    );
    assert_eq!(
        db.rows("SELECT o.ordno FROM cust c RIGHT EXCEPTION JOIN orders o ON c.cusno = o.cusno"),
        [row(&["103"])]
    );
    assert_eq!(
        db.rows("SELECT c.cusno, o.ordno FROM cust c FULL OUTER JOIN orders o ON c.cusno = o.cusno ORDER BY 1, 2"),
        [
            row(&["1", "100"]),
            row(&["1", "101"]),
            row(&["2", "-"]),
            row(&["3", "102"]),
            row(&["4", "-"]),
            row(&["-", "103"])
        ]
    );
    // Common table expressions, in FROM and in a subquery.
    assert_eq!(
        db.rows("WITH big (who, total) AS (SELECT cusno, SUM(amount) FROM orders GROUP BY cusno) SELECT c.name, b.total FROM cust c JOIN big b ON b.who = c.cusno WHERE b.total > 10 ORDER BY 1"),
        [row(&["Alice", "25.25"]), row(&["Carol", "1000.00"])]
    );
    assert_eq!(
        db.rows("WITH rich AS (SELECT cusno FROM cust WHERE bal > 100) SELECT ordno FROM orders WHERE cusno IN (SELECT cusno FROM rich)"),
        [row(&["102"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM cust, orders"), [row(&["16"])]);
    assert_eq!(
        db.rows("SELECT c.name FROM cust c, orders o WHERE c.cusno = o.cusno AND o.amount > 100"),
        [row(&["Carol"])]
    );
    // Aggregates with and without groups, HAVING.
    assert_eq!(
        db.rows("SELECT COUNT(*), COUNT(since), SUM(bal), AVG(bal), MIN(name), MAX(bal) FROM cust"),
        [row(&["4", "2", "260.50", "65.12500000", "Alice", "250.00"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM cust WHERE cusno > 100"),
        [row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT cusno, SUM(amount) FROM orders GROUP BY cusno HAVING SUM(amount) > 10 ORDER BY cusno"),
        [row(&["1", "25.25"]), row(&["3", "1000.00"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(DISTINCT cusno) FROM orders"),
        [row(&["3"])]
    );
    // Subqueries: scalar, IN, EXISTS, correlated, quantified.
    assert_eq!(db.rows("SELECT name FROM cust WHERE cusno IN (SELECT cusno FROM orders WHERE amount > 10) ORDER BY name"), [row(&["Alice"]), row(&["Carol"])]);
    assert_eq!(db.rows("SELECT name FROM cust c WHERE EXISTS (SELECT 1 FROM orders o WHERE o.cusno = c.cusno AND o.amount < 10)"), [row(&["Alice"])]);
    assert_eq!(db.rows("SELECT name FROM cust c WHERE NOT EXISTS (SELECT 1 FROM orders o WHERE o.cusno = c.cusno) ORDER BY 1"), [row(&["Bob"]), row(&["Dave"])]);
    assert_eq!(
        db.rows("SELECT name, (SELECT SUM(amount) FROM orders o WHERE o.cusno = c.cusno) FROM cust c ORDER BY cusno"),
        [row(&["Alice", "25.25"]), row(&["Bob", "-"]), row(&["Carol", "1000.00"]), row(&["Dave", "-"])]
    );
    assert_eq!(
        db.rows(
            "SELECT name FROM cust WHERE bal > ALL (SELECT amount FROM orders WHERE cusno = 1)"
        ),
        [row(&["Carol"])]
    );
    assert_eq!(
        db.rows("SELECT name FROM cust WHERE bal = (SELECT MAX(bal) FROM cust)"),
        [row(&["Carol"])]
    );
    assert_eq!(
        db.code("SELECT name FROM cust WHERE bal = (SELECT bal FROM cust)"),
        -811
    );
}

#[test]
fn external_sort_spills_and_merges() {
    let mut db = Db::new();
    setup(&mut db);
    db.ctx.sort_budget = 2;
    let want = [
        row(&["Carol"]),
        row(&["Alice"]),
        row(&["Bob"]),
        row(&["Dave"]),
    ];
    // Four rows, two per run: two runs merged in memory.
    assert_eq!(
        db.rows("SELECT name FROM cust ORDER BY bal DESC, name"),
        want
    );
    // The same through files in a directory.
    let dir = std::env::temp_dir().join(format!("qsql-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    db.ctx.spill = Box::new(sql::exec::spill::FileSpill::new(dir.clone()));
    assert_eq!(
        db.rows("SELECT name FROM cust ORDER BY bal DESC, name"),
        want
    );
    assert_eq!(
        db.rows("SELECT cusno, name FROM cust WHERE bal >= 0 ORDER BY 2 DESC"),
        [
            row(&["4", "Dave"]),
            row(&["3", "Carol"]),
            row(&["2", "Bob"]),
            row(&["1", "Alice"])
        ]
    );
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "runs are removed"
    );
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn opnqryf_selects_and_orders() {
    use db::odp::{OpenOpts, Usage};
    use sql::ast::{Ident, ObjectName};
    use sql::exec::opnqryf::{OpnQryF, clof, opnqryf};
    let mut db = Db::new();
    setup(&mut db);
    let cust = ObjectName {
        schema: None,
        name: Ident::new("CUST"),
    };
    let mut t = db.store.begin().unwrap();
    let n = opnqryf(
        &mut t,
        &mut db.ctx,
        &OpnQryF {
            file: cust.clone(),
            qryslt: "BAL *GE 0 *AND NAME *NE \"Dave\"".into(),
            keyfld: vec![("BAL".into(), true), ("NAME".into(), false)],
            usage: Usage::INPUT,
        },
    )
    .unwrap();
    assert_eq!(n, 3);
    // The program's open with SHARE(*YES) reads the query's records.
    let file_id = db.ctx.resolve_file(&mut t, &cust).unwrap();
    let fr = dds::FileRef::parse("CUST").unwrap();
    let o = db
        .ctx
        .session
        .open(
            &mut t,
            &fr,
            &mut |_| Ok(file_id),
            OpenOpts {
                share: true,
                ..OpenOpts::input(JobHandle(1))
            },
        )
        .unwrap();
    let mut names = Vec::new();
    loop {
        let next = o.borrow_mut().read(&mut t, false).unwrap();
        let Some(r) = next else { break };
        let v = o.borrow().field(&r, "NAME").unwrap();
        match v {
            db::Value::Char(b) => names.push(ccsid::decode_trimmed(&b)),
            v => panic!("{v:?}"),
        }
    }
    assert_eq!(names, ["Carol", "Alice", "Bob"]);
    // Positioning by the KEYFLD key, and a second query is refused.
    o.borrow_mut()
        .setll(
            &mut t,
            &db::KeyArg::one(db::Value::Num("10.50".parse().unwrap())),
        )
        .unwrap();
    assert!(o.borrow().fb.equal);
    let r = o.borrow_mut().read(&mut t, false).unwrap().unwrap();
    assert_eq!(
        o.borrow().field(&r, "CUSNO").unwrap(),
        db::Value::Num("1".parse().unwrap())
    );
    assert!(
        opnqryf(
            &mut t,
            &mut db.ctx,
            &OpnQryF {
                file: cust.clone(),
                qryslt: "*ALL".into(),
                keyfld: Vec::new(),
                usage: Usage::INPUT,
            },
        )
        .is_err()
    );
    db.ctx.session.close(&mut t, &o);
    assert_eq!(clof(&mut t, &mut db.ctx, &cust).unwrap(), 1);
    assert_eq!(clof(&mut t, &mut db.ctx, &cust).unwrap(), 0);
    t.commit().unwrap();
}

#[test]
fn identity_defaults_and_alter_table() {
    let mut db = Db::new();
    db.ok("CREATE TABLE qstest/log (id INTEGER GENERATED ALWAYS AS IDENTITY (START WITH 10 INCREMENT BY 5), note CHAR(10) NOT NULL, at DATE NOT NULL DEFAULT CURRENT DATE, who CHAR(10) NOT NULL DEFAULT USER)");
    assert_eq!(db.count("INSERT INTO log (note) VALUES ('a'), ('b')"), 2);
    assert_eq!(
        db.count("INSERT INTO log (note, who) VALUES ('c', 'ME')"),
        1
    );
    assert_eq!(db.code("INSERT INTO log (id, note) VALUES (1, 'x')"), -798);
    assert_eq!(
        db.rows("SELECT id, note, who, at = CURRENT DATE FROM log ORDER BY id"),
        [
            row(&["10", "a", "QSECOFR", "1"]),
            row(&["15", "b", "QSECOFR", "1"]),
            row(&["20", "c", "ME", "1"])
        ]
    );
    // ALTER TABLE: a column added with a default, one dropped, a type
    // widened, a unique constraint added and dropped.
    db.ok("ALTER TABLE qstest/log ADD COLUMN qty DECIMAL(5, 0) NOT NULL DEFAULT 7");
    db.ok("ALTER TABLE qstest/log DROP COLUMN who");
    db.ok("ALTER TABLE qstest/log ALTER COLUMN note SET DATA TYPE CHAR(20)");
    assert_eq!(
        db.rows("SELECT id, note, qty FROM log ORDER BY id"),
        [
            row(&["10", "a", "7"]),
            row(&["15", "b", "7"]),
            row(&["20", "c", "7"])
        ]
    );
    assert_eq!(
        db.count("INSERT INTO log (note, qty) VALUES ('twenty characters!!!', 1)"),
        1
    );
    assert_eq!(db.rows("SELECT MAX(id) FROM log"), [row(&["25"])]);
    db.ok("ALTER TABLE qstest/log ADD CONSTRAINT onenote UNIQUE (note)");
    assert_eq!(db.code("INSERT INTO log (note, qty) VALUES ('a', 1)"), -803);
    db.ok("ALTER TABLE qstest/log DROP CONSTRAINT onenote");
    assert_eq!(db.count("INSERT INTO log (note, qty) VALUES ('a', 1)"), 1);
    assert_eq!(
        db.code("ALTER TABLE qstest/log DROP CONSTRAINT nosuch"),
        -204
    );
    db.ok("DROP TABLE qstest/log");
}

#[test]
fn indexes_and_plans() {
    let mut db = Db::new();
    setup(&mut db);
    // The primary key is the file's own path.
    let p = db.plan("SELECT * FROM cust WHERE cusno = 3");
    assert!(p.contains("keyed via QSTEST/CUST own key eq=1"), "{p}");
    let p = db.plan("SELECT * FROM cust WHERE cusno BETWEEN 2 AND 3 ORDER BY cusno");
    assert!(
        p.contains("range=yes") && p.contains("ordered") && p.contains("ORDER BY satisfied"),
        "{p}"
    );
    let p = db.plan("SELECT * FROM cust WHERE name = 'Bob'");
    assert!(
        p.contains("SCAN QSTEST/CUST (CUST) full") && !p.contains("SORT"),
        "{p}"
    );
    // An index becomes a keyed logical file the planner uses.
    db.ok("CREATE INDEX qstest/cusbyname ON cust (name)");
    let p = db.plan("SELECT * FROM cust WHERE name = 'Bob'");
    assert!(
        p.contains("keyed via QSTEST/CUSBYNAME (CUSBYNAME) eq=1"),
        "{p}"
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE name = 'Bob'"),
        [row(&["2"])]
    );
    assert_eq!(
        db.rows("SELECT cusno FROM cust WHERE name >= 'Carol' ORDER BY name"),
        [row(&["3"]), row(&["4"])]
    );
    assert_eq!(
        db.rows("SELECT name FROM cust ORDER BY name DESC"),
        [
            row(&["Dave"]),
            row(&["Carol"]),
            row(&["Bob"]),
            row(&["Alice"])
        ]
    );
    let p = db.plan("SELECT name FROM cust ORDER BY name DESC");
    assert!(p.contains("ordered"), "{p}");
    // The key order is kept with a predicate on another column too.
    assert_eq!(
        db.rows("SELECT name FROM cust WHERE bal >= 0 ORDER BY name"),
        [
            row(&["Alice"]),
            row(&["Bob"]),
            row(&["Carol"]),
            row(&["Dave"])
        ]
    );
    // A UNIQUE constraint is a unique index the catalog lists.
    db.ok("CREATE TABLE qstest/tags (id INTEGER NOT NULL PRIMARY KEY, tag CHAR(10) NOT NULL UNIQUE, grp CHAR(5), CONSTRAINT grptag UNIQUE (grp, tag))");
    assert_eq!(
        db.count("INSERT INTO tags VALUES (1, 'a', 'x'), (2, 'b', 'x')"),
        2
    );
    assert_eq!(db.code("INSERT INTO tags VALUES (3, 'a', 'y')"), -803);
    assert_eq!(
        db.rows("SELECT constraint_name, constraint_type FROM qsys2/syscst WHERE table_name = 'TAGS' ORDER BY 1"),
        [row(&["GRPTAG", "UNIQUE"]), row(&["Q_QSTEST_TAGS_ID_00001", "PRIMARY KEY"]), row(&["Q_TAGS_1", "UNIQUE"])]
    );
    db.ok("DROP TABLE qstest/tags");
    // A unique index refuses a duplicate.
    db.ok("CREATE UNIQUE INDEX qstest/cusnote ON cust (note)");
    assert_eq!(
        db.code("INSERT INTO cust (cusno, name, note) VALUES (5, 'Eve', 'vip')"),
        -803
    );
    assert_eq!(
        db.count("INSERT INTO cust (cusno, name, note) VALUES (5, 'Eve', 'new')"),
        1
    );
    // Joins read the join column through the index when it is the inner table.
    let p = db.plan("SELECT * FROM orders o JOIN cust c ON c.cusno = o.cusno");
    assert!(p.contains("JOIN Inner nested loop"), "{p}");
    // Dropping the table takes its indexes.
    db.ok("DROP TABLE qstest/cust");
    assert_eq!(db.code("SELECT * FROM cust"), -204);
    assert_eq!(db.code("SELECT * FROM cusbyname"), -204);
    assert_eq!(db.code("DROP INDEX cusbyname"), -204);
}

#[test]
fn updates_deletes_and_errors() {
    let mut db = Db::new();
    setup(&mut db);
    assert_eq!(
        db.count("UPDATE cust SET bal = bal + 1, note = 'bumped' WHERE cusno <= 2"),
        2
    );
    assert_eq!(
        db.rows("SELECT cusno, bal, note FROM cust WHERE cusno <= 2 ORDER BY cusno"),
        [
            row(&["1", "11.50", "bumped"]),
            row(&["2", "1.00", "bumped"])
        ]
    );
    assert_eq!(
        db.count("UPDATE cust SET (bal, since) = (DEFAULT, NULL) WHERE name = 'Alice'"),
        1
    );
    assert_eq!(
        db.rows("SELECT bal, since FROM cust WHERE cusno = 1"),
        [row(&["0.00", "-"])]
    );
    assert_eq!(db.count("UPDATE cust SET bal = 99 WHERE cusno = 42"), 0);
    assert_eq!(
        db.count("DELETE FROM cust WHERE note IS NULL AND cusno > 3"),
        1
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM cust"), [row(&["3"])]);
    assert_eq!(db.count("INSERT INTO cust (cusno, name) SELECT ordno, 'from order' FROM orders WHERE amount >= 1000"), 1);
    assert_eq!(
        db.rows("SELECT name FROM cust WHERE cusno = 102"),
        [row(&["from order"])]
    );
    assert_eq!(db.count("DELETE FROM orders"), 4);
    // Errors with their codes.
    assert_eq!(db.code("SELECT * FROM nosuch"), -204);
    assert_eq!(db.code("SELECT nosuch FROM cust"), -206);
    assert_eq!(
        db.code("SELECT cusno FROM cust c, orders o WHERE cusno = 1"),
        -203
    );
    assert_eq!(
        db.code("INSERT INTO cust (cusno, name) VALUES (1, 'dup')"),
        -803
    );
    assert_eq!(db.code("INSERT INTO cust (cusno) VALUES (7)"), -407);
    assert_eq!(
        db.code("INSERT INTO cust (cusno, name) VALUES ('x', 'bad')"),
        -302
    );
    assert_eq!(
        db.code("INSERT INTO cust (cusno, name) VALUES (12345678, 'big')"),
        -802
    );
    assert_eq!(db.code("INSERT INTO cust (cusno, name) VALUES (8)"), -117);
    assert_eq!(db.code("SELECT cusno FROM cust WHERE name = 1"), -401);
    assert_eq!(db.code("SELECT 1 / 0 FROM cust"), -802);
    assert_eq!(db.code("SELECT NOSUCHFN(1) FROM cust"), -84);
    assert_eq!(db.code("CREATE TABLE cust (a INT)"), -901);
    // A row through RPG's path and SQL's look the same to the journal.
    db.ok("CREATE TABLE qstest/jt (k INT NOT NULL PRIMARY KEY, v CHAR(5))");
    let mut t = db.store.begin().unwrap();
    let lib = db.ctx.names.curlib.unwrap();
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        lib,
        Name::parse("JTRCV").unwrap(),
        &jrn::rcv::CrtJrnRcvOpts::default(),
    )
    .unwrap();
    let jid = jrn::journal::crtjrn(
        &mut t,
        lib,
        Name::parse("JTJRN").unwrap(),
        rcv,
        &jrn::journal::CrtJrnOpts::default(),
    )
    .unwrap();
    let jt = db
        .ctx
        .resolve_file(&mut t, &sql::ast::ObjectName::new("JT"))
        .unwrap();
    db::journal::strjrnpf(&mut t, JobHandle(1), jt, jid, true, true, 1).unwrap();
    t.commit().unwrap();
    assert_eq!(db.count("INSERT INTO jt VALUES (1, 'a'), (2, 'b')"), 2);
    assert_eq!(db.count("UPDATE jt SET v = 'c' WHERE k = 1"), 1);
    assert_eq!(db.count("DELETE FROM jt WHERE k = 2"), 1);
    let mut t = db.store.begin().unwrap();
    let entries = jrn::dspjrn(&mut t, jid, &jrn::Filter::default()).unwrap();
    let kinds: Vec<String> = entries.iter().map(jrn::Entry::type_name).collect();
    assert_eq!(kinds, ["F JM", "R PT", "R PT", "R UB", "R UP", "R DL"]);
    assert_eq!(entries[1].job.number, 1);
}

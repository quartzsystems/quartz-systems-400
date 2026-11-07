// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The planner of Section 7: join methods by cost with the nested
//! loop's answers, column statistics and `SYSCOLUMNSTAT`, predicates
//! pushed into views and common table expressions, subqueries
//! decorrelated, row goals, index-only access, several indexes at once,
//! `EXPLAIN`, the plan cache and the query options' limits.

#![allow(clippy::too_many_lines, dead_code)]

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

/// `n` rows of `qstest/big`: id, grp = id % 50, v = id % 7, name.
fn big(db: &mut Db, n: u64) {
    db.ok("CREATE TABLE qstest/big (id INTEGER NOT NULL PRIMARY KEY, grp INTEGER NOT NULL, v INTEGER, name CHAR(10) NOT NULL)");
    let mut i = 1;
    while i <= n {
        let end = (i + 499).min(n);
        let vals: Vec<String> = (i..=end)
            .map(|k| {
                format!(
                    "({k}, {}, {}, 'N{}')",
                    k % 50,
                    if k % 10 == 0 {
                        "NULL".into()
                    } else {
                        (k % 7).to_string()
                    },
                    k % 13
                )
            })
            .collect();
        db.ok(&format!(
            "INSERT INTO qstest/big VALUES {}",
            vals.join(", ")
        ));
        i = end + 1;
    }
}

fn small(db: &mut Db) {
    db.ok("CREATE TABLE qstest/grps (grp INTEGER NOT NULL, label CHAR(10) NOT NULL)");
    let vals: Vec<String> = (0..60).rev().map(|g| format!("({g}, 'G{g}')")).collect();
    db.ok(&format!(
        "INSERT INTO qstest/grps VALUES {}",
        vals.join(", ")
    ));
}

#[test]
fn join_methods_give_the_nested_loops_answer() {
    let mut db = Db::new();
    big(&mut db, 3000);
    small(&mut db);
    // `+ 0` hides the equality: a nested loop.
    let loop_rows = db
        .rows("SELECT b.id, g.label FROM big b JOIN grps g ON g.grp = b.grp + 0 WHERE b.id <= 400");
    let hashed =
        db.rows("SELECT b.id, g.label FROM big b JOIN grps g ON g.grp = b.grp WHERE b.id <= 400");
    assert_eq!(loop_rows.len(), 400);
    assert_eq!(loop_rows, hashed);
    assert!(
        db.plan("SELECT b.id, g.label FROM big b JOIN grps g ON g.grp = b.grp")
            .contains("hash join")
    );
    assert!(
        db.plan("SELECT b.id FROM big b JOIN grps g ON g.grp = b.grp + 0")
            .contains("nested loop")
    );
    // Outer joins keep their unmatched rows in the same places.
    let l1 = db
        .rows("SELECT g.grp, b.id FROM grps g LEFT JOIN big b ON b.grp + 0 = g.grp AND b.id < 120");
    let l2 =
        db.rows("SELECT g.grp, b.id FROM grps g LEFT JOIN big b ON b.grp = g.grp AND b.id < 120");
    assert_eq!(l1, l2);
    let r1 = db.rows(
        "SELECT g.grp, b.id FROM big b RIGHT JOIN grps g ON b.grp + 0 = g.grp AND b.id < 120",
    );
    let r2 =
        db.rows("SELECT g.grp, b.id FROM big b RIGHT JOIN grps g ON b.grp = g.grp AND b.id < 120");
    assert_eq!(r1, r2);
    // Both sides in key order: a sort-merge join, the same rows.
    let m1 =
        db.rows("SELECT a.id, c.id FROM big a JOIN big c ON c.id = a.id + 0 WHERE a.id <= 2000");
    let m2 = db.rows("SELECT a.id, c.id FROM big a JOIN big c ON c.id = a.id WHERE a.id <= 2000");
    assert_eq!(m1, m2);
    // Comma joins take the hash join too.
    let c1 =
        db.rows("SELECT b.id, g.label FROM big b, grps g WHERE g.grp = b.grp + 0 AND b.id <= 300");
    let c2 = db.rows("SELECT b.id, g.label FROM big b, grps g WHERE g.grp = b.grp AND b.id <= 300");
    assert_eq!(c1, c2);
}

#[test]
fn statistics_after_create_index() {
    let mut db = Db::new();
    big(&mut db, 2000);
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM qsys2/syscolumnstat WHERE TABLE_NAME = 'BIG' AND STATISTICS_NAME IS NOT NULL"),
        [row(&["0"])]
    );
    db.ok("CREATE INDEX qstest/bigv ON big (v, grp)");
    let r = db.rows(
        "SELECT COLUMN_NAME, NUMBER_DISTINCT_VALUES, NUMBER_NULLS, NUMBER_COLUMN_NAMES, STATISTIC_CREATOR, STATISTICS_ROWS FROM qsys2/syscolumnstat          WHERE TABLE_NAME = 'BIG' AND STATISTICS_NAME IS NOT NULL ORDER BY COLUMN_NAME",
    );
    assert_eq!(
        r,
        [
            row(&["GRP", "50", "0", "1", "*SYS", "2000"]),
            row(&["V", "7", "200", "1", "*SYS", "2000"])
        ]
    );
    let n = db.rows("SELECT STATISTICS_NAME FROM qsys2/syscolumnstat WHERE TABLE_NAME = 'BIG' AND COLUMN_NAME = 'V'");
    assert!(
        n[0][0].starts_with("QDBST_") && n[0][0].len() == 38,
        "{n:?}"
    );
}

#[test]
fn predicates_go_into_views_and_ctes() {
    let mut db = Db::new();
    big(&mut db, 1000);
    db.ok(
        "CREATE VIEW qstest/bigview AS SELECT id AS ident, grp, name FROM big WHERE v IS NOT NULL",
    );
    let p = db.plan("SELECT * FROM bigview WHERE ident = 77");
    assert!(p.contains("PREDICATES pushed") && p.contains("eq=1"), "{p}");
    assert_eq!(
        db.rows("SELECT ident, name FROM bigview WHERE ident = 77"),
        [row(&["77", "N12"])]
    );
    assert_eq!(
        db.rows("SELECT ident FROM bigview WHERE ident = 70"),
        Vec::<Vec<String>>::new()
    );
    let cte =
        "WITH t (k, g) AS (SELECT id, grp FROM big) SELECT k FROM t WHERE k BETWEEN 10 AND 12";
    assert!(db.plan(cte).contains("PREDICATES pushed"));
    assert_eq!(db.rows(cte), [row(&["10"]), row(&["11"]), row(&["12"])]);
    // Through GROUP BY only on a grouping column; never on an aggregate.
    let g = "SELECT grp, n FROM (SELECT grp, COUNT(*) AS n FROM big GROUP BY grp) x WHERE grp = 3 AND n > 1";
    assert_eq!(db.rows(g), [row(&["3", "20"])]);
    assert_eq!(
        db.rows("SELECT n FROM (SELECT COUNT(*) AS n FROM big) x WHERE n > 5"),
        [row(&["1000"])]
    );
    // A name two derived tables have stays ambiguous.
    assert_eq!(
        db.code("SELECT * FROM (SELECT id FROM big) a, (SELECT id FROM big) b WHERE id = 1"),
        -203
    );
}

#[test]
fn subqueries_run_once() {
    let mut db = Db::new();
    big(&mut db, 1500);
    small(&mut db);
    let q = "SELECT g.grp, (SELECT COUNT(*) FROM big b WHERE b.grp = g.grp), (SELECT MAX(b.id) FROM big b WHERE b.grp = g.grp) FROM grps g WHERE g.grp >= 45";
    let r = db.rows(q);
    assert_eq!(r.len(), 15);
    // Groups 50 to 59 have no rows: the aggregate over none.
    assert_eq!(r[0], row(&["59", "0", "-"]));
    assert_eq!(r[14], row(&["45", "30", "1495"]));
    // `b.grp - g.grp = 0` correlates otherwise: run per row, the same.
    let e = db.rows("SELECT g.grp FROM grps g WHERE EXISTS (SELECT 1 FROM big b WHERE b.grp = g.grp AND b.v = 3)");
    let per_row = db.rows("SELECT g.grp FROM grps g WHERE EXISTS (SELECT 1 FROM big b WHERE b.grp - g.grp = 0 AND b.v = 3)");
    assert_eq!(e, per_row);
    assert_eq!(e.len(), 45);
    let ne = db.rows(
        "SELECT COUNT(*) FROM grps g WHERE NOT EXISTS (SELECT 1 FROM big b WHERE b.grp = g.grp)",
    );
    assert_eq!(ne, [row(&["10"])]);
    let i = db.rows("SELECT g.grp FROM grps g WHERE 'N1' IN (SELECT b.name FROM big b WHERE b.grp = g.grp) AND g.grp < 3");
    assert_eq!(i, [row(&["2"]), row(&["1"]), row(&["0"])]);
    // A scalar subquery with more than one row for a key still fails.
    assert_eq!(
        db.code("SELECT (SELECT b.id FROM big b WHERE b.grp = g.grp) FROM grps g"),
        -811
    );
    // Uncorrelated: the same answer for every row.
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM grps WHERE grp IN (SELECT v FROM big)"),
        [row(&["7"])]
    );
}

#[test]
fn row_goals_index_only_and_several_indexes() {
    let mut db = Db::new();
    big(&mut db, 2000);
    assert_eq!(
        db.rows("SELECT id FROM big ORDER BY id OPTIMIZE FOR ALL ROWS FETCH FIRST 1 ROW ONLY"),
        [row(&["1"])]
    );
    assert_eq!(db.code("SELECT id FROM big OPTIMIZE FOR 0 ROWS"), -405);
    // A key-ordered read stops at the rows asked for.
    let first = db.rows("SELECT id, name FROM big WHERE v = 3 ORDER BY id FETCH FIRST 3 ROWS ONLY");
    assert_eq!(
        first,
        [row(&["3", "N3"]), row(&["17", "N4"]), row(&["24", "N11"])]
    );
    assert_eq!(
        db.rows(
            "SELECT id FROM big WHERE id > 10 ORDER BY id OFFSET 2 ROWS FETCH FIRST 2 ROWS ONLY"
        ),
        [row(&["13"]), row(&["14"])]
    );
    assert_eq!(
        db.rows("SELECT id FROM big WHERE id > 1990 OPTIMIZE FOR 2 ROWS"),
        (1991..=2000)
            .map(|i| row(&[&i.to_string()]))
            .collect::<Vec<_>>()
    );
    // Only key fields: read from the path.
    db.ok("CREATE INDEX qstest/bigname ON big (name, grp)");
    let p = db.plan("SELECT name, grp FROM big WHERE name = 'N3' AND grp > 40");
    assert!(p.contains("index only"), "{p}");
    let via_keys = db.rows("SELECT name, grp FROM big WHERE name = 'N3' AND grp > 40");
    let via_rows = db.rows("SELECT name, grp, id FROM big WHERE name = 'N3' AND grp > 40");
    assert_eq!(
        via_keys,
        via_rows.iter().map(|r| r[..2].to_vec()).collect::<Vec<_>>()
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM big WHERE name = 'N3'"),
        [row(&["154"])]
    );
    // A disjunction over two paths: their record numbers, read in arrival order.
    db.ok("CREATE INDEX qstest/bigv ON big (v)");
    let p = db.plan("SELECT id FROM big WHERE v = 6 OR name = 'N0'");
    assert!(p.contains("bitmap over 2 ranges"), "{p}");
    let bitmap = db.rows("SELECT id FROM big WHERE v = 6 OR name = 'N0'");
    let full = db.rows("SELECT id FROM big WHERE v + 0 = 6 OR name || '' = 'N0'");
    assert_eq!(bitmap, full);
    let inlist = db.rows("SELECT id FROM big WHERE v IN (1, 2) AND id < 40");
    let inlist_full = db.rows("SELECT id FROM big WHERE v + 0 IN (1, 2) AND id < 40");
    assert_eq!(inlist, inlist_full);
}

#[test]
fn explain_plan_cache_and_query_options() {
    let mut db = Db::new();
    big(&mut db, 100);
    let r = db.rows("EXPLAIN SELECT id FROM big WHERE id = 4");
    assert_eq!(
        r[0][1..],
        ["3001".to_string(), "I1".into(), "-".into()],
        "{r:?}"
    );
    let r = db.rows("EXPLAIN SELECT id FROM big WHERE name = 'x' ORDER BY name");
    assert!(
        r.iter().any(|x| x[1] == "3000") && r.iter().any(|x| x[1] == "3003"),
        "{r:?}"
    );
    // The plan cache: statements prepared, written to a monitor file.
    db.ok("CALL QSYS2.CLEAR_PLAN_CACHE()");
    db.rows("SELECT COUNT(*) FROM big");
    db.rows("SELECT COUNT(*) FROM big");
    db.ok("CALL QSYS2.DUMP_PLAN_CACHE('QSTEST', 'PCDUMP')");
    let d = db.rows("SELECT QQRID, QQC21, QQ1000 FROM qstest/pcdump WHERE QQ1000 LIKE 'SELECT COUNT(*) FROM big%'");
    assert_eq!(d, [row(&["1000", "SI", "SELECT COUNT(*) FROM big"])]);
    db.ok("CALL QSYS2.CHANGE_PLAN_CACHE_SIZE(64)");
    assert_eq!(
        db.code("CALL QSYS2.DUMP_PLAN_CACHE('QSTEST', 'PCDUMP', PLAN_IDENTIFIER => 5)"),
        -84
    );
    // The query governor: a time limit of 0 stops every query.
    db.ok("CALL QSYS2.OVERRIDE_QAQQINI(1, '', '')");
    db.ok("CALL QSYS2.OVERRIDE_QAQQINI(2, 'QUERY_TIME_LIMIT', '0')");
    assert_eq!(db.code("SELECT COUNT(*) FROM big"), -666);
    db.ok("CALL QSYS2.OVERRIDE_QAQQINI(2, 'QUERY_TIME_LIMIT', '*NOMAX')");
    db.ok("CALL QSYS2.OVERRIDE_QAQQINI(2, 'QUERY_STORAGE_LIMIT', '0')");
    assert_eq!(db.rows("SELECT COUNT(*) FROM big"), [row(&["100"])]);
    db.ok("CALL QSYS2.OVERRIDE_QAQQINI(3, '', '')");
}

#[test]
fn statistics_outlive_an_ipl() {
    // The store on a device kept for the second run.
    let d = Arc::new(MemBlockDev::with_seed(16384, 12));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
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
    let ctx = move || {
        let mut c = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        c.now = Some(1_767_225_600);
        c
    };
    let q = "SELECT COLUMN_NAME, NUMBER_DISTINCT_VALUES, NUMBER_NULLS, STATISTICS_ROWS, STATISTICS_NAME FROM qsys2/syscolumnstat WHERE TABLE_NAME = 'BIG' AND STATISTICS_NAME IS NOT NULL ORDER BY COLUMN_NAME";
    let mut db = Db { store, ctx: ctx() };
    big(&mut db, 2000);
    db.ok("CREATE INDEX qstest/bigv ON big (v, grp)");
    let before = db.rows(q);
    assert_eq!(before.len(), 2, "{before:?}");
    let plan = db.plan("SELECT * FROM qstest/big WHERE v = 3");
    drop(db);
    // A new thread is a new service: its statistics table starts empty
    // and reads the kept ones from the reopened store.
    let (after, replan) = std::thread::spawn(move || {
        let store = Store::open(
            vec![Box::new(d) as Box<dyn BlockDev>],
            StoreConfig::default(),
        )
        .unwrap()
        .0;
        let mut db = Db { store, ctx: ctx() };
        (db.rows(q), db.plan("SELECT * FROM qstest/big WHERE v = 3"))
    })
    .join()
    .unwrap();
    assert_eq!(after, before);
    assert_eq!(replan, plan);
}

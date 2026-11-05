// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL benchmarks of `docs/SQL.md` §11, on an in-memory store on the
//! host: a 1,000,000-row `ORDER BY`, a three-way join, a recursive
//! common table expression, a windowed aggregate, an SQL procedure that
//! calls another 10,000 times, a 100 MB CLOB written and read back.
//! Ignored by default; `cargo run -p build -- bench` runs them with
//! `cargo test --release -p conformance --test sql_bench -- --ignored
//! --nocapture`. `QS400_SQL_BENCH_ROWS=N` scales the large table
//! (default 1,000,000).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, Outcome, SqlContext, Value, prepare_dynamic};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
/// 2 GiB of blocks: the large table, its index and the CLOB.
const BLOCKS: u64 = 524_288;

struct Shared(Rc<RefCell<Session>>);

impl Embedded for Shared {
    fn run(&mut self, req: &Request) -> Reply {
        self.0.borrow_mut().run(req)
    }
}

struct StoreSource {
    store: Rc<RefCell<Store>>,
    lib: ObjectId,
}

impl mi::act::ObjectSource for StoreSource {
    fn program(&mut self, name: &str, _lib: &str, kind: ile::PgmKind) -> Option<(Arc<ile::ProgramObject>, String)> {
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().ok()?;
        let ty = if kind == ile::PgmKind::Pgm { types::PGM } else { types::SRVPGM };
        let l = store::obj::library::Library::load(&mut txn, self.lib).ok()?;
        let id = l.lookup(&mut txn, ty, Name::parse(name).ok()?).ok()??;
        let p = ile::store::load_program(&mut txn, id).ok()?;
        txn.abort();
        Some((Arc::new(p), "QSTEST".into()))
    }
}

struct Bench {
    store: Rc<RefCell<Store>>,
    session: Rc<RefCell<Session>>,
    job: Box<mi::Job>,
}

impl Bench {
    fn new() -> Self {
        let dev = Arc::new(MemBlockDev::with_seed(BLOCKS, 7));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 65_536,
            now: NOW,
        };
        let mut store = Store::format(vec![Box::new(dev) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
        let mut txn = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        let lib = ops::create(
            &mut txn,
            &CreateParams {
                library: qsys,
                name: Name::parse("QSTEST").unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: "SQL benchmarks".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
        .unwrap();
        txn.commit().unwrap();
        let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
        ctx.now = Some(NOW);
        ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
        // The large table's rows are held at once by the sort and the join.
        ctx.memory_limit = 4 << 30;
        let store = Rc::new(RefCell::new(store));
        let session = Rc::new(RefCell::new(Session::with_ctx(Box::new(Rc::clone(&store)), ctx)));
        let mut job = mi::Job::boxed(Box::new(StoreSource { store: Rc::clone(&store), lib }));
        qrpgrt::install(&mut job);
        job.sql = Some(Box::new(Shared(Rc::clone(&session))));
        Self { store, session, job }
    }

    /// Run `text` with `params`; its outcome. Panics on an error.
    fn run(&mut self, text: &str, params: &[Value]) -> Option<Outcome> {
        let p = prepare_dynamic(text, Naming::Sql).unwrap_or_else(|e| panic!("{text}: {e}"));
        let mut txn = store::txn::Txn::lent(Box::new(|| self.store.borrow_mut())).unwrap();
        let (ca, o) = p.run(&mut txn, &mut self.session.borrow_mut().ctx, params);
        assert!(ca.sqlcode >= 0, "{text}: {ca:?}");
        txn.commit().unwrap();
        o
    }

    fn rows(&mut self, text: &str) -> Vec<Vec<Value>> {
        match self.run(text, &[]) {
            Some(Outcome::Rows(rs)) => rs.rows,
            o => panic!("{text}: {o:?}"),
        }
    }

    /// `CALL` through the job, as a program calls it.
    fn call(&mut self, text: &str) {
        let req = Request {
            scope: "1:BENCH".into(),
            desc: Desc { op: Op::Exec, sql_naming: true, text: text.into(), ..Desc::default() },
            inputs: Vec::new(),
        };
        let (reply, _) = qrpgrt::sqlcall::run_request(&mut self.job, req);
        assert!(reply.sqlca.sqlcode >= 0, "{text}: {:?}", reply.sqlca);
    }
}

fn timed<T>(name: &str, what: &str, f: impl FnOnce() -> T) -> (T, Duration) {
    let t = Instant::now();
    let r = f();
    let d = t.elapsed();
    println!("bench {name:<16} {:>10.1} ms  {what}", d.as_secs_f64() * 1000.0);
    (r, d)
}

fn rows_wanted() -> u64 {
    std::env::var("QS400_SQL_BENCH_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000)
}

fn load(b: &mut Bench, n: u64) {
    b.run("CREATE TABLE QSTEST.BIG (ID INTEGER NOT NULL PRIMARY KEY, GRP INTEGER NOT NULL, V INTEGER NOT NULL, NAME CHAR(12) NOT NULL)", &[]);
    let mut i = 1;
    while i <= n {
        let end = (i + 999).min(n);
        let vals: Vec<String> = (i..=end).map(|k| format!("({k}, {}, {}, 'N{}')", k % 1000, (k * 7919) % 1_000_003, k % 97)).collect();
        b.run(&format!("INSERT INTO QSTEST.BIG VALUES {}", vals.join(", ")), &[]);
        i = end + 1;
    }
    b.run("CREATE TABLE QSTEST.GRP (GRP INTEGER NOT NULL PRIMARY KEY, REGION INTEGER NOT NULL)", &[]);
    let vals: Vec<String> = (0..1000).map(|g| format!("({g}, {})", g % 10)).collect();
    b.run(&format!("INSERT INTO QSTEST.GRP VALUES {}", vals.join(", ")), &[]);
    b.run("CREATE TABLE QSTEST.REGION (REGION INTEGER NOT NULL PRIMARY KEY, LABEL CHAR(10) NOT NULL)", &[]);
    let vals: Vec<String> = (0..10).map(|r| format!("({r}, 'R{r}')")).collect();
    b.run(&format!("INSERT INTO QSTEST.REGION VALUES {}", vals.join(", ")), &[]);
}

#[test]
#[ignore = "a benchmark: cargo run -p build -- bench"]
#[allow(clippy::many_single_char_names)]
fn sql_benchmarks() {
    let n = rows_wanted();
    let mut b = Bench::new();
    timed("load", &format!("{n} rows inserted, 1,000 a statement"), || load(&mut b, n));

    let (r, _) = timed("order-by", &format!("SELECT * FROM BIG ORDER BY V ({n} rows sorted)"), || b.rows("SELECT * FROM QSTEST.BIG ORDER BY V"));
    assert_eq!(r.len() as u64, n);
    drop(r);

    let (r, _) = timed("join-3", "BIG x GRP x REGION, grouped by region", || {
        b.rows("SELECT R.LABEL, COUNT(*), SUM(B.V) FROM QSTEST.BIG B JOIN QSTEST.GRP G ON G.GRP = B.GRP JOIN QSTEST.REGION R ON R.REGION = G.REGION GROUP BY R.LABEL")
    });
    assert_eq!(r.len(), 10);

    let (r, _) = timed("recursive-cte", "99,999 levels of WITH RECURSIVE", || {
        b.rows("WITH T (K) AS (SELECT 1 FROM SYSIBM.SYSDUMMY1 UNION ALL SELECT K + 1 FROM T WHERE K < 99999) SELECT COUNT(*), MAX(K) FROM T")
    });
    assert_eq!(r[0][0].as_i64().unwrap(), 99_999);

    let w = n.min(100_000);
    let (r, _) = timed("window", &format!("SUM() OVER (PARTITION BY GRP ORDER BY ID) over {w} rows"), || {
        b.rows(&format!("SELECT ID, SUM(V) OVER (PARTITION BY GRP ORDER BY ID ROWS UNBOUNDED PRECEDING) FROM QSTEST.BIG WHERE ID <= {w}"))
    });
    assert_eq!(r.len() as u64, w);

    b.run("CREATE PROCEDURE QSTEST.BUMP (INOUT N INTEGER) LANGUAGE SQL BEGIN SET N = N + 1; END", &[]);
    b.run(
        "CREATE PROCEDURE QSTEST.LOOP10K () LANGUAGE SQL BEGIN DECLARE I INTEGER DEFAULT 0; DECLARE X INTEGER DEFAULT 0; \
         WHILE I < 10000 DO CALL QSTEST.BUMP(X); SET I = I + 1; END WHILE; END",
        &[],
    );
    timed("procedure-loop", "an SQL procedure calling another 10,000 times", || b.call("CALL QSTEST.LOOP10K()"));

    b.run("CREATE TABLE QSTEST.DOCS (ID INTEGER NOT NULL, D CLOB(200M))", &[]);
    let text: String = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-+"
        .chars()
        .cycle()
        .take(100 << 20)
        .collect();
    let v = Value::Str(text);
    timed("clob-write", "a 100 MB CLOB inserted", || b.run("INSERT INTO QSTEST.DOCS VALUES (1, ?)", std::slice::from_ref(&v)));
    let (r, _) = timed("clob-read", "and read back", || b.rows("SELECT D FROM QSTEST.DOCS WHERE ID = 1"));
    assert_eq!(r[0][0], v);
}

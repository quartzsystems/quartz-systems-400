// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database monitor: `STRDBMON` makes the `QAQQDBMN`-shaped file,
//! a watched job's statements and the planner's choices become `1000`,
//! `3000`, `3001`, `3003`, `3010` and `3014` records, `ENDDBMON`
//! writes them with the `3018` end record.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::dbmon::{Detail, JobIdent, Scope, StartSpec};
use sql::embed::{Desc, Embedded, Op, Request, Session};
use sql::{Naming, RunOpts, SqlContext, Value, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn job() -> JobIdent {
    JobIdent { name: "QPADEV0001".into(), user: "QSECOFR".into(), number: 4711 }
}

fn setup() -> (Rc<RefCell<Store>>, Session) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 7));
    let params = FormatParams { asp_number: 1, asp_uuid: [4; 16], unit_uuids: vec![[5; 16]], unit_serials: Vec::new(), wal_pages: 128, now: NOW };
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
    let names = NameContext { curlib: Some(lib), libl: vec![lib] };
    let mut ctx = SqlContext::new(qsys, JobHandle(1), names.clone());
    ctx.now = Some(NOW);
    let text = "CREATE TABLE QSTEST.T (ID INT NOT NULL PRIMARY KEY, NAME VARCHAR(20) NOT NULL, N DECIMAL(7, 2));
        INSERT INTO QSTEST.T VALUES (1, 'Ann', 1.5), (2, 'Bob', 2.5), (3, 'Cy', 3.5);
        CREATE TABLE QSTEST.NOTMON (A INT);";
    let r = runsqlstm(&mut store, &mut ctx, text, &RunOpts { naming: Naming::Sql, ..RunOpts::default() }).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    let shared = Rc::new(RefCell::new(store));
    let mut s = Session::new(Box::new(Rc::clone(&shared)), qsys, JobHandle(1), names);
    s.ctx.now = Some(NOW);
    s.ctx.dbmon.ident = Some(job());
    s.ctx.dbmon.system = "QS400".into();
    (shared, s)
}

fn spec(file: &str) -> StartSpec {
    StartSpec {
        lib: "QSTEST".into(),
        file: file.into(),
        member: "*FIRST".into(),
        add: false,
        scope: Scope::Job(job().number),
        job_text: "*".into(),
        detail: Detail::Basic,
        frcrcd: 0,
        incsyssql: false,
        comment: "probe".into(),
        command: format!("STRDBMON OUTFILE(QSTEST/{file})"),
        starter: job(),
    }
}

fn with_txn<T>(store: &Rc<RefCell<Store>>, s: &mut Session, f: impl FnOnce(&mut store::txn::Txn<'_>, &mut SqlContext) -> T) -> T {
    let mut st = store.borrow_mut();
    let mut t = st.begin().unwrap();
    let r = f(&mut t, &mut s.ctx);
    t.commit().unwrap();
    r
}

fn req(op: Op, name: &str, text: &str, inputs: Vec<Value>) -> Request {
    Request { scope: "1:M".into(), desc: Desc { op, name: name.into(), sql_naming: true, text: text.into(), ..Desc::default() }, inputs }
}

fn rows(s: &mut Session, text: &str) -> Vec<Vec<Value>> {
    let mut out = Vec::new();
    let r = s.run(&req(Op::Open, "MC", text, Vec::new()));
    assert!(!r.sqlca.is_error(), "{text}: {:?}", r.sqlca);
    loop {
        let r = s.run(&req(Op::Fetch, "MC", "", Vec::new()));
        if r.sqlca.sqlcode != 0 {
            break;
        }
        out.extend(r.rows);
    }
    s.run(&req(Op::Close, "MC", "", Vec::new()));
    out
}

fn s(v: &Value) -> String {
    match v {
        Value::Str(s) => s.trim_end().to_string(),
        Value::Null => "-".into(),
        v => v.to_string(),
    }
}

#[test]
fn a_job_monitor_writes_statements_and_access_methods() {
    let (store, mut s1) = setup();
    let id = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &spec("MON"))).unwrap();
    assert_eq!(id.len(), 10);
    // A second monitor over the same job is refused.
    let again = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &spec("MON2")));
    assert_eq!(again.unwrap_err().id, "CPF436C");

    let r = s1.run(&req(Op::Query, "", "SELECT NAME FROM QSTEST.T WHERE ID = ?", vec![Value::int(2)]));
    assert_eq!(r.rows, vec![vec![Value::Str("Bob".into())]]);
    let r = s1.run(&req(Op::Exec, "", "UPDATE QSTEST.T SET N = N + 1 WHERE N > 2", Vec::new()));
    assert_eq!(r.sqlca.sqlerrd[2], 2);
    assert_eq!(rows(&mut s1, "SELECT NAME FROM QSTEST.T ORDER BY N DESC").len(), 3);
    let r = s1.run(&req(Op::Exec, "", "DELETE FROM QSTEST.NOSUCH", Vec::new()));
    assert_eq!(r.sqlca.sqlcode, -204);

    let n = with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::Job(job().number), "*", None, "done")).unwrap();
    assert_eq!(n, 1);
    let again = with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::Job(job().number), "*", None, ""));
    assert_eq!(again.unwrap_err().id, "CPF436D");

    // The file has QAQQDBMN's columns and the records in order.
    let got = rows(&mut s1, "SELECT QQRID, QQC21, QQC11, QQC12, QQRCOD, QQTFN, QQIFNM, QQI8, QQI2, QQUDEF, QQJOB, QQJNUM FROM QSTEST.MON ORDER BY QQTIME, RRN(MON)");
    let lines: Vec<String> = got.iter().map(|r| r.iter().map(s).collect::<Vec<_>>().join(" ")).collect();
    let text = lines.join("\n");
    assert!(lines.first().is_some_and(|l| l.starts_with("3018 - C S - - - - - probe QPADEV0001 004711")), "{text}");
    assert!(lines.last().is_some_and(|l| l.starts_with("3018 - C E - - - - - done")), "{text}");
    for want in [
        "3001 - Y - I1 T T - - ",      // the primary key constraint, positioned to
        "1000 SI S S - - - 0 0 ",       // SELECT INTO
        "3000 - N - T2 T - - - ",       // UPDATE ... WHERE N > 2: a scan
        "1000 UP U S - - - 0 2 ",
        "3003 - - - F7 - - - - ",       // ORDER BY N: a sort
        "1000 OP S S - - - 0 0 ",
        "1000 FE S S - - - 0 0 ",
        "1000 CL O S - - - 0 0 ",
        "1000 DL D S - - - -204 0 ",
    ] {
        assert!(lines.iter().any(|l| l.starts_with(want)), "no `{want}` in\n{text}");
    }
    assert!(lines.iter().any(|l| l.starts_with("3010 ")), "{text}");
    assert!(lines.iter().any(|l| l.starts_with("3014 ")), "{text}");
    // Reading the monitor file is not monitored: the monitor has ended.
    let count = rows(&mut s1, "SELECT COUNT(*) FROM QSTEST.MON");
    assert_eq!(count[0][0].as_i64().unwrap(), i64::try_from(lines.len()).unwrap());
}

#[test]
fn the_output_file_must_have_the_monitor_format() {
    let (store, mut s1) = setup();
    let e = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &spec("NOTMON"))).unwrap_err();
    assert_eq!(e.id, "CPF3061");
    let mut bad = spec("X");
    bad.lib = "NOSUCH".into();
    let e = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &bad)).unwrap_err();
    assert_eq!(e.id, "CPF3064");
}

#[test]
fn every_job_monitors_and_add_keep_records() {
    let (store, mut s1) = setup();
    let mut all = spec("ALLMON");
    all.scope = Scope::All;
    all.job_text = "*ALL/*ALL/*ALL".into();
    all.frcrcd = 1;
    let id1 = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &all)).unwrap();
    // Two monitors over every job may run; ending one needs its ID.
    let id2 = with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &spec("ALLMON2").clone_with(Scope::All))).unwrap();
    s1.run(&req(Op::Exec, "", "INSERT INTO QSTEST.T VALUES (9, 'Zed', 9)", Vec::new()));
    let e = with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::All, "*ALL", None, "")).unwrap_err();
    assert_eq!(e.id, "CPF43A1");
    with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::All, "*ALL", Some(&id2), "")).unwrap();
    // FRCRCD(1): the first monitor's records are written as they come.
    let got = rows(&mut s1, "SELECT QQRID, QQC21 FROM QSTEST.ALLMON WHERE QQRID = 1000");
    assert_eq!(got.len(), 1);
    with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::All, "*ALL", Some(&id1), "")).unwrap();
    // *ADD keeps the records there.
    let mut add = spec("ALLMON");
    add.add = true;
    with_txn(&store, &mut s1, |t, c| sql::dbmon::start(t, c, &add)).unwrap();
    with_txn(&store, &mut s1, |t, c| sql::dbmon::end(t, c, &Scope::Job(job().number), "*", None, "")).unwrap();
    let got = rows(&mut s1, "SELECT COUNT(*) FROM QSTEST.ALLMON WHERE QQRID = 3018");
    assert_eq!(got[0][0].as_i64().unwrap(), 4);
}

trait CloneWith {
    fn clone_with(&self, scope: Scope) -> StartSpec;
}

impl CloneWith for StartSpec {
    fn clone_with(&self, scope: Scope) -> StartSpec {
        StartSpec { scope, ..self.clone() }
    }
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Embedded SQL sessions: `DESCRIBE` of prepared statements and the end
//! of a scope or an activation group closing cursors.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, RunOpts, SqlContext, Value, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn session() -> Session {
    let d = Arc::new(MemBlockDev::with_seed(16384, 21));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
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
    let mut ctx = SqlContext::new(qsys, JobHandle(1), names.clone());
    ctx.now = Some(NOW);
    let text = "CREATE TABLE QSTEST.T (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL, DEPT CHAR(3), SAL DECIMAL(9, 2));
        INSERT INTO QSTEST.T VALUES (1, 'Ann', 'ACC', 10.00), (2, 'Bob', 'DEV', NULL);";
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(&mut store, &mut ctx, text, &opts).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    Session::new(Box::new(Rc::new(RefCell::new(store))), qsys, JobHandle(1), names)
}

fn run(s: &mut Session, scope: &str, desc: Desc, inputs: Vec<Value>) -> Reply {
    s.run(&Request {
        scope: scope.into(),
        desc,
        inputs,
    })
}

fn op(op: Op, name: &str, text: &str) -> Desc {
    Desc {
        op,
        name: name.into(),
        text: text.into(),
        ..Desc::default()
    }
}

/// The entries of a reply: SQLTYPE, SQLLEN, SQLNAME.
fn entries(r: &Reply) -> Vec<(i64, i64, String)> {
    r.rows
        .iter()
        .map(|row| (row[0].as_i64().unwrap(), row[1].as_i64().unwrap(), row[2].as_text()))
        .collect()
}

#[test]
fn describe_gives_the_columns_and_the_markers() {
    let mut s = session();
    let r = run(&mut s, "1:P", op(Op::Prepare, "S1", "SELECT ID, NAME, DEPT, SAL, ID * 2 AS TWICE, COUNT(*) AS N FROM T GROUP BY ID, NAME, DEPT, SAL"), vec![]);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let r = run(&mut s, "1:P", op(Op::Describe, "S1", ""), vec![]);
    assert_eq!(
        entries(&r),
        [
            (496, 4, "ID".into()),
            (448, 20, "NAME".into()),
            (453, 3, "DEPT".into()),
            (485, 9 * 256 + 2, "SAL".into()),
            (496, 4, "TWICE".into()),
            (496, 4, "N".into()),
        ]
    );
    run(&mut s, "1:P", op(Op::Prepare, "S2", "SELECT NAME FROM T WHERE ID = ? AND DEPT = ?"), vec![]);
    let mut d = op(Op::Describe, "S2", "");
    d.orient = "INPUT".into();
    let r = run(&mut s, "1:P", d, vec![]);
    assert_eq!(entries(&r), [(497, 4, "00001".into()), (453, 3, "00002".into())]);
    // Not a query: no entries; never prepared: -516.
    run(&mut s, "1:P", op(Op::Prepare, "S3", "DELETE FROM T WHERE ID = 99"), vec![]);
    assert!(run(&mut s, "1:P", op(Op::Describe, "S3", ""), vec![]).rows.is_empty());
    assert_eq!(run(&mut s, "1:P", op(Op::Describe, "S9", ""), vec![]).sqlca.sqlcode, -516);
}

#[test]
fn scopes_end_with_the_module_or_the_group() {
    let mut s = session();
    let open = op(Op::Open, "C1", "SELECT ID FROM T ORDER BY ID");
    for scope in ["1:A", "1:B", "2:A"] {
        assert_eq!(run(&mut s, scope, open.clone(), vec![]).sqlca.sqlcode, 0);
    }
    // The module's end closes its cursor in that group only.
    run(&mut s, "1:A", op(Op::End, "", ""), vec![]);
    assert_eq!(run(&mut s, "1:A", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "2:A", open.clone(), vec![]).sqlca.sqlcode, -502);
    // The group's end closes all of its scopes.
    run(&mut s, "1:", op(Op::End, "", ""), vec![]);
    assert_eq!(run(&mut s, "1:A", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "1:B", open.clone(), vec![]).sqlca.sqlcode, 0);
    assert_eq!(run(&mut s, "2:A", open, vec![]).sqlca.sqlcode, -502);
}

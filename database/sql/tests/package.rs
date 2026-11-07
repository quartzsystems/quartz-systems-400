// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL packages through the embedded session (`Op::Package`): made,
//! statements added once by text, read back, cleared, bound as a DRDA
//! requester's, deleted.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::Value;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn session() -> (Session, Rc<RefCell<Store>>) {
    let d = Arc::new(MemBlockDev::with_seed(8192, 3));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
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
            text: String::new(),
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
    let store = Rc::new(RefCell::new(store));
    (
        Session::new(Box::new(store.clone()), qsys, JobHandle(1), names),
        store,
    )
}

fn pkg(s: &mut Session, action: &str, mut inputs: Vec<Value>) -> Reply {
    let mut all = vec![Value::Str("QSTEST".into()), Value::Str("QSPKG9899".into())];
    all.append(&mut inputs);
    s.run(&Request {
        scope: "QZDASOINIT:".into(),
        desc: Desc {
            op: Op::Package,
            name: action.into(),
            ..Desc::default()
        },
        inputs: all,
    })
}

fn s(t: &str) -> Value {
    Value::Str(t.into())
}

#[test]
fn a_package_keeps_its_statements() {
    let (mut sess, _) = session();
    let made = pkg(
        &mut sess,
        "CREATE",
        vec![
            Value::int(0),
            Value::int(1),
            Value::int(0),
            Value::int(1),
            s("/"),
            Value::int(0),
            s(":"),
            s("."),
            s(""),
            s(""),
        ],
    );
    assert_eq!(made.sqlca.sqlcode, 0, "{:?}", made.sqlca);
    // Made again: SQL0601, as the primary answers the second create.
    assert_eq!(pkg(&mut sess, "CREATE", Vec::new()).sqlca.sqlcode, -601);
    let text = "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1";
    let add = |sess: &mut Session, t: &str| {
        pkg(
            sess,
            "ADD",
            vec![
                Value::int(2),
                s(t),
                Value::Bin(vec![1, 2]),
                Value::Bin(vec![3]),
                Value::int(0),
            ],
        )
    };
    let a = add(&mut sess, text);
    let name = a.rows[0][0].as_text();
    assert!(
        name.starts_with("QZ") && name.ends_with("001") && name.len() == 18,
        "{name}"
    );
    // The same text keeps its name; another gets the next.
    assert_eq!(add(&mut sess, text).rows[0][0].as_text(), name);
    let b = add(&mut sess, "SELECT 1 FROM SYSIBM.SYSDUMMY1").rows[0][0].as_text();
    assert!(b.ends_with("002"), "{b}");
    let r = pkg(&mut sess, "RETURN", Vec::new());
    assert_eq!(r.rows.len(), 3);
    assert_eq!(r.rows[0][1], Value::int(1), "*SQL naming");
    assert_eq!(r.rows[1][0].as_text(), name);
    assert_eq!(r.rows[1][2].as_text(), text);
    assert_eq!(r.rows[1][3], Value::Bin(vec![1, 2]));
    assert_eq!(pkg(&mut sess, "CLEAR", Vec::new()).sqlca.sqlcode, 0);
    assert_eq!(pkg(&mut sess, "RETURN", Vec::new()).rows.len(), 1);
    assert_eq!(pkg(&mut sess, "DELETE", Vec::new()).sqlca.sqlcode, 0);
    assert_eq!(pkg(&mut sess, "RETURN", Vec::new()).sqlca.sqlcode, -204);
}

#[test]
fn a_bind_makes_the_package_again() {
    let (mut sess, _) = session();
    for _ in 0..2 {
        let r = pkg(
            &mut sess,
            "BIND",
            vec![
                s("12345"),
                Value::int(1),
                s("DECLARE C1 CURSOR FOR S1"),
                Value::int(2),
                s("DECLARE C2 CURSOR FOR S2"),
            ],
        );
        assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    }
    let r = pkg(&mut sess, "RETURN", Vec::new());
    assert_eq!(r.rows[0][0], Value::int(1), "bound");
    assert_eq!(r.rows[0][9].as_text(), "12345");
    let sections: Vec<(String, Value)> = r.rows[1..]
        .iter()
        .map(|x| (x[0].as_text(), x[6].clone()))
        .collect();
    assert_eq!(
        sections,
        [
            ("S1".to_string(), Value::int(1)),
            ("S2".to_string(), Value::int(2))
        ]
    );
}

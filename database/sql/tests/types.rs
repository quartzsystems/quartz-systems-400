// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data types end to end: floats, timestamps to the picosecond,
//! DECFLOAT, binary strings, BOOLEAN and LOBs, created, stored, read
//! back, compared, cast and described.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, Value, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(65536, 11));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 1024,
            now: 1_767_225_600,
        };
        let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
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
        let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
        ctx.now = Some(1_767_225_600);
        Self { store, ctx }
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
        self.run_with(text, &[]).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    fn values(&mut self, text: &str) -> Vec<Vec<Value>> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs.rows,
            o => panic!("{text}: {o:?}"),
        }
    }

    fn rows(&mut self, text: &str) -> Vec<Vec<String>> {
        self.values(text)
            .iter()
            .map(|r| r.iter().map(|v| if v.is_null() { "-".into() } else { v.to_string().trim_end().to_string() }).collect())
            .collect()
    }

    fn one(&mut self, text: &str) -> String {
        self.rows(text)[0][0].clone()
    }
}

#[test]
fn floats_round_trip() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/FL (R REAL, D DOUBLE)");
    db.ok("INSERT INTO QSTEST/FL VALUES(4.5, 4.5), (-0.1, 0.1), (1234.0625, 1.0E-5)");
    assert_eq!(
        db.rows("SELECT R, D FROM QSTEST/FL"),
        vec![vec!["4.5", "4.5"], vec!["-0.1", "0.1"], vec!["1234.0625", "0.00001"]]
    );
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/FL WHERE R = 4.5"), "1");
}

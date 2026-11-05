// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Global variables and distinct types as the primary has them
//! (`tools/conformance/sql/cases/objects2/`).

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
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

}

#[test]
fn global_variables() {
    let mut db = Db::new();
    db.ok("CREATE VARIABLE QSTEST.GVINT INTEGER DEFAULT 7");
    db.ok("CREATE VARIABLE QSTEST.GVCHR VARCHAR(20) DEFAULT 'start'");
    db.ok("CREATE VARIABLE QSTEST.GVDEC DECIMAL(9,2)");
    db.ok("CREATE VARIABLE QSTEST.A_LONG_VARIABLE_NAME DATE DEFAULT CURRENT DATE");
    assert_eq!(db.code("CREATE VARIABLE QSTEST.GVINT INTEGER"), (-601, "42710".to_string()));
    assert_eq!(db.rows("VALUES QSTEST.GVINT"), ["7"]);
    assert_eq!(db.rows("VALUES (QSTEST.GVCHR, QSTEST.GVDEC)"), ["start|NULL"]);
    db.ok("SET QSTEST.GVINT = 42");
    assert_eq!(db.rows("VALUES QSTEST.GVINT + 1"), ["43"]);
    db.ok("SET QSTEST.GVCHR = 'changed', QSTEST.GVDEC = 12.345");
    assert_eq!(db.rows("VALUES (QSTEST.GVCHR, QSTEST.GVDEC)"), ["changed|12.34"]);
    db.ok("SET (GVINT, GVDEC) = (5, DEFAULT)");
    assert_eq!(db.rows("VALUES (GVINT, GVDEC)"), ["5|NULL"]);
    db.ok("CREATE TABLE QSTEST.GVT (N INTEGER, C VARCHAR(20))");
    db.ok("INSERT INTO QSTEST.GVT VALUES (QSTEST.GVINT, QSTEST.GVCHR)");
    assert_eq!(db.rows("SELECT N, C FROM QSTEST.GVT WHERE N = GVINT"), ["5|changed"]);
    assert_eq!(db.code("VALUES QSTEST.GVNONE"), (-206, "42703".to_string()));
    assert_eq!(db.code("SET QSTEST.GVNONE = 1"), (-206, "42703".to_string()));
    db.ok("CREATE OR REPLACE VARIABLE QSTEST.GVDEC DECIMAL(5,1) DEFAULT 1.5");
    assert_eq!(db.rows("VALUES QSTEST.GVDEC"), ["1.5"]);
    db.ok("CREATE VARIABLE QSTEST.GVK INTEGER CONSTANT 3");
    assert_eq!(db.code("SET QSTEST.GVK = 4").0, -151);
    assert_eq!(
        db.rows("SELECT VARIABLE_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE, NUMERIC_PRECISION, CCSID, DEFAULT, SYSTEM_VAR_NAME FROM QSYS2.SYSVARIABLES ORDER BY VARIABLE_NAME"),
        [
            "A_LONG_VARIABLE_NAME|DATE|4|NULL|NULL|NULL|CURRENT DATE|A_LON00001",
            "GVCHR|VARCHAR|20|NULL|NULL|0|'start'|GVCHR",
            "GVDEC|DECIMAL|5|1|5|NULL|1.5|GVDEC",
            "GVINT|INTEGER|4|0|9|NULL|7|GVINT",
            "GVK|INTEGER|4|0|9|NULL|3|GVK",
        ]
    );
    db.ok("DROP VARIABLE QSTEST.GVINT");
    assert_eq!(db.code("DROP VARIABLE QSTEST.GVINT"), (-204, "42704".to_string()));
    assert_eq!(db.code("VALUES GVINT"), (-206, "42703".to_string()));
}

#[test]
fn distinct_types() {
    let mut db = Db::new();
    db.ok("CREATE TYPE QSTEST.MONEY AS DECIMAL(9,2)");
    db.ok("CREATE TYPE QSTEST.CODE AS CHAR(4) WITH COMPARISONS");
    assert_eq!(db.code("CREATE TYPE QSTEST.MONEY AS INTEGER"), (-601, "42710".to_string()));
    // BLOCKED(sql): no strong typing yet, so no use of the type.
    assert_eq!(db.code("CREATE TABLE QSTEST.UDTT (ID INTEGER, AMT QSTEST.MONEY)").0, -84);
    assert_eq!(
        db.rows("SELECT USER_DEFINED_TYPE_NAME, SOURCE_TYPE, LENGTH, NUMERIC_SCALE, METATYPE, SYSTEM_TYPE_NAME FROM QSYS2.SYSTYPES ORDER BY USER_DEFINED_TYPE_NAME"),
        ["CODE|CHARACTER|4|NULL|T|CODE", "MONEY|DECIMAL|9|2|T|MONEY"]
    );
    db.ok("DROP TYPE QSTEST.MONEY");
    db.ok("DROP TYPE QSTEST.CODE");
    assert_eq!(db.code("DROP TYPE QSTEST.NOTYPE"), (-204, "42704".to_string()));
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSYS2.SYSJARCONTENTS"), ["0"]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSYS2.SYSJAROBJECTS"), ["0"]);
}

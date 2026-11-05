// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! JSON end to end against pub400's outcomes
//! (`tools/conformance/sql/cases/types/JSON1.sql`).

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
        Self::sized(8192, 1024)
    }

    /// A store of `blocks` pages with a log of `wal` pages.
    fn sized(blocks: u64, wal: u64) -> Self {
        let d = Arc::new(MemBlockDev::with_seed(blocks, 11));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: wal,
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

impl Db {
    fn code(&mut self, text: &str) -> i32 {
        match self.run_with(text, &[]) {
            Ok(Outcome::Rows(_) | Outcome::Count(_) | Outcome::Done) => 0,
            Err(e) => e.sqlcode(),
        }
    }
}

const D: &str = " FROM QSTEST/T1";

fn one_row() -> Db {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/T1 (K INTEGER)");
    db.ok("INSERT INTO QSTEST/T1 VALUES(1)");
    db
}

fn one(db: &mut Db, e: &str) -> String {
    db.one(&format!("SELECT {e}{D}"))
}

fn code(db: &mut Db, e: &str) -> i32 {
    db.code(&format!("SELECT {e}{D}"))
}

#[test]
fn constructors() {
    let mut db = one_row();
    assert_eq!(one(&mut db, "JSON_OBJECT('a' VALUE 1, 'b' VALUE 'x')"), r#"{"a":1,"b":"x"}"#);
    assert_eq!(
        one(
            &mut db,
            "JSON_OBJECT(KEY 'a' VALUE 1.50, KEY 'b' VALUE -0.5, KEY 'c' VALUE 1.5E0, KEY 'd' VALUE CAST(1 AS SMALLINT), \
             KEY 'e' VALUE DECFLOAT('1.0E3'))"
        ),
        r#"{"a":1.50,"b":-0.5,"c":1.5E0,"d":1,"e":1.0E+3}"#
    );
    assert_eq!(
        one(&mut db, r#"JSON_OBJECT('a' VALUE '{"x":1}' FORMAT JSON, 'b' VALUE '{"x":1}')"#),
        r#"{"a":{"x":1},"b":"{\"x\":1}"}"#
    );
    assert_eq!(
        one(&mut db, "JSON_OBJECT('a' VALUE JSON_ARRAY(1, 2, 'x'), 'b' VALUE JSON_OBJECT('c' VALUE TRUE))"),
        r#"{"a":[1,2,"x"],"b":{"c":true}}"#
    );
    assert_eq!(one(&mut db, "JSON_OBJECT('a' VALUE 1, 'a' VALUE 2)"), r#"{"a":1,"a":2}"#);
    assert_eq!(code(&mut db, "JSON_OBJECT('a' VALUE 1, 'a' VALUE 2 WITH UNIQUE KEYS)"), -16400);
    assert_eq!(one(&mut db, "JSON_OBJECT()"), "{}");
    assert_eq!(one(&mut db, "JSON_OBJECT('a' VALUE '[1,2' FORMAT JSON)"), r#"{"a":[1,2}"#);
    assert_eq!(one(&mut db, "JSON_ARRAY(VALUES 1 UNION ALL VALUES 2)"), "[1,2]");
    assert_eq!(
        one(&mut db, "JSON_ARRAY(1E300, -0E0, 0.1E0, CAST(0.1 AS REAL), 12345678901234567890)"),
        "[1.0000000000000001E300,-0E0,1.0000000000000001E-1,1.0000000149011612E-1,12345678901234567890]"
    );
    assert_eq!(one(&mut db, "JSON_ARRAY(1, CAST(NULL AS INTEGER), 'a')"), r#"[1,"a"]"#);
    assert_eq!(one(&mut db, "JSON_ARRAY(1, CAST(NULL AS INTEGER), 'a' NULL ON NULL)"), r#"[1,null,"a"]"#);
    assert_eq!(code(&mut db, "JSON_OBJECT('a' VALUE NULL)"), -206);
    assert_eq!(code(&mut db, "JSON_OBJECT('a' VALUE 'abcdef' RETURNING VARCHAR(5))"), -802);
    assert_eq!(code(&mut db, "JSON_UPDATE('{}', '$.a', 1)"), -204);
}

#[test]
fn queries() {
    let mut db = one_row();
    let doc = r#"'{"a":{"b":[10,20,30]},"s":"xéy","n":null,"t":true,"f":1.5e2}'"#;
    assert_eq!(one(&mut db, &format!("JSON_VALUE({doc}, '$.a.b[1]')")), "20");
    assert_eq!(one(&mut db, &format!("JSON_VALUE({doc}, '$.s')")), "xéy");
    assert_eq!(one(&mut db, &format!("JSON_VALUE({doc}, '$.n')")), "-");
    assert_eq!(one(&mut db, &format!("JSON_VALUE({doc}, '$.t')")), "true");
    assert_eq!(one(&mut db, &format!("JSON_VALUE({doc}, '$.f')")), "150");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":1.5}', '$.a' RETURNING DECIMAL(5, 2))"#), "1.50");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":1.5}', '$.a' RETURNING INTEGER)"#), "1");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":1}', '$.b' DEFAULT 'none' ON EMPTY)"#), "none");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":[1]}', '$.a')"#), "1");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":1}', 'strict $.b')"#), "-");
    assert_eq!(code(&mut db, r#"JSON_VALUE('{"a":1}', 'strict $.b' ERROR ON ERROR)"#), -16410);
    assert_eq!(code(&mut db, r#"JSON_VALUE('{"a":"x"}', '$.a' RETURNING INTEGER ERROR ON ERROR)"#), -406);
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":"x"}', '$.a' RETURNING INTEGER)"#), "-");
    assert_eq!(one(&mut db, "JSON_VALUE('not json', '$.a')"), "-");
    assert_eq!(code(&mut db, "JSON_VALUE('not json', '$.a' ERROR ON ERROR)"), -16402);
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":1}', 'bad path')"#), "-");
    assert_eq!(code(&mut db, r#"JSON_VALUE('{"a":1}', '$.a ? (@ > 0)')"#), -84);
    assert_eq!(one(&mut db, "JSON_VALUE('[1,2,3]', '$[last]')"), "3");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a b":3}', '$."a b"')"#), "3");
    assert_eq!(one(&mut db, r#"JSON_VALUE('{"a":5}', 'lax $.a[0]')"#), "5");
    let q = r#"'{"a":{"b":[10,20]}}'"#;
    assert_eq!(one(&mut db, &format!("JSON_QUERY({q}, '$.a')")), r#"{"b":[10,20]}"#);
    assert_eq!(one(&mut db, &format!("JSON_QUERY({q}, '$.a.b[0]')")), "10");
    assert_eq!(one(&mut db, &format!("JSON_QUERY({q}, '$.a.b[0]' WITH WRAPPER)")), "[10]");
    assert_eq!(one(&mut db, &format!("JSON_QUERY({q}, '$.a.b[*]' WITH CONDITIONAL WRAPPER)")), "[10,20]");
    assert_eq!(one(&mut db, r#"JSON_QUERY('{"a":"x"}', '$.a')"#), r#""x""#);
    assert_eq!(one(&mut db, r#"JSON_QUERY('{"a":"x"}', '$.a' OMIT QUOTES)"#), "x");
    assert_eq!(one(&mut db, r#"JSON_QUERY('{"a":1}', '$.b' EMPTY ARRAY ON EMPTY)"#), "[]");
    assert_eq!(one(&mut db, r#"JSON_QUERY('{"a" : [ 1 , {"b" :  2} ] }', '$')"#), r#"{"a":[1,{"b":2}]}"#);
    assert_eq!(one(&mut db, r#"JSON_QUERY('{"a":{"x":1,"y":2}}', '$.a.*' WITH WRAPPER)"#), "[1,2]");
    assert_eq!(one(&mut db, r#"JSON_EXISTS('{"a":1}', '$.a')"#), "TRUE");
    assert_eq!(one(&mut db, r#"JSON_EXISTS('{"a":1}', 'strict $.b' FALSE ON ERROR)"#), "FALSE");
    assert_eq!(code(&mut db, "JSON_EXISTS('x', '$.a' ERROR ON ERROR)"), -16402);
    let is = [
        r#"'{"a":1}' IS JSON"#,
        "'x' IS JSON",
        "'[1]' IS JSON OBJECT",
        "'[1]' IS JSON ARRAY",
        "'1' IS JSON SCALAR",
        r#"'{"a":1,"a":2}' IS JSON WITH UNIQUE KEYS"#,
        "'x' IS NOT JSON",
    ];
    let got: Vec<String> = is.iter().map(|p| one(&mut db, &format!("CASE WHEN {p} THEN 1 ELSE 0 END"))).collect();
    assert_eq!(got, ["1", "0", "0", "1", "1", "0", "1"]);
    let b = r#"'{"a":1,"b":"x","c":[true,null],"d":1.5,"e":12345678901}'"#;
    assert_eq!(
        one(&mut db, &format!("HEX(JSON_TO_BSON({b}))")),
        "3A000000106100010000000262000200000078000463000C000000083000010A310000016400000000000000F83F126500351CDCDF0200000000"
    );
    assert_eq!(
        one(&mut db, &format!("BSON_TO_JSON(JSON_TO_BSON({b}))")),
        r#"{"a":1,"b":"x","c":[true, null ],"d":1.5,"e":12345678901}"#
    );
    assert_eq!(one(&mut db, r#"JSON_VALUE(JSON_TO_BSON('{"a":7}') FORMAT BSON, '$.a')"#), "7");
}

#[test]
fn aggregates() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/JS1 (K INTEGER, NAME VARCHAR(10), AMT DECIMAL(5, 2))");
    db.ok("INSERT INTO QSTEST/JS1 VALUES (1, 'a', 1.50), (2, 'b', NULL), (3, 'c', -2.00)");
    assert_eq!(db.one("SELECT JSON_ARRAYAGG(NAME ORDER BY K DESC) FROM QSTEST/JS1"), r#"["c","b","a"]"#);
    assert_eq!(
        db.rows("SELECT JSON_ARRAYAGG(AMT), JSON_ARRAYAGG(AMT NULL ON NULL) FROM QSTEST/JS1")[0],
        ["[1.50,-2.00]", "[1.50,null,-2.00]"]
    );
    assert_eq!(
        db.rows("SELECT JSON_OBJECTAGG(NAME VALUE AMT), JSON_OBJECTAGG(KEY NAME VALUE AMT ABSENT ON NULL) FROM QSTEST/JS1")[0],
        [r#"{"a":1.50,"b":null,"c":-2.00}"#, r#"{"a":1.50,"c":-2.00}"#]
    );
    assert_eq!(
        db.rows("SELECT K, JSON_ARRAYAGG(NAME) FROM QSTEST/JS1 GROUP BY K ORDER BY K"),
        [["1", r#"["a"]"#], ["2", r#"["b"]"#], ["3", r#"["c"]"#]]
    );
    assert_eq!(
        db.rows("SELECT JSON_OBJECT('k' VALUE K, 'n' VALUE NAME, 'a' VALUE AMT) FROM QSTEST/JS1 ORDER BY K"),
        [[r#"{"k":1,"n":"a","a":1.50}"#], [r#"{"k":2,"n":"b","a":null}"#], [r#"{"k":3,"n":"c","a":-2.00}"#]]
    );
}

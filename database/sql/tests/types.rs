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
    // IEEE values past a decimal's 63 digits.
    db.ok("INSERT INTO QSTEST/FL VALUES(3.4028235E38, 1E300), (1E-45, -2.5E-308), (-0E0, -0E0)");
    assert_eq!(
        db.rows("SELECT R, D FROM QSTEST/FL WHERE D > 1E200 OR D < 0 ORDER BY D"),
        vec![vec!["1E-45", "-2.5E-308"], vec!["3.4028235E38", "1E300"]]
    );
    assert_eq!(db.one("SELECT D * 10 FROM QSTEST/FL WHERE D > 1E200"), "1E301");
    assert_eq!(db.one("SELECT D / 2 FROM QSTEST/FL WHERE D > 1E200"), "5E299");
    // Mixed with a decimal: a double.
    assert_eq!(db.one("SELECT D + 1 FROM QSTEST/FL WHERE D = 4.5"), "5.5");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/FL WHERE D = 0"), "1");
    assert!(db.run_with("SELECT D * D FROM QSTEST/FL WHERE D > 1E200", &[]).is_err());
    assert!(db.run_with("INSERT INTO QSTEST/FL (R) VALUES(1E39)", &[]).is_err());
    assert!(db.run_with("SELECT CAST(D AS DECIMAL(31, 0)) FROM QSTEST/FL WHERE D > 1E200", &[]).is_err());
    assert_eq!(db.one("SELECT CAST(D AS INTEGER) FROM QSTEST/FL WHERE D = 4.5"), "4");
    assert_eq!(db.one("SELECT MAX(D) FROM QSTEST/FL"), "1E300");
}

#[test]
fn timestamps_to_the_picosecond() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/TS (A TIMESTAMP(12), B TIMESTAMP(0), C TIMESTAMP, D TIMESTAMP(3) NOT NULL WITH DEFAULT)");
    db.ok(
        "INSERT INTO QSTEST/TS (A, B, C) VALUES('2026-11-02-10.20.30.123456789012', '2026-11-02 10:20:30.9', \
         '2026-11-02 10:20:30.123456789')",
    );
    db.ok("INSERT INTO QSTEST/TS (A, B, C) VALUES('2026-11-02-10.20.30.123456789013', CURRENT TIMESTAMP, CURRENT TIMESTAMP)");
    assert_eq!(
        db.rows("SELECT A, B, C, D FROM QSTEST/TS WHERE A < TIMESTAMP '2026-11-02 10:20:30.123456789013'"),
        vec![vec!["2026-11-02 10:20:30.123456789012", "2026-11-02 10:20:30", "2026-11-02 10:20:30.123456", "0001-01-01 00:00:00.000"]]
    );
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/TS WHERE A > '2026-11-02-10.20.30.123456789012'"), "1");
    assert_eq!(db.one("SELECT MAX(A) FROM QSTEST/TS"), "2026-11-02 10:20:30.123456789013");
    assert_eq!(
        db.one("SELECT CAST(A AS TIMESTAMP(9)) FROM QSTEST/TS ORDER BY A FETCH FIRST 1 ROW ONLY"),
        "2026-11-02 10:20:30.123456789"
    );
    assert_eq!(
        db.one("SELECT A + 1 MICROSECOND FROM QSTEST/TS ORDER BY A FETCH FIRST 1 ROW ONLY"),
        "2026-11-02 10:20:30.123457789012"
    );
    assert_eq!(
        db.rows("SELECT NUMERIC_SCALE, LENGTH FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'TS' ORDER BY ORDINAL_POSITION"),
        vec![vec!["12", "32"], vec!["0", "19"], vec!["6", "26"], vec!["3", "23"]]
    );
    assert!(db.run_with("CREATE TABLE QSTEST/TS2 (A TIMESTAMP(13))", &[]).is_err());
}

#[test]
fn decfloat_values() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/DF (K INTEGER NOT NULL, A DECFLOAT(16), B DECFLOAT, PRIMARY KEY(K))");
    db.ok("INSERT INTO QSTEST/DF VALUES(1, 1.5, 1.5E-30), (2, -2, 'Infinity'), (3, 12345678901234567, 'NaN'), (4, NULL, '-sNaN')");
    db.ok("INSERT INTO QSTEST/DF VALUES(5, 'Infinity', 1.00)");
    assert_eq!(
        db.rows("SELECT A, B FROM QSTEST/DF ORDER BY K"),
        vec![
            vec!["1.5", "1.5E-30"],
            vec!["-2", "Infinity"],
            vec!["1.234567890123457E+16", "NaN"],
            vec!["-", "-SNaN"],
            vec!["Infinity", "1.00"],
        ]
    );
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DF WHERE B = 1"), "1");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DF WHERE B > 1000"), "2");
    assert_eq!(db.one("SELECT A * 2 + 1 FROM QSTEST/DF WHERE K = 1"), "4.0");
    assert_eq!(db.one("SELECT B / 3 FROM QSTEST/DF WHERE K = 5"), "0.3333333333333333333333333333333333");
    assert_eq!(db.one("SELECT SUM(A) FROM QSTEST/DF WHERE K < 3"), "-0.5");
    assert_eq!(db.rows("SELECT K FROM QSTEST/DF ORDER BY B, K"), vec![vec!["4"], vec!["1"], vec!["5"], vec!["2"], vec!["3"]]);
    assert_eq!(db.one("SELECT CAST(B AS DECIMAL(5, 2)) FROM QSTEST/DF WHERE K = 5"), "1.00");
    assert_eq!(db.one("SELECT NORMALIZE_DECFLOAT(B) FROM QSTEST/DF WHERE K = 5"), "1");
    assert!(db.run_with("SELECT B - B FROM QSTEST/DF WHERE K = 2", &[]).is_err());
    let p = Value::DecFloat(qdecimal::DecFloat::parse("-1E+5", qdecimal::DfFormat::D34).unwrap());
    db.run_with("INSERT INTO QSTEST/DF VALUES(6, ?, ?)", &[Value::int(7), p]).unwrap();
    assert_eq!(db.one("SELECT A || ' ' || B FROM QSTEST/DF WHERE K = 6"), "7 -1E+5");
    assert_eq!(
        db.rows(
            "SELECT DATA_TYPE, LENGTH, NUMERIC_PRECISION FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'DF' AND \
             COLUMN_NAME <> 'K' ORDER BY ORDINAL_POSITION"
        ),
        vec![vec!["DECFLOAT", "8", "16"], vec!["DECFLOAT", "16", "34"]]
    );
}

#[test]
fn binary_strings() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/BI (A BINARY(4), B VARBINARY(10), C CHAR(4) FOR BIT DATA)");
    db.ok("INSERT INTO QSTEST/BI VALUES(BX'01FF', BX'00C1', X'C1C2C3C4')");
    db.ok("INSERT INTO QSTEST/BI VALUES(X'02', BX'', X'00')");
    assert_eq!(
        db.rows("SELECT A, B, HEX(B), LENGTH(A), LENGTH(B) FROM QSTEST/BI WHERE A = BX'01FF0000'"),
        vec![vec!["01FF0000", "00C1", "00C1", "4", "2"]]
    );
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BI WHERE B > BX'00'"), "1");
    assert_eq!(db.one("SELECT HEX(SUBSTR(B, 2, 1)) FROM QSTEST/BI WHERE LENGTH(B) = 2"), "C1");
    assert_eq!(db.one("SELECT CAST(B AS BINARY(3)) FROM QSTEST/BI WHERE LENGTH(B) = 2"), "00C100");
    assert_eq!(db.one("SELECT B || BX'FF' FROM QSTEST/BI WHERE LENGTH(B) = 2"), "00C1FF");
    assert!(db.run_with("INSERT INTO QSTEST/BI VALUES(BX'0102030405', NULL, NULL)", &[]).is_err());
    db.run_with("INSERT INTO QSTEST/BI (B) VALUES(?)", &[Value::Bin(vec![9, 8, 7])]).unwrap();
    match db.run_with("SELECT HEX(B) FROM QSTEST/BI WHERE B = ?", &[Value::Bin(vec![9, 8, 7])]).unwrap() {
        Outcome::Rows(rs) => assert_eq!(rs.rows[0][0].to_string(), "090807"),
        o => panic!("{o:?}"),
    }
    assert_eq!(
        db.rows("SELECT DATA_TYPE, LENGTH, CCSID FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'BI' ORDER BY ORDINAL_POSITION"),
        vec![vec!["BINARY", "4", "65535"], vec!["VARBIN", "10", "65535"], vec!["CHAR", "4", "65535"]]
    );
}

#[test]
fn booleans() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/BO (K INTEGER, F BOOLEAN, G BOOLEAN NOT NULL DEFAULT TRUE)");
    db.ok("INSERT INTO QSTEST/BO (K, F) VALUES(1, TRUE), (2, FALSE), (3, UNKNOWN), (4, 'true')");
    assert_eq!(
        db.rows("SELECT K, F, G FROM QSTEST/BO ORDER BY K"),
        vec![vec!["1", "TRUE", "TRUE"], vec!["2", "FALSE", "TRUE"], vec!["3", "-", "TRUE"], vec!["4", "TRUE", "TRUE"]]
    );
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE F"), "2");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE NOT F"), "1");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE F IS UNKNOWN"), "1");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE F IS NOT TRUE"), "2");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE F = FALSE"), "1");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE (K > 2) = F"), "2");
    assert_eq!(db.one("SELECT CAST(F AS CHAR(5)) FROM QSTEST/BO WHERE K = 2"), "FALSE");
    db.run_with("INSERT INTO QSTEST/BO (K, F) VALUES(5, ?)", &[Value::Bool(false)]).unwrap();
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BO WHERE F IS FALSE"), "2");
    assert_eq!(db.one("SELECT DATA_TYPE FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'BO' AND COLUMN_NAME = 'F'"), "BOOLEAN");
}

#[test]
fn lobs() {
    let mut db = Db::sized(40000, 12000);
    db.ok("CREATE TABLE QSTEST/LB (K INTEGER NOT NULL PRIMARY KEY, B BLOB(20M), C CLOB(1K), D DBCLOB(100))");
    db.ok("INSERT INTO QSTEST/LB VALUES(1, BX'0102', 'short', 'ünïcode €')");
    let big: Vec<u8> = (0..16 * 1024 * 1024 + 77u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let text = "abc".repeat(300);
    db.run_with("INSERT INTO QSTEST/LB VALUES(2, ?, ?, NULL)", &[Value::Bin(big.clone()), Value::Str(text.clone())]).unwrap();
    db.ok("INSERT INTO QSTEST/LB (K) VALUES(3)");
    let rows = db.values("SELECT B, C, D, LENGTH(B), LENGTH(C) FROM QSTEST/LB ORDER BY K");
    assert_eq!(
        rows[0],
        vec![Value::Bin(vec![1, 2]), Value::Str("short".into()), Value::Str("ünïcode €".into()), Value::int(2), Value::int(5)]
    );
    assert_eq!(rows[1][0], Value::Bin(big.clone()));
    assert_eq!(rows[1][1], Value::Str(text));
    assert_eq!(rows[1][3], Value::int(i64::try_from(big.len()).unwrap()));
    assert!(rows[2][0].is_null());
    assert_eq!(db.one("SELECT HEX(SUBSTR(B, 1000001, 3)) FROM QSTEST/LB WHERE K = 2"), "101112");
    assert_eq!(db.one("SELECT SUBSTR(C, 4, 5) FROM QSTEST/LB WHERE K = 2"), "abcab");
    db.ok("UPDATE QSTEST/LB SET C = 'changed and longer than twenty-four bytes' WHERE K = 1");
    assert_eq!(db.one("SELECT C FROM QSTEST/LB WHERE K = 1"), "changed and longer than twenty-four bytes");
    assert!(db.run_with("INSERT INTO QSTEST/LB (K, C) VALUES(4, ?)", &[Value::Str("x".repeat(1025))]).is_err());
    assert!(db.run_with("CREATE INDEX QSTEST/LBX ON QSTEST/LB (C)", &[]).is_err());
    assert_eq!(
        db.rows(
            "SELECT DATA_TYPE, LENGTH, CCSID FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'LB' AND COLUMN_NAME <> 'K' \
             ORDER BY ORDINAL_POSITION"
        ),
        vec![vec!["BLOB", "20971520", "65535"], vec!["CLOB", "1024", "-"], vec!["DBCLOB", "100", "1200"]]
    );
}

#[test]
fn column_names_up_to_128() {
    let mut db = Db::new();
    let long = format!("A_COLUMN_NAME_{}", "X".repeat(100));
    assert_eq!(long.len(), 114);
    db.ok(&format!("CREATE TABLE QSTEST/LN ({long} FOR COLUMN SHORT INT, B INT)"));
    db.ok(&format!("INSERT INTO QSTEST/LN ({long}, B) VALUES(7, 8)"));
    assert_eq!(db.one(&format!("SELECT {long} FROM QSTEST/LN")), "7");
    assert_eq!(db.one("SELECT COLUMN_NAME FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'LN' AND SYSTEM_COLUMN_NAME = 'SHORT'"), long);
    let p = prepare("SELECT * FROM QSTEST/LN", Naming::Sys).unwrap();
    let mut t = db.store.begin().unwrap();
    let names: Vec<String> =
        sql::embed::describe::entries(&mut t, &db.ctx, &p.statement, false).into_iter().map(|(_, n, _)| n).collect();
    t.abort();
    assert_eq!(names, [long.clone(), "B".into()]);
    let too_long = "Y".repeat(129);
    let e = db.run_with(&format!("CREATE TABLE QSTEST/LN2 ({too_long} FOR COLUMN Y INT)"), &[]).unwrap_err();
    assert_eq!((e.sqlcode(), e.sqlstate()), (-107, "42622"));
}

#[test]
fn describe_types() {
    let mut db = Db::new();
    db.ok(
        "CREATE TABLE QSTEST/DS (A DECFLOAT(16), B DECFLOAT, C BINARY(4), D VARBINARY(9), E BOOLEAN NOT NULL, \
         F BLOB(1K), G CLOB(2K), H DBCLOB(3), I TIMESTAMP(12), J CHAR(3) FOR BIT DATA)",
    );
    let p = prepare("SELECT * FROM QSTEST/DS", Naming::Sys).unwrap();
    let mut t = db.store.begin().unwrap();
    let got: Vec<(i16, i32, u16)> = sql::embed::describe::entries(&mut t, &db.ctx, &p.statement, false)
        .into_iter()
        .map(|(c, _, _)| (c.base, c.len, c.ccsid))
        .collect();
    t.abort();
    assert_eq!(
        got,
        [
            (996, 8, 0),
            (996, 16, 0),
            (912, 4, 65535),
            (908, 9, 65535),
            (2436, 1, 0),
            (404, 1024, 65535),
            (408, 2048, 0),
            (412, 3, 1200),
            (392, 32, 0),
            (452, 3, 65535)
        ]
    );
}

#[test]
fn integers_take_their_whole_range() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/IN (S SMALLINT NOT NULL PRIMARY KEY, I INTEGER, B BIGINT)");
    db.ok("INSERT INTO QSTEST/IN VALUES(-32768, -2147483648, -9223372036854775808)");
    db.ok("INSERT INTO QSTEST/IN VALUES(32767, 2147483647, 9223372036854775807)");
    db.ok("INSERT INTO QSTEST/IN VALUES(10000, 1.9, -1.9)");
    db.ok("INSERT INTO QSTEST/IN VALUES(9999, 0, 0)");
    assert_eq!(
        db.rows("SELECT S, I, B FROM QSTEST/IN ORDER BY S"),
        vec![
            vec!["-32768", "-2147483648", "-9223372036854775808"],
            vec!["9999", "0", "0"],
            vec!["10000", "1", "-1"],
            vec!["32767", "2147483647", "9223372036854775807"]
        ]
    );
    // The key orders the five-digit values past the four-digit ones.
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/IN WHERE S > 9999"), "2");
    assert!(db.run_with("INSERT INTO QSTEST/IN VALUES(32768, 0, 0)", &[]).is_err());
    assert!(db.run_with("INSERT INTO QSTEST/IN VALUES(1, 2147483648, 0)", &[]).is_err());
    assert!(db.run_with("INSERT INTO QSTEST/IN VALUES(1, 0, 9223372036854775808)", &[]).is_err());
    assert!(db.run_with("INSERT INTO QSTEST/IN VALUES(32767, 0, 0)", &[]).is_err());
    assert_eq!(
        db.rows(
            "SELECT DATA_TYPE, LENGTH, NUMERIC_PRECISION FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'IN' \
             ORDER BY ORDINAL_POSITION"
        ),
        vec![vec!["SMALLINT", "2", "5"], vec!["INTEGER", "4", "10"], vec!["BIGINT", "8", "19"]]
    );
    let p = prepare("SELECT * FROM QSTEST/IN", Naming::Sys).unwrap();
    let mut t = db.store.begin().unwrap();
    let got: Vec<(i16, i32)> = sql::embed::describe::entries(&mut t, &db.ctx, &p.statement, false)
        .into_iter()
        .map(|(c, _, _)| (c.base, c.len))
        .collect();
    t.abort();
    assert_eq!(got, [(500, 2), (496, 4), (492, 8)]);
}

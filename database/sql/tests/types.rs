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
    db.ok("INSERT INTO QSTEST/DF VALUES(1, 1.5, '1.5E-30'), (2, -2, 'Infinity'), (3, 12345678901234567, 'NaN'), (4, NULL, '-sNaN')");
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
    // Infinity - Infinity is NaN on the primary.
    assert_eq!(db.one("SELECT B - B FROM QSTEST/DF WHERE K = 2"), "NaN");
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

#[test]
fn aliases() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/BASE (K INTEGER NOT NULL PRIMARY KEY, V VARCHAR(10))");
    db.ok("INSERT INTO QSTEST/BASE VALUES(1, 'one'), (2, 'two')");
    db.ok("CREATE ALIAS QSTEST/AB FOR QSTEST/BASE");
    assert_eq!(db.one("SELECT V FROM QSTEST/AB WHERE K = 2"), "two");
    db.ok("INSERT INTO QSTEST/AB VALUES(3, 'three')");
    db.ok("UPDATE QSTEST/AB SET V = 'uno' WHERE K = 1");
    db.ok("DELETE FROM QSTEST/AB WHERE K = 2");
    assert_eq!(db.rows("SELECT K, V FROM QSTEST/BASE ORDER BY K"), vec![vec!["1", "uno"], vec!["3", "three"]]);
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/AB A JOIN QSTEST/BASE B ON A.K = B.K"), "2");
    db.ok("LABEL ON ALIAS QSTEST/AB IS 'Base by another name'");
    assert_eq!(
        db.rows(
            "SELECT TABLE_TYPE, COLUMN_COUNT, TABLE_TEXT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST' \
             AND TABLE_NAME = 'AB'"
        ),
        vec![vec!["A", "2", "Base by another name"]]
    );
    // An alias over a member: the second member's rows.
    {
        let mut t = db.store.begin().unwrap();
        let id = db.ctx.resolve_file(&mut t, &sql::ast::ObjectName::qualified("QSTEST", "BASE")).unwrap();
        db::file::change_attr(&mut t, id, &mut |a| a.max_members = 0).unwrap();
        db::file::add_member(&mut t, id, Name::parse("M2").unwrap(), "", None, 1_767_225_600).unwrap();
        t.commit().unwrap();
    }
    db.ok("CREATE ALIAS QSTEST/AM FOR QSTEST/BASE (M2)");
    db.ok("INSERT INTO QSTEST/AM VALUES(9, 'nine')");
    assert_eq!(db.rows("SELECT K FROM QSTEST/AM"), vec![vec!["9"]]);
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BASE"), "2");
    // The alias can name a table made later; OR REPLACE points it elsewhere.
    db.ok("CREATE ALIAS QSTEST/LATER FOR QSTEST/NOTYET");
    assert!(db.run_with("SELECT * FROM QSTEST/LATER", &[]).is_err());
    db.ok("CREATE TABLE QSTEST/NOTYET (X INTEGER)");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/LATER"), "0");
    assert!(db.run_with("CREATE ALIAS QSTEST/LATER FOR QSTEST/BASE", &[]).is_err());
    db.ok("CREATE OR REPLACE ALIAS QSTEST/LATER FOR QSTEST/BASE");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/LATER"), "2");
    assert!(db.run_with("CREATE OR REPLACE ALIAS QSTEST/BASE FOR QSTEST/NOTYET", &[]).is_err());
    assert!(db.run_with("DROP ALIAS QSTEST/BASE", &[]).is_err());
    db.ok("DROP ALIAS QSTEST/AB");
    assert!(db.run_with("SELECT * FROM QSTEST/AB", &[]).is_err());
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/BASE"), "2");
}

#[test]
fn graphic_takes_unicode() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/GR (K INTEGER NOT NULL PRIMARY KEY, G GRAPHIC(5) CCSID 1200, V VARGRAPHIC(20) CCSID 13488)");
    db.ok("INSERT INTO QSTEST/GR VALUES(1, G'ＡＢ', N'全角テキスト'), (2, '漢字', 'ω'), (3, 'Ω', ''), (4, NULL, 'Ωx')");
    assert_eq!(
        db.rows("SELECT K, G, V, LENGTH(V) FROM QSTEST/GR ORDER BY K"),
        vec![
            vec!["1", "ＡＢ", "全角テキスト", "6"],
            vec!["2", "漢字", "ω", "1"],
            vec!["3", "Ω", "", "0"],
            vec!["4", "-", "Ωx", "2"]
        ]
    );
    assert_eq!(db.one("SELECT K FROM QSTEST/GR WHERE G = '漢字'"), "2");
    assert_eq!(db.one("SELECT K FROM QSTEST/GR WHERE V LIKE '全角%'"), "1");
    db.ok("UPDATE QSTEST/GR SET V = V || '😀' WHERE K = 2");
    assert_eq!(db.one("SELECT V FROM QSTEST/GR WHERE K = 2"), "ω😀");
    // Five characters; past them only blanks.
    db.ok("INSERT INTO QSTEST/GR (K, G) VALUES(5, 'ＡＢＣＤＥ  ')");
    assert!(db.run_with("INSERT INTO QSTEST/GR (K, G) VALUES(6, 'ＡＢＣＤＥＦ')", &[]).is_err());
    // An index over the graphic column orders by UTF-16.
    db.ok("CREATE INDEX QSTEST/GRV ON QSTEST/GR (V)");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/GR WHERE V = 'ω😀'"), "1");
}

#[test]
fn varbinary_keys_do_not_pad() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/VB (B VARBINARY(4) NOT NULL PRIMARY KEY, N INTEGER)");
    db.ok("INSERT INTO QSTEST/VB VALUES(BX'0100', 3), (BX'01', 2), (BX'', 1), (BX'02', 5), (BX'010000', 4)");
    // X'01' and X'0100' are different keys, the shorter first.
    assert_eq!(db.rows("SELECT N FROM QSTEST/VB ORDER BY B"), vec![vec!["1"], vec!["2"], vec!["3"], vec!["4"], vec!["5"]]);
    assert_eq!(db.one("SELECT N FROM QSTEST/VB WHERE B = BX'0100'"), "3");
    assert!(db.run_with("INSERT INTO QSTEST/VB VALUES(BX'01', 9)", &[]).is_err());
    db.ok("CREATE INDEX QSTEST/VBD ON QSTEST/VB (B DESC)");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/VB WHERE B > BX'01'"), "3");
}

/// The LOB IDs a table's member keeps.
fn lob_ids(db: &mut Db, lib: &str, table: &str) -> Vec<u64> {
    let mut t = db.store.begin().unwrap();
    let info = db.ctx.resolve_table(&mut t, &sql::ast::ObjectName::qualified(lib, table)).unwrap();
    let ids = db::member::lob::ids(&mut t, info.member.object).unwrap();
    t.abort();
    ids
}

#[test]
fn lob_space_is_reclaimed() {
    let mut db = Db::sized(40000, 12000);
    let long = |c: char| Value::Str(c.to_string().repeat(3000));
    // Not journaled: an update or delete frees the old value at once.
    db.ok("CREATE TABLE QSTEST/LR (K INTEGER NOT NULL PRIMARY KEY, C CLOB(1M), B BLOB(1M))");
    db.run_with("INSERT INTO QSTEST/LR VALUES(1, ?, BX'01')", &[long('a')]).unwrap();
    db.run_with("INSERT INTO QSTEST/LR VALUES(2, ?, NULL)", &[long('b')]).unwrap();
    assert_eq!(lob_ids(&mut db, "QSTEST", "LR").len(), 2);
    db.run_with("UPDATE QSTEST/LR SET C = ? WHERE K = 1", &[long('c')]).unwrap();
    assert_eq!(lob_ids(&mut db, "QSTEST", "LR").len(), 2);
    db.ok("UPDATE QSTEST/LR SET B = BX'02' WHERE K = 1");
    assert_eq!(lob_ids(&mut db, "QSTEST", "LR").len(), 2);
    db.ok("DELETE FROM QSTEST/LR WHERE K = 2");
    assert_eq!(lob_ids(&mut db, "QSTEST", "LR").len(), 1);
    // A statement that fails frees nothing.
    assert!(db.run_with("UPDATE QSTEST/LR SET C = ?, K = 1 / 0 WHERE K = 1", &[long('d')]).is_err());
    assert_eq!(db.one("SELECT SUBSTR(C, 1, 3) FROM QSTEST/LR WHERE K = 1"), "ccc");

    // Journaled: the old values stay for the journal's before images
    // until RGZPFM, which keeps those the records name.
    db.ok("CREATE SCHEMA LOBJ");
    db.ok("CREATE TABLE LOBJ/LJ (K INTEGER NOT NULL PRIMARY KEY, C CLOB(1M))");
    db.run_with("INSERT INTO LOBJ/LJ VALUES(1, ?), (2, ?), (3, ?)", &[long('x'), long('y'), long('z')]).unwrap();
    db.run_with("UPDATE LOBJ/LJ SET C = ? WHERE K = 1", &[long('w')]).unwrap();
    db.ok("DELETE FROM LOBJ/LJ WHERE K = 2");
    assert_eq!(lob_ids(&mut db, "LOBJ", "LJ").len(), 4);
    {
        let mut t = db.store.begin().unwrap();
        let id = db.ctx.resolve_file(&mut t, &sql::ast::ObjectName::qualified("LOBJ", "LJ")).unwrap();
        let r = db::util::rgzpfm(&mut t, JobHandle(1), id, qsname::MemberSpec::First, &db::util::KeyFile::None).unwrap();
        assert_eq!((r.records, r.lobs_freed), (2, 2));
        t.commit().unwrap();
    }
    assert_eq!(lob_ids(&mut db, "LOBJ", "LJ").len(), 2);
    assert_eq!(
        db.rows("SELECT K, SUBSTR(C, 1, 2), LENGTH(C) FROM LOBJ/LJ ORDER BY K"),
        vec![vec!["1", "ww", "3000"], vec!["3", "zz", "3000"]]
    );
}

#[test]
fn lobs_copy() {
    let mut db = Db::sized(40000, 12000);
    db.ok("CREATE TABLE QSTEST/LC (K INTEGER NOT NULL PRIMARY KEY, C CLOB(10K), B BLOB(10K), D DBCLOB(100))");
    db.run_with(
        "INSERT INTO QSTEST/LC VALUES(1, ?, BX'00FF10', 'Ωmega'), (2, 'short', NULL, NULL)",
        &[Value::Str("q".repeat(2000))],
    )
    .unwrap();
    db.ok("CREATE TABLE QSTEST/LI (K INTEGER NOT NULL PRIMARY KEY, C CLOB(10K), B BLOB(10K), D DBCLOB(100))");
    let mut t = db.store.begin().unwrap();
    let from = db.ctx.resolve_file(&mut t, &sql::ast::ObjectName::qualified("QSTEST", "LC")).unwrap();
    let lib = db.ctx.library_for(&mut t, &sql::ast::ObjectName::qualified("QSTEST", "LC")).unwrap();
    let to = db::util::Target::Create { library: lib, name: Name::parse("LCCOPY").unwrap() };
    let opts = db::util::CpyfOpts { crtfile: true, ..db::util::CpyfOpts::default() };
    let r = db::util::cpyf(&mut t, JobHandle(1), from, &to, &opts).unwrap();
    assert_eq!(r.copied, 2);
    let text = db::util::cpytoimpf(&mut t, JobHandle(1), from, qsname::MemberSpec::First, &db::util::ImpfOpts::default()).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert!(text.contains(&"q".repeat(2000)), "{text}");
    assert!(text.contains("00FF10") && text.contains("Ωmega"), "{text}");
    let into = db.ctx.resolve_file(&mut t, &sql::ast::ObjectName::qualified("QSTEST", "LI")).unwrap();
    let rep = db::util::cpyfrmimpf(&mut t, JobHandle(1), into, qsname::MemberSpec::First, text.as_bytes(), &db::util::ImpfOpts::default())
        .unwrap();
    assert_eq!(rep.records, 2);
    t.commit().unwrap();
    // The copy's values are its own: the original's change does not reach them.
    db.ok("UPDATE QSTEST/LC SET C = 'changed'");
    for table in ["LCCOPY", "LI"] {
        assert_eq!(
            db.rows(&format!("SELECT K, LENGTH(C), HEX(B), D FROM QSTEST/{table} ORDER BY K")),
            vec![vec!["1", "2000", "00FF10", "Ωmega"], vec!["2", "5", "-", "-"]],
            "{table}"
        );
    }
}

#[test]
fn registers_catalog_and_predicates() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/SM (K INTEGER NOT NULL, T TIMESTAMP(3) NOT NULL DEFAULT CURRENT TIMESTAMP(3))");
    db.ok("INSERT INTO QSTEST/SM (K) VALUES(1)");
    assert_eq!(db.one("SELECT LENGTH(CHAR(CURRENT TIMESTAMP(0))) FROM SYSIBM.SYSDUMMY1"), "19");
    assert_eq!(db.one("SELECT LENGTH(CHAR(CURRENT_TIMESTAMP(12))) FROM SYSIBM.SYSDUMMY1"), "32");
    assert_eq!(db.one("SELECT LENGTH(CHAR(CURRENT TIMESTAMP)) FROM SYSIBM.SYSDUMMY1"), "26");
    assert_eq!(db.one("SELECT LENGTH(CHAR(T)) FROM QSTEST/SM"), "23");
    assert!(prepare("SELECT CURRENT TIMESTAMP(13) FROM SYSIBM.SYSDUMMY1", Naming::Sys).is_err());
    assert_eq!(
        db.rows("SELECT DATA_TYPE FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'SM' ORDER BY ORDINAL_POSITION"),
        vec![vec!["INTEGER"], vec!["TIMESTMP"]]
    );
    // A predicate as a value is a BOOLEAN.
    assert_eq!(db.values("SELECT K = 1, K > 1, K IN (1, 2) FROM QSTEST/SM")[0], vec![Value::Bool(true), Value::Bool(false), Value::Bool(true)]);
    let p = prepare("SELECT K = 1, CURRENT TIMESTAMP(9) FROM QSTEST/SM", Naming::Sys).unwrap();
    let mut t = db.store.begin().unwrap();
    let got: Vec<(i16, i32)> = sql::embed::describe::entries(&mut t, &db.ctx, &p.statement, false)
        .into_iter()
        .map(|(c, _, _)| (c.base, c.len))
        .collect();
    t.abort();
    assert_eq!(got, [(2436, 1), (392, 29)]);
}

/// Values equal but for the exponent, with the text each reads back as.
const EXPONENTS: [(&str, &str); 14] = [
    ("1.5", "1.5"),
    ("1.50", "1.50"),
    ("1.500", "1.500"),
    ("1E+2", "1E+2"),
    ("100", "100"),
    ("1.00E+2", "100"),
    ("10E+1", "1.0E+2"),
    ("0", "0"),
    ("0.00", "0.00"),
    ("0E+3", "0E+3"),
    ("-0", "-0"),
    ("-1.50", "-1.50"),
    ("0.000001", "0.000001"),
    ("1E-7", "1E-7"),
];

fn df(text: &str) -> Value {
    Value::DecFloat(qdecimal::DecFloat::parse(text, qdecimal::DfFormat::D34).unwrap())
}

#[test]
fn decfloat_keeps_its_exponent() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/DX (K INTEGER NOT NULL, A DECFLOAT(16), B DECFLOAT(34), PRIMARY KEY(K))");
    let both = |db: &mut Db, k: usize| db.rows(&format!("SELECT A, B FROM QSTEST/DX WHERE K = {k}"))[0].clone();
    for (i, &(text, back)) in EXPONENTS.iter().enumerate() {
        let want = vec![back.to_string(), back.to_string()];
        // An exponent literal is a double (see decfloat_from_doubles); a
        // string stands for the literal here.
        let lit = format!("'{text}'");
        db.ok(&format!("INSERT INTO QSTEST/DX VALUES({}, {lit}, {lit})", 100 + i));
        assert_eq!(both(&mut db, 100 + i), want, "literal {text}");
        // A string.
        db.ok(&format!("INSERT INTO QSTEST/DX VALUES({}, '{text}', '{text}')", 200 + i));
        assert_eq!(both(&mut db, 200 + i), want, "string {text}");
        // Parameters: a DECFLOAT(34), as a client's DECFLOAT arrives, and text.
        let k = Value::int(i64::try_from(300 + i).unwrap());
        db.run_with("INSERT INTO QSTEST/DX VALUES(?, ?, ?)", &[k, df(text), Value::Str(text.into())]).unwrap();
        assert_eq!(both(&mut db, 300 + i), want, "parameter {text}");
        // Update, by literal, by parameter, and column to column.
        db.ok(&format!("INSERT INTO QSTEST/DX VALUES({}, 7, 7)", 400 + i));
        db.ok(&format!("UPDATE QSTEST/DX SET B = {lit} WHERE K = {}", 400 + i));
        db.run_with(&format!("UPDATE QSTEST/DX SET A = ? WHERE K = {}", 400 + i), &[df(text)]).unwrap();
        assert_eq!(both(&mut db, 400 + i), want, "update {text}");
        db.ok(&format!("UPDATE QSTEST/DX SET A = 7, B = 7 WHERE K = {}", 400 + i));
        db.ok(&format!("UPDATE QSTEST/DX SET B = '{text}' WHERE K = {}", 400 + i));
        db.ok(&format!("UPDATE QSTEST/DX SET A = B WHERE K = {}", 400 + i));
        assert_eq!(both(&mut db, 400 + i), want, "column to column {text}");
        // Expressions that keep the exponent.
        let neg = back.strip_prefix('-').map_or_else(|| format!("-{back}"), str::to_string);
        assert_eq!(
            db.rows(&format!(
                "SELECT CAST(B AS DECFLOAT(16)), DECFLOAT(A), CAST('{text}' AS DECFLOAT), CAST({lit} AS DECFLOAT(16)), -B, \
                 NORMALIZE_DECFLOAT(B) = B FROM QSTEST/DX WHERE K = {}",
                200 + i
            ))[0],
            [back, back, back, back, neg.as_str(), "TRUE"].map(String::from).to_vec(),
            "expressions {text}"
        );
        // Fetched values cross processes as they are.
        for v in &db.values(&format!("SELECT A, B FROM QSTEST/DX WHERE K = {}", 300 + i))[0] {
            let w = sql::embed::to_wire(v);
            assert_eq!(w.1.trim_start_matches("16:"), back, "{text}");
            assert_eq!(sql::embed::from_wire(&w), *v, "{text}");
        }
    }
    // Equal values compare equal whatever their exponents.
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DX WHERE B = 1.5 AND K < 200"), "3");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DX WHERE B = 100 AND K < 200"), "4");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DX WHERE A = 0 AND K < 200"), "4");
    assert_eq!(db.one("SELECT COUNT(*) FROM QSTEST/DX WHERE B = 1E+2 AND K < 200"), "4");
    assert_eq!(db.rows("SELECT B FROM QSTEST/DX WHERE K < 103 ORDER BY B DESC, K"), [["1.5"], ["1.50"], ["1.500"]]);
    // Arithmetic's own exponents, an exponent literal beside a DECFLOAT
    // kept as written.
    let ex = |db: &mut Db, e: &str| db.one(&format!("SELECT {e} FROM QSTEST/DX WHERE K = 101"));
    assert_eq!(ex(&mut db, "B + 1E+2"), "101.50");
    assert_eq!(ex(&mut db, "B * 1E+2"), "150");
    assert_eq!(ex(&mut db, "B * 1E+3"), "1.50E+3");
    assert_eq!(ex(&mut db, "B - B"), "0.00");
    assert_eq!(ex(&mut db, "A * 2.0"), "3.000");
    assert_eq!(ex(&mut db, "0E+3 + B"), "1.50");
    assert_eq!(ex(&mut db, "B / 1E+2"), "0.0150");
    assert_eq!(ex(&mut db, "SUM(B)"), "1.50");
    assert_eq!(ex(&mut db, "CAST(1.00E+2 AS DOUBLE)"), "100");
    // A double becomes its exact value, normalized, as on the primary.
    assert_eq!(ex(&mut db, "CAST(CAST(1E+2 AS DOUBLE) AS DECFLOAT)"), "1E+2");
    assert_eq!(ex(&mut db, "CAST(CAST(1.5 AS DOUBLE) AS DECFLOAT)"), "1.5");
    // A DECFLOAT(16) on the wire keeps its format.
    let w = sql::embed::to_wire(&db.values("SELECT A FROM QSTEST/DX WHERE K = 103")[0][0]);
    assert_eq!(w, (6, "16:1E+2".to_string()));
}

#[test]
fn decfloat_from_doubles() {
    // The primary's values (tools/conformance/sql/cases/types/DECFLT1.sql, DECFLT2.sql).
    let mut db = Db::new();
    db.ok("CREATE TABLE QSTEST/DD (K INTEGER NOT NULL, A DECFLOAT(16), B DECFLOAT(34))");
    db.ok("INSERT INTO QSTEST/DD VALUES (3, -0, 1E-30), (4, 12345678901234567, 0.1E0)");
    assert_eq!(
        db.rows("SELECT CHAR(A), CHAR(B) FROM QSTEST/DD ORDER BY K"),
        [["0", "1.000000000000000083336420607585985E-30"], ["1.234567890123457E+16", "0.1000000000000000055511151231257827"]]
    );
    assert_eq!(db.one("SELECT CHAR(DECFLOAT(1.5E-30)) FROM QSTEST/DD WHERE K = 3"), "1.499999999999999949842322870776844E-30");
    assert_eq!(db.one("SELECT CHAR(DECFLOAT(100E0)) FROM QSTEST/DD WHERE K = 3"), "1E+2");
    assert_eq!(db.one("SELECT CHAR(DECFLOAT(1, 34) + 1.5E-30) FROM QSTEST/DD WHERE K = 3"), "1.000000000000000000000000000001500");
    assert_eq!(db.one("SELECT CHAR(CAST(0.1E0 AS DECFLOAT(16))) FROM QSTEST/DD WHERE K = 3"), "0.1");
    assert_eq!(db.one("SELECT CHAR(DECFLOAT(1) + CAST(0.1 AS REAL)) FROM QSTEST/DD WHERE K = 3"), "1.100000001490116119384765625");
    assert_eq!(db.one("SELECT CHAR(DECFLOAT(1.0E0) * 3) FROM QSTEST/DD WHERE K = 3"), "3");
}

#[test]
fn column_ccsids() {
    // The primary's answers (tools/conformance/sql/cases/types/LOB1.sql); its
    // SYSCOLUMNS spells VARGRAPHIC as VARG (the catalog's, not ours yet).
    let mut db = Db::new();
    let code = |db: &mut Db, s: &str| db.run_with(s, &[]).map_or_else(|e| e.sqlcode(), |_| 0);
    assert_eq!(code(&mut db, "CREATE TABLE QSTEST/C1 (G GRAPHIC(3) CCSID 37)"), -189);
    assert_eq!(code(&mut db, "CREATE TABLE QSTEST/C1 (G CHAR(3) CCSID 1200)"), -189);
    assert_eq!(code(&mut db, "CREATE TABLE QSTEST/C1 (H VARCHAR(3) FOR MIXED DATA)"), -604);
    assert_eq!(code(&mut db, "CREATE TABLE QSTEST/C1 (J CLOB(5) FOR BIT DATA)"), -683);
    for t in ["CHAR(3) CCSID 500", "VARCHAR(5) CCSID 65535", "NCHAR(2)", "NVARCHAR(4)", "NCLOB(10)", "VARGRAPHIC(4) CCSID 13488", "CHAR(3) FOR SBCS DATA"] {
        let r = db.run_with(&format!("CREATE TABLE QSTEST/C2 (X {t})"), &[]);
        assert!(r.is_ok(), "{t}: {r:?}");
        db.ok("DROP TABLE QSTEST/C2");
    }
    db.ok("CREATE TABLE QSTEST/C3 (A CHAR(3) CCSID 500, B VARCHAR(5) CCSID 65535, C NCHAR(2), D NVARCHAR(4), E NCLOB(10), F VARGRAPHIC(4) CCSID 13488)");
    db.ok("INSERT INTO QSTEST/C3 VALUES ('abc', 'ab', 'éx', 'ünï', 'nclob', 'grap')");
    assert_eq!(db.rows("SELECT A, HEX(B), C, D, E, F FROM QSTEST/C3")[0], ["abc", "8182", "éx", "ünï", "nclob", "grap"]);
    assert_eq!(
        db.rows("SELECT COLUMN_NAME, DATA_TYPE, LENGTH, CCSID FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'C3' ORDER BY ORDINAL_POSITION"),
        [
            ["A", "CHAR", "3", "500"],
            ["B", "VARCHAR", "5", "65535"],
            ["C", "GRAPHIC", "2", "1200"],
            ["D", "VARGRAPHIC", "4", "1200"],
            ["E", "DBCLOB", "10", "1200"],
            ["F", "VARGRAPHIC", "4", "13488"]
        ]
    );
}

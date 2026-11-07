// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL targets: `sql` parses statements, SQL PL bodies and SQL/JSON
//! paths; `sqlrun` runs the statements of an input against a store in
//! memory, so the engine's checks and evaluation meet the same inputs.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::exec::json::{path, value};
use sql::{Naming, SqlContext};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

/// The clock the store and the statements see.
const NOW: u64 = 1_767_225_600;

/// Parse as every statement form, both namings; the text as a JSON path
/// and a JSON document, and the path over the document.
pub fn parse(data: &[u8]) {
    let t = String::from_utf8_lossy(data);
    let _ = sql::parse_all(&t, Naming::Sql);
    let _ = sql::parse_all(&t, Naming::Sys);
    let p = path::parse(&t);
    let doc = value::parse(&t);
    if let (Ok(p), Ok(d)) = (p, doc) {
        let _ = p.eval(&d);
    }
}

/// Words that reach outside the process (HTTP, DNS, the IFS, commands):
/// an input naming one is not run.
const OUTSIDE: &[&str] = &["HTTP", "DNS_LOOKUP", "IFS_", "QCMDEXC", "SYSTOOLS", "SLEEP"];

/// Run the statements of `data` in order on a fresh store with a
/// library `FZ`, each its own transaction.
pub fn run(data: &[u8]) {
    let t = String::from_utf8_lossy(data);
    let upper = t.to_ascii_uppercase();
    if OUTSIDE.iter().any(|w| upper.contains(w)) {
        return;
    }
    let Ok(stmts) = sql::parse_all(&t, Naming::Sql) else {
        return;
    };
    let Some((mut store, mut ctx)) = fresh() else {
        return;
    };
    for st in stmts.into_iter().take(16) {
        let p = sql::Prepared {
            statement: st,
            naming: Naming::Sql,
        };
        let Ok(mut txn) = store.begin() else { return };
        let (ca, _) = p.run(&mut txn, &mut ctx, &[]);
        if ca.sqlcode < 0 {
            txn.abort();
        } else if txn.commit().is_err() {
            return;
        }
    }
}

/// A small store with QSYS and the library `FZ`, and a context over it.
fn fresh() -> Option<(Store, SqlContext)> {
    let dev = Arc::new(MemBlockDev::new(8192));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(dev) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .ok()?;
    let mut txn = store.begin().ok()?;
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").ok()?;
    let lib = ops::create(
        &mut txn,
        &CreateParams {
            library: qsys,
            name: Name::parse("FZ").ok()?,
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .ok()?;
    txn.commit().ok()?;
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
    );
    ctx.now = Some(NOW);
    ctx.current_schema = Name::parse("FZ").ok();
    ctx.user = "QSECOFR".into();
    Some((store, ctx))
}

/// Seeds: the grammar the SQL completion added (DDL, SQL PL, JSON, OLAP,
/// common table expressions), next to the Phase 3 queries.
pub const SEEDS: &[&[u8]] = &[
    b"SELECT A, COUNT(*) FROM T WHERE B > 1 GROUP BY A ORDER BY 2 FETCH FIRST 5 ROWS ONLY",
    b"CREATE TABLE FZ.ORDERS (ID INTEGER GENERATED ALWAYS AS IDENTITY (START WITH 10 INCREMENT BY 5), CUST CHAR(8) NOT NULL DEFAULT 'X', AMT DECIMAL(9, 2) CHECK (AMT >= 0), TS TIMESTAMP NOT NULL GENERATED ALWAYS FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP, PRIMARY KEY (ID), CONSTRAINT U1 UNIQUE (CUST))",
    b"CREATE TABLE FZ.T1 (A INT, B VARCHAR(20), C DECFLOAT(34), D BOOLEAN, E CLOB(1K)); INSERT INTO FZ.T1 VALUES (1, 'x', 1.5, TRUE, 'c'), (2, NULL, -1, FALSE, NULL); SELECT A, SUM(A) OVER (ORDER BY A ROWS BETWEEN 1 PRECEDING AND CURRENT ROW), LAG(B, 1, 'none') OVER (PARTITION BY D ORDER BY A) FROM FZ.T1",
    b"CREATE SEQUENCE FZ.S1 AS INTEGER START WITH 5 INCREMENT BY -1 MINVALUE 1 MAXVALUE 5 CYCLE CACHE 2; VALUES NEXT VALUE FOR FZ.S1; ALTER SEQUENCE FZ.S1 RESTART WITH 3",
    b"CREATE VIEW FZ.V1 (A, N) AS SELECT A, COUNT(*) FROM FZ.T1 GROUP BY A; CREATE INDEX FZ.I1 ON FZ.T1 (A DESC, B) WHERE A > 0; LABEL ON TABLE FZ.T1 IS 'x'; COMMENT ON COLUMN FZ.T1.A IS 'y'",
    b"WITH RECURSIVE R (N) AS (SELECT 1 FROM SYSIBM.SYSDUMMY1 UNION ALL SELECT N + 1 FROM R WHERE N < 5) SEARCH DEPTH FIRST BY N SET S SELECT N FROM R",
    b"VALUES (JSON_VALUE('{\"a\":[1,2,{\"b\":\"x\"}]}', 'lax $.a[2].b' RETURNING VARCHAR(10) DEFAULT 'd' ON EMPTY ERROR ON ERROR), JSON_QUERY('[1,2]', 'strict $[*]' WITH CONDITIONAL WRAPPER), JSON_OBJECT('k' VALUE 1 ABSENT ON NULL))",
    b"SELECT T.* FROM JSON_TABLE('{\"r\":[{\"a\":1},{\"a\":2}]}', '$.r[*]' COLUMNS (N FOR ORDINALITY, A INTEGER PATH '$.a', B VARCHAR(5) PATH '$.b' DEFAULT 'z' ON EMPTY)) AS T",
    b"CREATE OR REPLACE PROCEDURE FZ.P1 (IN X INTEGER, OUT Y VARCHAR(20)) LANGUAGE SQL BEGIN DECLARE I INTEGER DEFAULT 0; DECLARE C CURSOR WITH RETURN FOR SELECT A FROM FZ.T1; DECLARE CONTINUE HANDLER FOR SQLEXCEPTION SET Y = 'err'; L1: WHILE I < X DO SET I = I + 1; IF I = 3 THEN ITERATE L1; END IF; END WHILE; CASE WHEN X > 1 THEN SET Y = 'big'; ELSE SET Y = 'small'; END CASE; OPEN C; END",
    b"CREATE FUNCTION FZ.F1 (P DECIMAL(9, 2)) RETURNS DECIMAL(9, 2) LANGUAGE SQL DETERMINISTIC BEGIN ATOMIC DECLARE R DECIMAL(9, 2); SET R = P * 2; IF R > 100 THEN SIGNAL SQLSTATE '75001' SET MESSAGE_TEXT = 'too big'; END IF; RETURN R; END",
    b"CREATE TRIGGER FZ.TR1 BEFORE INSERT ON FZ.T1 REFERENCING NEW AS N FOR EACH ROW MODE DB2ROW WHEN (N.A < 0) BEGIN SET N.A = 0; END",
    b"MERGE INTO FZ.T1 T USING (VALUES (1, 'y')) AS S (A, B) ON T.A = S.A WHEN MATCHED AND T.B IS NULL THEN DELETE WHEN MATCHED THEN UPDATE SET B = S.B WHEN NOT MATCHED THEN INSERT (A, B) VALUES (S.A, S.B)",
    b"SELECT GRP, LISTAGG(S, ',') WITHIN GROUP (ORDER BY S DESC), PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY V) FROM FZ.T GROUP BY ROLLUP (GRP) HAVING GROUPING(GRP) = 0",
    b"DECLARE GLOBAL TEMPORARY TABLE SESSION.W AS (SELECT * FROM FZ.T1) WITH DATA ON COMMIT PRESERVE ROWS NOT LOGGED WITH REPLACE",
    b"lax $.a[1 to last].b",
    b"{\"a\":[{\"b\":1},{\"b\":[true,null,\"x\"]}],\"c\":-1.5e3}",
];

/// Tokens mutations insert.
pub const DICT: &[&[u8]] = &[
    b"SELECT ",
    b" FROM ",
    b" WHERE ",
    b" JOIN ",
    b"(",
    b")",
    b"'",
    b"CASE WHEN ",
    b" UNION ",
    b"MERGE INTO ",
    b"CREATE TABLE ",
    b"ALTER TABLE ",
    b" ADD COLUMN ",
    b" ALTER COLUMN ",
    b" SET DATA TYPE ",
    b"CREATE INDEX ",
    b"CREATE VIEW ",
    b"CREATE SEQUENCE ",
    b" GENERATED ALWAYS AS IDENTITY ",
    b" PRIMARY KEY ",
    b" REFERENCES ",
    b" ON DELETE CASCADE",
    b" CHECK (",
    b"LABEL ON ",
    b"COMMENT ON ",
    b"GRANT ",
    b"DROP ",
    b" RESTRICT",
    b" CASCADE",
    b"BEGIN ",
    b" END",
    b"DECLARE ",
    b" HANDLER FOR ",
    b"SIGNAL SQLSTATE ",
    b"RESIGNAL",
    b"GET DIAGNOSTICS ",
    b"FOR ",
    b" DO ",
    b"LEAVE ",
    b"ITERATE ",
    b"WHILE ",
    b"REPEAT ",
    b"UNTIL ",
    b"RETURN ",
    b"CALL ",
    b"EXECUTE IMMEDIATE ",
    b"SET ",
    b";",
    b" OVER (",
    b"PARTITION BY ",
    b" ROWS BETWEEN ",
    b" UNBOUNDED PRECEDING",
    b" FOLLOWING",
    b"ROW_NUMBER()",
    b"LAG(",
    b"NTILE(",
    b"WITH ",
    b"RECURSIVE ",
    b" AS (",
    b" SEARCH DEPTH FIRST BY ",
    b" CYCLE ",
    b"JSON_VALUE(",
    b"JSON_TABLE(",
    b"JSON_OBJECT(",
    b"JSON_ARRAYAGG(",
    b"$.",
    b"[*]",
    b"lax ",
    b"strict ",
    b" COLUMNS (",
    b" PATH ",
    b"DECFLOAT(",
    b"BOOLEAN",
    b"CLOB(",
    b"VARGRAPHIC(",
    b" FOR BIT DATA",
    b" CCSID 1200",
    b"CAST(",
    b"TIMESTAMP(12)",
];

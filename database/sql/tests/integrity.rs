// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Table integrity from SQL: constraints with the SQLCODEs the primary
//! gives (`tools/conformance/sql/oracle/integrity/`), referential rules,
//! identity columns, sequences, row change timestamps and ALTER TABLE.

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
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
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
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
        ctx.now = Some(1_767_225_600);
        ctx.user = "QSECOFR".into();
        let mut db = Self { store, ctx };
        // A schema, so its tables are journaled (cascades need it).
        db.ok("CREATE SCHEMA IT");
        db
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
        self.run(text)
            .unwrap_or_else(|e| panic!("{text}: {e} ({})", e.sqlcode()))
    }

    fn rows(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|v| {
                            if v.is_null() {
                                "-".into()
                            } else {
                                v.to_string().trim_end().to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect(),
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

fn c(code: i32, state: &str) -> (i32, String) {
    (code, state.to_string())
}

#[test]
fn constraints_as_the_primary_answers() {
    let mut db = Db::new();
    db.ok("CREATE TABLE IT.IPARENT (ID INTEGER NOT NULL, NAME CHAR(10) NOT NULL WITH DEFAULT, AMT DECIMAL(7,2), CONSTRAINT IPARENT_PK PRIMARY KEY (ID), CONSTRAINT IPARENT_AMT CHECK (AMT >= 0), CONSTRAINT IPARENT_UQ UNIQUE (NAME))");
    db.ok("CREATE TABLE IT.ICHILD (CID INTEGER NOT NULL PRIMARY KEY, PID INTEGER, QTY INTEGER DEFAULT 1, CONSTRAINT ICHILD_FK FOREIGN KEY (PID) REFERENCES IT.IPARENT (ID) ON DELETE RESTRICT ON UPDATE RESTRICT)");
    db.ok("INSERT INTO IT.IPARENT VALUES (1, 'ONE', 1.00), (2, 'TWO', 2.00)");
    assert_eq!(
        db.code("INSERT INTO IT.IPARENT VALUES (3, 'ONE', 1.00)"),
        c(-803, "23505")
    );
    assert_eq!(
        db.code("INSERT INTO IT.IPARENT VALUES (1, 'XXX', 1.00)"),
        c(-803, "23505")
    );
    assert_eq!(
        db.code("INSERT INTO IT.IPARENT VALUES (4, 'FOUR', -1.00)"),
        c(-545, "23513")
    );
    assert_eq!(
        db.code("UPDATE IT.IPARENT SET AMT = -5 WHERE ID = 1"),
        c(-545, "23513")
    );
    db.ok("INSERT INTO IT.ICHILD (CID, PID) VALUES (10, 1)");
    assert_eq!(
        db.code("INSERT INTO IT.ICHILD (CID, PID) VALUES (11, 9)"),
        c(-530, "23503")
    );
    db.ok("INSERT INTO IT.ICHILD (CID, PID) VALUES (12, NULL)");
    assert_eq!(
        db.code("UPDATE IT.ICHILD SET PID = 8 WHERE CID = 10"),
        c(-530, "23503")
    );
    assert_eq!(
        db.code("DELETE FROM IT.IPARENT WHERE ID = 1"),
        c(-532, "23001")
    );
    assert_eq!(
        db.code("UPDATE IT.IPARENT SET ID = 5 WHERE ID = 1"),
        c(-531, "23001")
    );
    db.ok("UPDATE IT.IPARENT SET ID = 6 WHERE ID = 2");
    assert_eq!(
        db.rows("SELECT CONSTRAINT_NAME, CONSTRAINT_TYPE, CONSTRAINT_KEYS, ENABLED, CHECK_PENDING FROM QSYS2.SYSCST WHERE TABLE_SCHEMA = 'IT' ORDER BY CONSTRAINT_NAME"),
        [
            "ICHILD_FK|FOREIGN KEY|1|YES|NO",
            "IPARENT_AMT|CHECK|-|YES|NO",
            "IPARENT_PK|PRIMARY KEY|1|YES|-",
            "IPARENT_UQ|UNIQUE|1|YES|-",
            "Q_IT_ICHILD_CID_00001|PRIMARY KEY|1|YES|-",
        ]
    );
    assert_eq!(db.rows("SELECT CONSTRAINT_NAME, CHECK_CLAUSE FROM QSYS2.SYSCHKCST WHERE CONSTRAINT_SCHEMA = 'IT'"), ["IPARENT_AMT|AMT >= 0"]);
    assert_eq!(
        db.rows("SELECT CONSTRAINT_NAME, UNIQUE_CONSTRAINT_NAME, MATCH_OPTION, UPDATE_RULE, DELETE_RULE, COLUMN_COUNT FROM QSYS2.SYSREFCST WHERE CONSTRAINT_SCHEMA = 'IT'"),
        ["ICHILD_FK|IPARENT_PK|NONE|RESTRICT|RESTRICT|1"]
    );
    assert_eq!(
        db.rows("SELECT CONSTRAINT_NAME, COLUMN_NAME, ORDINAL_POSITION, COLUMN_POSITION FROM QSYS2.SYSKEYCST WHERE TABLE_SCHEMA = 'IT' ORDER BY 1"),
        ["ICHILD_FK|PID|1|2", "IPARENT_PK|ID|1|1", "IPARENT_UQ|NAME|1|2", "Q_IT_ICHILD_CID_00001|CID|1|1"]
    );
    assert_eq!(
        db.code("ALTER TABLE IT.IPARENT DROP CONSTRAINT IPARENT_PK RESTRICT"),
        c(-616, "42893")
    );
    // DROP PRIMARY KEY takes the foreign keys over it.
    db.ok("ALTER TABLE IT.IPARENT DROP PRIMARY KEY");
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSREFCST WHERE CONSTRAINT_SCHEMA = 'IT'"),
        ["0"]
    );
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD DROP CONSTRAINT NOPE"),
        c(-204, "42704")
    );
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD CONSTRAINT ICHILD_QTY CHECK (QTY > 5)"),
        c(-544, "23512")
    );
    db.ok("ALTER TABLE IT.ICHILD ADD CONSTRAINT ICHILD_Q2 CHECK (QTY < 100)");
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD CONSTRAINT ICHILD_PK2 PRIMARY KEY (PID)"),
        c(-624, "42889")
    );
    db.ok("INSERT INTO IT.ICHILD (CID, PID) VALUES (13, NULL)");
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD UNIQUE (QTY)"),
        c(-603, "23515")
    );
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD CONSTRAINT IPARENT_UQ UNIQUE (CID)"),
        c(-601, "42710")
    );
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD CONSTRAINT C3 CHECK (CURRENT DATE > '2000-01-01')"),
        c(-546, "42621")
    );
    db.ok("ALTER TABLE IT.ICHILD ADD CONSTRAINT C4 CHECK (UPPER('A') = 'A' AND QTY BETWEEN -5 AND 99 AND COALESCE(PID, 0) >= -5)");
    assert_eq!(
        db.code("ALTER TABLE IT.ICHILD ADD CONSTRAINT C5 UNIQUE (CID, CID)"),
        c(-537, "42709")
    );
    db.ok("DROP TABLE IT.ICHILD");
    db.ok("DROP TABLE IT.IPARENT");
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSCST WHERE TABLE_SCHEMA = 'IT'"),
        ["0"]
    );
}

#[test]
fn referential_rules() {
    let mut db = Db::new();
    db.ok(
        "CREATE TABLE IT.RPAR (ID INTEGER NOT NULL PRIMARY KEY, AMT DECIMAL(7,2) CHECK (AMT >= 0))",
    );
    db.ok("CREATE TABLE IT.RCH1 (CID INTEGER NOT NULL, PID INTEGER, CONSTRAINT RCH1_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON DELETE CASCADE)");
    db.ok("CREATE TABLE IT.RCH2 (CID INTEGER NOT NULL, PID INTEGER, CONSTRAINT RCH2_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON DELETE SET NULL)");
    db.ok("CREATE TABLE IT.RCH3 (CID INTEGER NOT NULL, PID INTEGER NOT NULL DEFAULT 0, CONSTRAINT RCH3_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON DELETE SET DEFAULT)");
    assert_eq!(
        db.code("CREATE TABLE IT.RCH4 (CID INTEGER NOT NULL, PID INTEGER NOT NULL, CONSTRAINT RCH4_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON DELETE SET NULL)"),
        c(-629, "42834")
    );
    assert_eq!(db.code("CREATE TABLE IT.RCH5 (CID INTEGER NOT NULL, PID INTEGER, CONSTRAINT RCH5_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON UPDATE CASCADE)").0, -104);
    db.ok("CREATE TABLE IT.RCH6 (CID INTEGER NOT NULL, PID INTEGER, CONSTRAINT RCH6_FK FOREIGN KEY (PID) REFERENCES IT.RPAR ON DELETE NO ACTION ON UPDATE NO ACTION)");
    assert_eq!(db.code("CREATE TABLE IT.RCH7 (CID INTEGER NOT NULL, PID CHAR(4), CONSTRAINT RCH7_FK FOREIGN KEY (PID) REFERENCES IT.RPAR)"), c(-538, "42830"));
    assert_eq!(db.code("CREATE TABLE IT.RCH8 (CID INTEGER NOT NULL, PID INTEGER, CONSTRAINT RCH8_FK FOREIGN KEY (PID) REFERENCES IT.RPAR (AMT))"), c(-538, "42830"));
    db.ok("INSERT INTO IT.RPAR VALUES (0, 0), (1, 1), (2, 2)");
    db.ok("INSERT INTO IT.RCH1 VALUES (10, 1), (11, 2)");
    db.ok("INSERT INTO IT.RCH2 VALUES (20, 2)");
    db.ok("INSERT INTO IT.RCH3 VALUES (30, 2)");
    db.ok("INSERT INTO IT.RCH6 VALUES (60, 1)");
    assert_eq!(
        db.code("DELETE FROM IT.RPAR WHERE ID = 1"),
        c(-532, "23504")
    );
    assert_eq!(
        db.code("UPDATE IT.RPAR SET ID = 3 WHERE ID = 1"),
        c(-531, "23504")
    );
    db.ok("DELETE FROM IT.RPAR WHERE ID = 2");
    assert_eq!(db.rows("SELECT * FROM IT.RCH1"), ["10|1"]);
    assert_eq!(db.rows("SELECT * FROM IT.RCH2"), ["20|-"]);
    assert_eq!(db.rows("SELECT * FROM IT.RCH3"), ["30|0"]);
    assert_eq!(db.code("DROP TABLE IT.RPAR RESTRICT"), c(-478, "42893"));
    db.ok("DROP TABLE IT.RPAR CASCADE");
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSREFCST WHERE CONSTRAINT_SCHEMA = 'IT'"),
        ["0"]
    );
}

#[test]
fn identity_columns_and_sequences() {
    let mut db = Db::new();
    db.ok("CREATE TABLE IT.IDT (ID SMALLINT GENERATED ALWAYS AS IDENTITY (START WITH 32766 INCREMENT BY 1 NO CYCLE), V CHAR(3))");
    db.ok("INSERT INTO IT.IDT (V) VALUES ('a')");
    assert_eq!(db.rows("VALUES IDENTITY_VAL_LOCAL()"), ["32766"]);
    db.ok("INSERT INTO IT.IDT (V) VALUES ('b')");
    assert_eq!(
        db.code("INSERT INTO IT.IDT (V) VALUES ('c')"),
        c(-359, "23522")
    );
    assert_eq!(
        db.code("INSERT INTO IT.IDT VALUES (5, 'd')"),
        c(-798, "428C9")
    );
    db.ok("INSERT INTO IT.IDT OVERRIDING SYSTEM VALUE VALUES (5, 'd')");
    db.ok("ALTER TABLE IT.IDT ALTER COLUMN ID RESTART WITH 1");
    db.ok("INSERT INTO IT.IDT (V) VALUES ('e')");
    assert_eq!(
        db.rows("SELECT ID, V FROM IT.IDT ORDER BY V"),
        ["32766|a", "32767|b", "5|d", "1|e"]
    );
    db.ok("ALTER TABLE IT.IDT ALTER COLUMN ID SET INCREMENT BY 10");
    db.ok("INSERT INTO IT.IDT (V) VALUES ('f')");
    db.ok("INSERT INTO IT.IDT (V) VALUES ('g')");
    // The values IBM had cached (a block of 20 from 1) are passed over.
    assert_eq!(
        db.rows("SELECT ID FROM IT.IDT WHERE V IN ('f', 'g') ORDER BY V"),
        ["30", "40"]
    );
    db.ok("CREATE TABLE IT.CYC (ID BIGINT GENERATED BY DEFAULT AS IDENTITY (START WITH 3 MINVALUE 1 MAXVALUE 4 CYCLE), V INT)");
    for _ in 0..3 {
        db.ok("INSERT INTO IT.CYC (V) VALUES (0)");
    }
    assert_eq!(db.rows("SELECT ID FROM IT.CYC"), ["3", "4", "1"]);
    assert_eq!(
        db.code(
            "CREATE TABLE IT.BAD (ID INT GENERATED ALWAYS AS IDENTITY (MINVALUE 5 MAXVALUE 1))"
        )
        .0,
        -846
    );
    db.ok("CREATE SEQUENCE IT.ORDSEQ AS BIGINT START WITH 100 INCREMENT BY 5 MAXVALUE 110");
    assert_eq!(
        db.code("VALUES PREVIOUS VALUE FOR IT.ORDSEQ"),
        c(-845, "51035")
    );
    assert_eq!(db.rows("VALUES NEXT VALUE FOR IT.ORDSEQ"), ["100"]);
    assert_eq!(db.rows("VALUES NEXT VALUE FOR IT.ORDSEQ"), ["105"]);
    assert_eq!(db.rows("VALUES PREVIOUS VALUE FOR IT.ORDSEQ"), ["105"]);
    assert_eq!(db.rows("VALUES NEXT VALUE FOR IT.ORDSEQ"), ["110"]);
    assert_eq!(db.code("VALUES NEXT VALUE FOR IT.ORDSEQ"), c(-359, "23522"));
    db.ok("ALTER SEQUENCE IT.ORDSEQ RESTART WITH 101");
    assert_eq!(db.rows("VALUES NEXT VALUE FOR IT.ORDSEQ"), ["101"]);
    assert_eq!(
        db.rows("SELECT SEQUENCE_NAME, DATA_TYPE, START, INCREMENT, MAXIMUM_VALUE, CYCLE_OPTION, MAXASSIGNEDVAL FROM QSYS2.SYSSEQUENCES WHERE SEQUENCE_SCHEMA = 'IT'"),
        ["ORDSEQ|BIGINT|100|5|110|NO|106"]
    );
    assert_eq!(db.code("CREATE SEQUENCE IT.ORDSEQ").0, -601);
    // An identity column added numbers the rows there.
    db.ok("CREATE TABLE IT.ADDID (V INT)");
    db.ok("INSERT INTO IT.ADDID VALUES (7), (8), (9)");
    assert_eq!(
        db.code("ALTER TABLE IT.ADDID ALTER COLUMN V RESTART WITH 3"),
        c(-190, "42837")
    );
    db.ok("ALTER TABLE IT.ADDID ADD COLUMN Z BIGINT GENERATED ALWAYS AS IDENTITY");
    assert_eq!(db.rows("SELECT V, Z FROM IT.ADDID"), ["7|1", "8|2", "9|3"]);
    assert_eq!(
        db.code("ALTER TABLE IT.ADDID ADD COLUMN Y INT GENERATED ALWAYS AS IDENTITY"),
        c(-372, "428C1")
    );
    db.ok("DROP SEQUENCE IT.ORDSEQ");
    assert_eq!(db.code("VALUES NEXT VALUE FOR IT.ORDSEQ").0, -204);
}

#[test]
fn row_change_timestamps_and_alter_columns() {
    let mut db = Db::new();
    db.ok("CREATE TABLE IT.RCT (K INT NOT NULL PRIMARY KEY, V CHAR(5), TS TIMESTAMP NOT NULL GENERATED ALWAYS FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP, WHO VARCHAR(128) GENERATED ALWAYS AS (SESSION_USER), OP CHAR(1) GENERATED ALWAYS AS (DATA CHANGE OPERATION))");
    db.ok("INSERT INTO IT.RCT (K, V) VALUES (1, 'a')");
    assert_eq!(db.rows("SELECT WHO, OP FROM IT.RCT"), ["QSECOFR|I"]);
    db.ok("UPDATE IT.RCT SET V = 'b'");
    assert_eq!(db.rows("SELECT OP FROM IT.RCT"), ["U"]);
    assert_eq!(db.code("UPDATE IT.RCT SET OP = 'X'"), c(-798, "428C9"));
    db.ok("ALTER TABLE IT.RCT ADD COLUMN N DECIMAL(5, 0) NOT NULL DEFAULT 7 ADD CONSTRAINT N_OK CHECK (N > 0)");
    assert_eq!(db.rows("SELECT K, V, N FROM IT.RCT"), ["1|b|7"]);
    assert_eq!(
        db.code("ALTER TABLE IT.RCT DROP COLUMN N RESTRICT"),
        c(-478, "42893")
    );
    db.ok("ALTER TABLE IT.RCT DROP COLUMN N CASCADE");
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSCHKCST WHERE CONSTRAINT_NAME = 'N_OK'"),
        ["0"]
    );
    db.ok("ALTER TABLE IT.RCT ALTER COLUMN V SET DATA TYPE CHAR(8)");
    assert_eq!(
        db.code("INSERT INTO IT.RCT (K, V) VALUES (1, 'dup')"),
        c(-803, "23505")
    );
    assert_eq!(
        db.code("ALTER TABLE IT.RCT ADD PARTITION BY RANGE (K)").0,
        -84
    );
    assert_eq!(
        db.code("CREATE TABLE IT.HID (A INT, B INT IMPLICITLY HIDDEN)")
            .0,
        -84
    );
}

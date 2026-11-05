// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Objects and names: long SQL names and generated system names,
//! schemas, views as queries, CREATE TABLE AS and LIKE, OR REPLACE,
//! DROP with RESTRICT, RENAME, index extensions, the catalogs showing
//! both names. Expectations follow tools/conformance/oracle/sql-ddl.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(32768, 17));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 256,
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
        ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
        Self { store, ctx }
    }

    fn run(&mut self, text: &str) -> (sql::Sqlca, Option<Outcome>) {
        let p = match prepare(text, Naming::Sql) {
            Ok(p) => p,
            Err(e) => return (sql::Sqlca::from_error(&e), None),
        };
        let mut t = self.store.begin().unwrap();
        let r = p.run(&mut t, &mut self.ctx, &[]);
        if r.0.is_error() {
            t.abort();
        } else {
            t.commit().unwrap();
        }
        r
    }

    fn ok(&mut self, text: &str) -> Outcome {
        let (ca, o) = self.run(text);
        assert!(!ca.is_error(), "{text}: {}", ca.line());
        o.unwrap()
    }

    fn code(&mut self, text: &str) -> i32 {
        self.run(text).0.sqlcode
    }

    fn rows(&mut self, text: &str) -> Vec<Vec<String>> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| r.iter().map(|v| if v.is_null() { "-".into() } else { v.to_string().trim_end().to_string() }).collect())
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn columns(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs.columns.iter().map(|c| c.name.clone()).collect(),
            o => panic!("{text}: {o:?}"),
        }
    }
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

#[test]
fn long_names_get_ibm_system_names() {
    let mut db = Db::new();
    let (ca, _) = db.run("CREATE TABLE QSTEST.CUSTOMER_MASTER_FILE (CUSTOMER_NUMBER INT, CUSTOMER_NAME_LONG CHAR(10), SHORT CHAR(1), \"lower\" INT, A_B_C_D_E_F INT)");
    assert_eq!((ca.sqlcode, ca.sqlstate.as_str()), (7905, "01567"), "not a schema: not journaled");
    db.ok("CREATE TABLE QSTEST.CUSTOMER_MASTER_FILE2 (X INT)");
    db.ok("CREATE TABLE QSTEST.CUSTO00001X (X INT)");
    db.ok("CREATE TABLE QSTEST.\"lowtab\" (X INT)");
    db.ok("CREATE TABLE QSTEST.\"a long lower name\" (X INT)");
    db.ok("CREATE TABLE QSTEST.A_LONGER_NAME_X (X INT)");
    assert_eq!(
        db.rows("SELECT TABLE_NAME, SYSTEM_TABLE_NAME, TABLE_TYPE, TABLE_TEXT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST' ORDER BY SYSTEM_TABLE_NAME"),
        [
            row(&["a long lower name", "\"a_lo0001\"", "T", ""]),
            row(&["lowtab", "\"lowtab\"", "T", ""]),
            row(&["A_LONGER_NAME_X", "A_LON00001", "T", ""]),
            row(&["CUSTOMER_MASTER_FILE", "CUSTO00001", "T", ""]),
            row(&["CUSTOMER_MASTER_FILE2", "CUSTO00002", "T", ""]),
            row(&["CUSTO00001X", "CUSTO00003", "T", ""]),
        ]
    );
    assert_eq!(
        db.rows("SELECT COLUMN_NAME, SYSTEM_COLUMN_NAME, TABLE_NAME, SYSTEM_TABLE_NAME FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME = 'CUSTOMER_MASTER_FILE' ORDER BY ORDINAL_POSITION"),
        [
            row(&["CUSTOMER_NUMBER", "CUSTO00001", "CUSTOMER_MASTER_FILE", "CUSTO00001"]),
            row(&["CUSTOMER_NAME_LONG", "CUSTO00002", "CUSTOMER_MASTER_FILE", "CUSTO00001"]),
            row(&["SHORT", "SHORT", "CUSTOMER_MASTER_FILE", "CUSTO00001"]),
            row(&["lower", "LOWER00001", "CUSTOMER_MASTER_FILE", "CUSTO00001"]),
            row(&["A_B_C_D_E_F", "A_B_C00001", "CUSTOMER_MASTER_FILE", "CUSTO00001"]),
        ]
    );
    // Either name finds the table; columns by either name.
    db.ok("INSERT INTO CUSTOMER_MASTER_FILE (CUSTOMER_NUMBER, SHORT) VALUES (1, 'A')");
    assert_eq!(db.rows("SELECT CUSTO00001, CUSTOMER_NUMBER, SHORT FROM CUSTO00001"), [row(&["1", "1", "A"])]);
    assert_eq!(db.rows("SELECT CUSTOMER_NUMBER FROM QSTEST.CUSTOMER_MASTER_FILE"), [row(&["1"])]);
    assert_eq!(db.code("SELECT * FROM NO_SUCH_LONG_TABLE_NAME"), -204);
    // DROP takes the index entry with it: the name is free again.
    db.ok("DROP TABLE CUSTOMER_MASTER_FILE2");
    assert_eq!(db.code("SELECT * FROM CUSTOMER_MASTER_FILE2"), -204);
    db.ok("CREATE TABLE QSTEST.CUSTOMER_MASTER_FILE3 (X INT)");
    assert_eq!(db.rows("SELECT SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_NAME = 'CUSTOMER_MASTER_FILE3'"), [row(&["CUSTO00002"])]);
    assert_eq!(db.code("CREATE TABLE QSTEST.CUSTOMER_MASTER_FILE (X INT)"), -601);
    let long = "X".repeat(129);
    assert_eq!(db.code(&format!("CREATE TABLE {long} (X INT)")), -107);
}

#[test]
fn for_system_name_and_rename() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSP1_LONG_TABLE_NAME FOR SYSTEM NAME QSP1T (ID INT NOT NULL, CUSTOMER_NAME FOR COLUMN CUSNAM VARCHAR(20))");
    assert_eq!(db.code("CREATE TABLE QSP1V FOR SYSTEM NAME QSP1VS (X INT)"), -7029);
    db.ok("INSERT INTO QSP1T VALUES (1, 'A')");
    assert_eq!(db.rows("SELECT CUSTOMER_NAME, CUSNAM FROM QSP1_LONG_TABLE_NAME"), [row(&["A", "A"])]);
    db.ok("RENAME TABLE QSP1_LONG_TABLE_NAME TO SYSTEM NAME QSP1U");
    assert_eq!(db.rows("SELECT TABLE_NAME, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST'"), [row(&["QSP1_LONG_TABLE_NAME", "QSP1U"])]);
    db.ok("RENAME TABLE QSP1U TO QSP1_RENAMED FOR SYSTEM NAME QSP1W");
    assert_eq!(db.rows("SELECT TABLE_NAME, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST'"), [row(&["QSP1_RENAMED", "QSP1W"])]);
    assert_eq!(db.code("SELECT * FROM QSP1_LONG_TABLE_NAME"), -204);
    db.ok("CREATE TABLE QSP1C (X INT)");
    db.ok("RENAME TABLE QSP1C TO QSP1C_LONGER_NAME");
    assert_eq!(db.rows("SELECT SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_NAME = 'QSP1C_LONGER_NAME'"), [row(&["QSP1C00001"])]);
    db.ok("RENAME TABLE QSP1C_LONGER_NAME TO QSP1X");
    assert_eq!(db.rows("SELECT TABLE_NAME, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_NAME = 'QSP1X'"), [row(&["QSP1X", "QSP1X"])]);
    // A long name keeps a generated system name of its prefix.
    db.ok("CREATE TABLE CUSTOMER_MASTER_FILE2 (X INT)");
    db.ok("RENAME TABLE CUSTOMER_MASTER_FILE2 TO CUSTOMER_RENAMED_TABLE");
    assert_eq!(db.rows("SELECT SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_NAME = 'CUSTOMER_RENAMED_TABLE'"), [row(&["CUSTO00001"])]);
    db.ok("CREATE INDEX CUSTOMER_INDEX_LONG ON CUSTOMER_RENAMED_TABLE (X)");
    db.ok("RENAME INDEX CUSTOMER_INDEX_LONG TO CUSTOMER_INDEX_RENAMED");
    assert_eq!(db.rows("SELECT INDEX_NAME, SYSTEM_INDEX_NAME, TABLE_NAME FROM QSYS2.SYSINDEXES"), [row(&["CUSTOMER_INDEX_RENAMED", "CUSTO00002", "CUSTOMER_RENAMED_TABLE"])]);
    db.ok("DROP INDEX CUSTOMER_INDEX_RENAMED");
}

#[test]
fn views_run_their_queries() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSV1T (ID INT NOT NULL, NAME VARCHAR(20), AMT DECIMAL(7, 2))");
    db.ok("CREATE TABLE QSV1O (ID INT NOT NULL, QTY INT)");
    db.ok("INSERT INTO QSV1T VALUES (1, 'A', 1.5), (2, 'B', 2.5), (3, NULL, 3.5)");
    db.ok("INSERT INTO QSV1O VALUES (1, 10), (2, 20), (2, 5)");
    // A plain selection stays a logical file.
    db.ok("CREATE VIEW QSV1V (I, N) AS SELECT ID, NAME FROM QSV1T WHERE ID > 1");
    // Expressions, joins, grouping and unions run as queries.
    db.ok("CREATE VIEW QSV1W AS SELECT ID, AMT * 2 AS DBL, UPPER(NAME) AS UNAME FROM QSV1T WHERE AMT < 3");
    db.ok("CREATE VIEW QSV1J AS SELECT T.ID, T.NAME, SUM(O.QTY) AS TOTAL FROM QSV1T T JOIN QSV1O O ON T.ID = O.ID GROUP BY T.ID, T.NAME");
    db.ok("CREATE VIEW QSV1X AS SELECT * FROM QSV1V WHERE I < 3");
    db.ok("CREATE VIEW CUSTOMER_UNION_VIEW (K) AS SELECT ID FROM QSV1T UNION SELECT QTY FROM QSV1O");
    assert_eq!(db.code("CREATE VIEW QSV1Y AS SELECT ID FROM QSV1T UNION SELECT QTY + 1 FROM QSV1O"), -153);
    assert_eq!(db.rows("SELECT * FROM QSV1W ORDER BY ID"), [row(&["1", "3.00", "A"]), row(&["2", "5.00", "B"])]);
    assert_eq!(db.rows("SELECT ID, TOTAL FROM QSV1J ORDER BY ID"), [row(&["1", "10"]), row(&["2", "25"])]);
    assert_eq!(db.rows("SELECT I, N FROM QSV1X"), [row(&["2", "B"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM CUSTOMER_UNION_VIEW"), [row(&["6"])]);
    assert_eq!(db.columns("SELECT * FROM QSV1W"), ["ID", "DBL", "UNAME"]);
    // A view in a join and a subquery.
    assert_eq!(db.rows("SELECT V.UNAME FROM QSV1W V JOIN QSV1T T ON V.ID = T.ID WHERE T.ID = 2"), [row(&["B"])]);
    assert_eq!(db.rows("SELECT ID FROM QSV1T WHERE ID IN (SELECT ID FROM QSV1J WHERE TOTAL > 20)"), [row(&["2"])]);
    assert_eq!(
        db.rows("SELECT TABLE_NAME, CHECK_OPTION, VIEW_DEFINITION, IS_UPDATABLE FROM QSYS2.SYSVIEWS ORDER BY TABLE_NAME"),
        [
            row(&["CUSTOMER_UNION_VIEW", "N", "SELECT ID FROM QSV1T UNION SELECT QTY FROM QSV1O", "N"]),
            row(&["QSV1J", "N", "SELECT T.ID, T.NAME, SUM(O.QTY) AS TOTAL FROM QSV1T T JOIN QSV1O O ON T.ID = O.ID GROUP BY T.ID, T.NAME", "N"]),
            row(&["QSV1V", "N", "SELECT ID, NAME FROM QSV1T WHERE ID > 1", "Y"]),
            row(&["QSV1W", "N", "SELECT ID, AMT * 2 AS DBL, UPPER(NAME) AS UNAME FROM QSV1T WHERE AMT < 3", "N"]),
            row(&["QSV1X", "N", "SELECT * FROM QSV1V WHERE I < 3", "N"]),
        ]
    );
    assert_eq!(
        db.rows("SELECT VIEW_NAME, OBJECT_NAME, OBJECT_TYPE FROM QSYS2.SYSVIEWDEP WHERE VIEW_NAME IN ('QSV1J', 'QSV1X') ORDER BY 1, 2"),
        [row(&["QSV1J", "QSV1O", "TABLE"]), row(&["QSV1J", "QSV1T", "TABLE"]), row(&["QSV1X", "QSV1V", "VIEW"])]
    );
    // The view resolves its tables where it was made, whatever the
    // current schema now.
    db.ctx.current_schema = None;
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSTEST.QSV1J"), [row(&["2"])]);
    db.ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
    // A read-only view refuses changes; a check option on one is refused.
    assert!(db.code("INSERT INTO QSV1W (ID) VALUES (9)") < 0);
    assert_eq!(db.code("CREATE VIEW QSV1C AS SELECT ID, AMT * 2 AS D FROM QSV1T WITH CHECK OPTION"), -84);
    // OR REPLACE of a view; of a table: SQL0601.
    db.ok("CREATE OR REPLACE VIEW QSV1W AS SELECT ID FROM QSV1T WHERE ID = 3");
    assert_eq!(db.rows("SELECT * FROM QSV1W"), [row(&["3"])]);
    assert_eq!(db.code("CREATE OR REPLACE VIEW QSV1T AS SELECT 1 AS X FROM QSV1O"), -601);
    // RESTRICT with views over it; CASCADE takes them.
    assert_eq!(db.code("DROP TABLE QSV1T RESTRICT"), -616);
    assert_eq!(db.code("DROP VIEW QSV1V RESTRICT"), -616);
    db.ok("DROP VIEW QSV1V CASCADE");
    assert_eq!(db.code("SELECT * FROM QSV1X"), -204);
    db.ok("DROP TABLE QSV1T");
    assert_eq!(db.code("SELECT * FROM QSV1J"), -204);
    assert_eq!(db.rows("SELECT TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST' ORDER BY 1"), [row(&["QSV1O"])]);
}

#[test]
fn check_option_on_a_logical_view() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSV2T (ID INT NOT NULL, NAME VARCHAR(20))");
    db.ok("CREATE VIEW QSV2V AS SELECT ID, NAME FROM QSV2T WHERE ID > 1 WITH CHECK OPTION");
    db.ok("CREATE VIEW QSV2L AS SELECT ID, NAME FROM QSV2T WHERE ID < 5 WITH LOCAL CHECK OPTION");
    db.ok("INSERT INTO QSV2V VALUES (2, 'A')");
    let (ca, _) = db.run("INSERT INTO QSV2V VALUES (0, 'B')");
    assert_eq!((ca.sqlcode, ca.sqlstate.as_str()), (-161, "44000"));
    assert_eq!(db.code("UPDATE QSV2V SET ID = 0"), -161);
    assert_eq!(db.code("INSERT INTO QSV2L VALUES (7, 'C')"), -161);
    assert_eq!(db.rows("SELECT TABLE_NAME, CHECK_OPTION FROM QSYS2.SYSVIEWS ORDER BY 1"), [row(&["QSV2L", "Y"]), row(&["QSV2V", "C"])]);
}

#[test]
fn create_table_as_like_and_replace() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSP1_LONG_TABLE_NAME FOR SYSTEM NAME QSP1T (ID INT NOT NULL, CUSTOMER_NAME FOR COLUMN CUSNAM VARCHAR(20), AMT DECIMAL(7, 2))");
    db.ok("INSERT INTO QSP1T VALUES (1, 'A', 1.5), (2, 'B', 2.5)");
    let (ca, o) = db.run("CREATE TABLE QSP1C AS (SELECT ID, CUSTOMER_NAME FROM QSP1T) WITH DATA");
    assert_eq!((ca.sqlcode, ca.sqlerrd[2]), (7905, 2));
    assert!(matches!(o, Some(Outcome::Count(2))));
    db.ok("CREATE TABLE QSP1D AS (SELECT * FROM QSP1T) WITH NO DATA");
    db.ok("CREATE TABLE QSP1H AS (SELECT * FROM QSP1T) DEFINITION ONLY");
    db.ok("CREATE TABLE QSP1N (A, B) AS (SELECT ID, AMT FROM QSP1T) WITH DATA");
    assert_eq!(db.code("CREATE TABLE QSP1E AS (SELECT ID, AMT + 1 FROM QSP1T) WITH DATA"), -153);
    db.ok("CREATE TABLE QSP1F LIKE QSP1T");
    db.ok("CREATE TABLE QSP1G (LIKE QSP1T)");
    assert_eq!(db.code("CREATE TABLE QSP1I LIKE QSP1T INCLUDING IDENTITY"), -84);
    assert_eq!(
        db.rows("SELECT TABLE_NAME, COLUMN_NAME, SYSTEM_COLUMN_NAME, IS_NULLABLE FROM QSYS2.SYSCOLUMNS WHERE TABLE_NAME IN ('QSP1C', 'QSP1D', 'QSP1G', 'QSP1N') ORDER BY TABLE_NAME, ORDINAL_POSITION"),
        [
            row(&["QSP1C", "ID", "ID", "N"]),
            row(&["QSP1C", "CUSTOMER_NAME", "CUSNAM", "Y"]),
            row(&["QSP1D", "ID", "ID", "N"]),
            row(&["QSP1D", "CUSTOMER_NAME", "CUSNAM", "Y"]),
            row(&["QSP1D", "AMT", "AMT", "Y"]),
            row(&["QSP1G", "ID", "ID", "Y"]),
            row(&["QSP1G", "CUSTOMER_NAME", "CUSNAM", "Y"]),
            row(&["QSP1G", "AMT", "AMT", "Y"]),
            row(&["QSP1N", "A", "A", "N"]),
            row(&["QSP1N", "B", "B", "Y"]),
        ]
    );
    assert_eq!(db.rows("SELECT * FROM QSP1C ORDER BY ID"), [row(&["1", "A"]), row(&["2", "B"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSP1D"), [row(&["0"])]);
    // OR REPLACE keeps the rows by column name, or drops them.
    db.ok("CREATE OR REPLACE TABLE QSP1R (X INT)");
    db.ok("INSERT INTO QSP1R VALUES 1");
    db.ok("CREATE OR REPLACE TABLE QSP1R (X INT, Y INT) ON REPLACE PRESERVE ROWS");
    assert_eq!(db.rows("SELECT * FROM QSP1R"), [row(&["1", "-"])]);
    db.ok("CREATE OR REPLACE TABLE QSP1R (X INT) ON REPLACE DELETE ROWS");
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSP1R"), [row(&["0"])]);
    assert_eq!(db.code("CREATE TABLE QSP1S (X INT) ON REPLACE DELETE ROWS"), -104);
    assert_eq!(db.code("CREATE OR REPLACE INDEX QSP1I ON QSP1T (ID)"), -104);
}

#[test]
fn schemas_long_names_and_journaling() {
    let mut db = Db::new();
    db.ok("CREATE SCHEMA MY_LONG_SCHEMA_NAME");
    assert_eq!(
        db.rows("SELECT SCHEMA_NAME, SYSTEM_SCHEMA_NAME FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME LIKE 'MY%'"),
        [row(&["MY_LONG_SCHEMA_NAME", "MY_LO00001"])]
    );
    assert_eq!(db.code("CREATE SCHEMA MY_LONG_SCHEMA_NAME"), -601);
    let (ca, _) = db.run("CREATE TABLE MY_LONG_SCHEMA_NAME.ORDERS (ID INT)");
    assert_eq!(ca.sqlcode, 0, "journaled to QSQJRN: no warning");
    db.ok("INSERT INTO MY_LONG_SCHEMA_NAME.ORDERS VALUES 1");
    // The schema's own catalog views.
    assert_eq!(db.rows("SELECT TABLE_NAME, TABLE_TYPE FROM MY_LONG_SCHEMA_NAME.SYSTABLES WHERE TABLE_NAME = 'ORDERS'"), [row(&["ORDERS", "T"])]);
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'MY_LONG_SCHEMA_NAME' AND TABLE_TYPE = 'V'"),
        [row(&["10"])]
    );
    db.ctx.current_schema = None;
    db.ok("SET SCHEMA MY_LONG_SCHEMA_NAME");
    assert_eq!(db.rows("SELECT * FROM ORDERS"), [row(&["1"])]);
    db.ok("SET SCHEMA QSTEST");
    assert_eq!(db.code("DROP SCHEMA MY_LONG_SCHEMA_NAME RESTRICT"), -616);
    db.ok("DROP TABLE MY_LONG_SCHEMA_NAME.ORDERS");
    db.ok("DROP SCHEMA MY_LONG_SCHEMA_NAME RESTRICT");
    assert_eq!(db.code("DROP SCHEMA MY_LONG_SCHEMA_NAME"), -204);
    db.ok("CREATE SCHEMA QSS1");
    db.ok("CREATE TABLE QSS1.T (X INT)");
    db.ok("CREATE VIEW QSS1.V AS SELECT X + 1 AS Y FROM QSS1.T");
    db.ok("DROP SCHEMA QSS1 CASCADE");
    assert_eq!(db.code("SELECT * FROM QSS1.T"), -204);
}

#[test]
fn index_extensions() {
    let mut db = Db::new();
    db.ok("CREATE TABLE QSI1T (ID INT NOT NULL, NAME VARCHAR(20), AMT DECIMAL(7, 2))");
    db.ok("INSERT INTO QSI1T VALUES (1, 'A', 1.5), (2, 'B', 2.5), (3, NULL, 3.5), (4, NULL, 4.5)");
    db.ok("CREATE INDEX QSI1S ON QSI1T (ID) WHERE AMT > 2");
    db.ok("CREATE UNIQUE WHERE NOT NULL INDEX QSI1U ON QSI1T (NAME)");
    db.ok("CREATE INDEX QSI1D ON QSI1T (AMT DESC, ID)");
    db.ok("CREATE ENCODED VECTOR INDEX QSI1EV ON QSI1T (NAME) INCLUDE (SUM(AMT))");
    assert_eq!(db.code("CREATE INDEX QSI1E ON QSI1T (UPPER(NAME))"), -84);
    assert_eq!(db.code("CREATE INDEX QSI1J ON QSI1T (ID) INCLUDE (AMT)"), -104);
    assert_eq!(db.code("CREATE INDEX QSI1W ON QSI1T (ID) WHERE AMT > ID"), -84);
    assert_eq!(db.code("INSERT INTO QSI1T VALUES (5, 'A', 0)"), -803);
    db.ok("INSERT INTO QSI1T VALUES (5, NULL, 0)");
    assert_eq!(
        db.rows("SELECT INDEX_NAME, IS_UNIQUE, INDEX_HAS_SEARCH_CONDITION, SEARCH_CONDITION, INCLUDE_EXPRESSION FROM QSYS2.SYSINDEXES ORDER BY 1"),
        [
            row(&["QSI1D", "D", "N", "-", "-"]),
            row(&["QSI1EV", "E", "N", "-", "SUM(AMT)"]),
            row(&["QSI1S", "D", "Y", "AMT > 2", "-"]),
            row(&["QSI1U", "V", "N", "-", "-"]),
        ]
    );
    assert_eq!(
        db.rows("SELECT COLUMN_NAME, ORDINAL_POSITION, ORDERING FROM QSYS2.SYSKEYS WHERE INDEX_NAME = 'QSI1D' ORDER BY 2"),
        [row(&["AMT", "1", "D"]), row(&["ID", "2", "A"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSI1T WHERE AMT > 2"), [row(&["3"])]);
}

/// The users of the grant test, made as `CRTUSRPRF` makes them.
fn users(db: &mut Db, names: &[&str]) {
    let sysval = |_: &str| String::new();
    let mut t = db.store.begin().unwrap();
    let qsys = db.ctx.qsys;
    sec::profile::shipped::install(&mut t, qsys, 40, 3, 1_767_225_600).unwrap();
    let secofr = sec::auth::principal::Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = sec::cmd::Env { qsys, actor: &secofr, now: 1_767_225_600, sysval: &sysval };
    for n in names {
        sec::cmd::usrprf::crtusrprf(&mut t, &env, &sec::cmd::Keywords::new(&[("USRPRF", n)])).unwrap();
    }
    t.commit().unwrap();
}

/// A user's authority bits to a table of QSTEST.
fn bits(db: &mut Db, table: &str, user: &str) -> Option<u32> {
    let mut t = db.store.begin().unwrap();
    let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse("QSTEST").unwrap()).unwrap().id;
    let id = store::obj::library::Library::load(&mut t, lib).unwrap().lookup(&mut t, types::FILE, Name::parse(table).unwrap()).unwrap().unwrap();
    let h = sec::auth::grant::header(&mut t, id).unwrap();
    let b = if user == "*PUBLIC" { Some(h.public_authority.0) } else { sec::auth::grant::private(&mut t, &h, user).unwrap().map(|a| a.0) };
    t.abort();
    b
}

#[test]
fn grant_and_revoke_map_to_object_authorities() {
    use store::obj::header::Authority as A;
    let mut db = Db::new();
    users(&mut db, &["GSEL", "GINS", "GALT", "GALL", "GOPT"]);
    db.ok("CREATE TABLE QSG1T (ID INT NOT NULL, NAME VARCHAR(20))");
    db.ok("CREATE VIEW QSG1V AS SELECT ID FROM QSG1T");
    db.ok("CREATE VIEW QSG1Q AS SELECT ID + 1 AS N FROM QSG1T");
    db.ok("GRANT SELECT ON QSG1T TO GSEL");
    db.ok("GRANT INSERT ON TABLE QSG1T TO GINS");
    db.ok("GRANT ALTER, INDEX ON QSG1T TO GALT");
    db.ok("GRANT ALL PRIVILEGES ON QSG1T TO GALL");
    db.ok("GRANT SELECT ON QSG1T TO GOPT WITH GRANT OPTION");
    db.ok("GRANT SELECT ON QSG1T TO PUBLIC");
    assert_eq!(bits(&mut db, "QSG1T", "GSEL"), Some(A::OBJOPR | A::READ));
    assert_eq!(bits(&mut db, "QSG1T", "GINS"), Some(A::OBJOPR | A::ADD));
    assert_eq!(bits(&mut db, "QSG1T", "GALT"), Some(A::OBJALTER));
    assert_eq!(bits(&mut db, "QSG1T", "GALL"), Some(A::OBJOPR | A::OBJALTER | A::OBJREF | A::READ | A::ADD | A::UPD | A::DLT));
    assert_eq!(bits(&mut db, "QSG1T", "GOPT"), Some(A::OBJOPR | A::OBJMGT | A::READ));
    assert_eq!(bits(&mut db, "QSG1T", "*PUBLIC").map(|b| b & (A::OBJOPR | A::READ)), Some(A::OBJOPR | A::READ));
    assert_eq!(
        db.rows("SELECT GRANTEE, PRIVILEGE_TYPE, IS_GRANTABLE FROM QSYS2.SYSTABAUTH WHERE TABLE_NAME = 'QSG1T' AND GRANTEE IN ('GSEL', 'GOPT', 'GALT') ORDER BY 1, 2"),
        [row(&["GALT", "ALTER", "NO"]), row(&["GALT", "INDEX", "NO"]), row(&["GOPT", "SELECT", "YES"]), row(&["GSEL", "SELECT", "NO"])]
    );
    // REVOKE takes the data authority, *OBJOPR with the last of them; a
    // privilege not held is a warning.
    db.ok("REVOKE INSERT ON QSG1T FROM GALL");
    assert_eq!(bits(&mut db, "QSG1T", "GALL"), Some(A::OBJOPR | A::OBJALTER | A::OBJREF | A::READ | A::UPD | A::DLT));
    db.ok("REVOKE SELECT ON QSG1T FROM GSEL");
    assert_eq!(bits(&mut db, "QSG1T", "GSEL"), None);
    let (ca, _) = db.run("REVOKE SELECT ON QSG1T FROM GINS");
    assert_eq!((ca.sqlcode, ca.sqlstate.as_str()), (569, "01006"));
    assert_eq!(bits(&mut db, "QSG1T", "GINS"), Some(A::OBJOPR | A::ADD));
    db.ok("REVOKE ALL ON QSG1T FROM GALL");
    assert_eq!(bits(&mut db, "QSG1T", "GALL"), None);
    db.ok("REVOKE ALL ON QSG1T FROM PUBLIC");
    assert_eq!(bits(&mut db, "QSG1T", "*PUBLIC").map(|b| b & 0x07FF), Some(0));
    // Views, refusals.
    db.ok("GRANT SELECT, UPDATE ON QSG1V TO GSEL");
    assert_eq!(db.code("GRANT INDEX ON QSG1V TO GSEL"), -557);
    assert_eq!(db.code("GRANT INSERT ON QSG1V TO GSEL"), -7027);
    assert_eq!(db.code("GRANT UPDATE ON QSG1Q TO GSEL"), -7027);
    assert_eq!(db.code("GRANT SELECT ON QSG1T TO NOSUCHUSR"), -204);
    assert_eq!(db.code("GRANT SELECT ON NOSUCHTBL TO GSEL"), -204);
    assert_eq!(db.code("GRANT UPDATE (NAME) ON QSG1T TO GSEL"), -84);
    assert_eq!(db.code("GRANT EXECUTE ON PROCEDURE QSYS.QCMDEXC TO GSEL"), -84);
    assert_eq!(db.code("GRANT USAGE ON SEQUENCE QSTEST.SEQ1 TO GSEL"), -84);
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSYS2.SYSCOLAUTH"), [row(&["0"])]);
}

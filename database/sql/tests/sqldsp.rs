// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays of SQL objects, as the primary shows them
//! (`tools/conformance/oracle/sql-admin/sqlobj-displays.txt`): DSPFD's
//! SQL file type and view statement, DSPDBR's views, a sequence's data
//! area, an SQL procedure's program text.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::ast::ObjectName;
use sql::{Naming, RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn setup() -> (Store, SqlContext) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 3));
    let params = FormatParams { asp_number: 1, asp_uuid: [4; 16], unit_uuids: vec![[5; 16]], unit_serials: Vec::new(), wal_pages: 128, now: NOW };
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
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    t.commit().unwrap();
    let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
    ctx.now = Some(NOW);
    let text = "CREATE TABLE QSTEST.DBM1 (K INT NOT NULL PRIMARY KEY, V VARCHAR(20), N DECIMAL(7, 2));
        INSERT INTO QSTEST.DBM1 VALUES (1, 'one', 1.5), (2, 'two', 2.5);
        CREATE INDEX QSTEST.DBM1V ON QSTEST.DBM1 (V);
        CREATE VIEW QSTEST.DBM1VW AS SELECT V, COUNT(*) AS C FROM QSTEST.DBM1 GROUP BY V;
        CREATE SEQUENCE QSTEST.DBMSEQ START WITH 5 INCREMENT BY 2;
        CREATE PROCEDURE QSTEST.DBMPRC (IN A INT) LANGUAGE SQL BEGIN DECLARE X INT; SET X = A; END;";
    let r = runsqlstm(&mut store, &mut ctx, text, &RunOpts { naming: Naming::Sql, ..RunOpts::default() }).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    (store, ctx)
}

fn file(store: &mut Store, ctx: &SqlContext, name: &str) -> String {
    let mut t = store.begin().unwrap();
    let id = ctx.resolve_file(&mut t, &ObjectName::qualified("QSTEST", name)).unwrap();
    let d = db::dsp::fd::dspfd(&mut t, id).unwrap();
    d.render(db::dsp::fd::FdType::All)
}

#[test]
fn dspfd_shows_sql_file_types_and_the_view_statement() {
    let (mut store, ctx) = setup();
    let t = file(&mut store, &ctx, "DBM1");
    assert!(t.lines().any(|l| l.contains("TXT0320") && l.trim_end().ends_with("TABLE")), "{t}");
    let i = file(&mut store, &ctx, "DBM1V");
    assert!(i.lines().any(|l| l.contains("TXT0320") && l.trim_end().ends_with("INDEX")), "{i}");
    let v = file(&mut store, &ctx, "DBM1VW");
    assert!(v.lines().any(|l| l.contains("Type of file") && l.trim_end().ends_with("LF")), "{v}");
    assert!(v.lines().any(|l| l.contains("TXT0320") && l.trim_end().ends_with("VIEW")), "{v}");
    assert!(v.contains("TXT0321"), "{v}");
    // In lines of 68, as the primary breaks it.
    assert!(v.contains("  CREATE VIEW DBM1VW AS SELECT V, COUNT(*) AS C FROM QSTEST.DBM1 GROUP
   BY V
"), "{v}");
    assert!(v.lines().any(|l| l.contains("TXT0322") && l.trim_end().ends_with("NONE")), "{v}");
}

#[test]
fn dspdbr_lists_views_that_run_their_query() {
    let (mut store, ctx) = setup();
    let mut t = store.begin().unwrap();
    let id = ctx.resolve_file(&mut t, &ObjectName::qualified("QSTEST", "DBM1")).unwrap();
    let r = db::dsp::dbr::dspdbr(&mut t, id, None).unwrap();
    let names: Vec<String> = r.dependents.iter().map(|d| d.file.to_string()).collect();
    assert!(names.contains(&"DBM1V".to_string()) && names.contains(&"DBM1VW".to_string()), "{names:?}");
    let vw = r.dependents.iter().find(|d| d.file.to_string() == "DBM1VW").unwrap();
    assert!(!vw.access_path);
}

#[test]
fn a_sequence_is_a_2000_byte_data_area_and_a_procedure_says_so() {
    let (mut store, ctx) = setup();
    let mut t = store.begin().unwrap();
    let lib = store::obj::library::Library::load(&mut t, ctx.names.curlib.unwrap()).unwrap();
    let seq = lib.lookup(&mut t, types::DTAARA, Name::parse("DBMSEQ").unwrap()).unwrap().unwrap();
    match store::obj::dtaara::read(&mut t, seq).unwrap() {
        store::obj::dtaara::DataAreaValue::Char(s) => {
            assert_eq!(s.len(), 2000);
            assert!(s.starts_with("QSQLSEQ"));
        }
        v => panic!("{v:?}"),
    }
    let pgm = lib.lookup(&mut t, types::PGM, Name::parse("DBMPRC").unwrap()).unwrap().unwrap();
    let (_, h) = ops::read_header(&mut t, pgm).unwrap();
    assert_eq!(ccsid::decode_trimmed(&h.text), "SQL PROCEDURE DBMPRC");
}

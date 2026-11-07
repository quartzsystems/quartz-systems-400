// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A schema with long-named tables, a view that runs its query and an
//! index, saved and restored: the SQL information and the name index
//! come back with the objects.

use std::io::Cursor;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use savfmt::{Compression, MediaHeader, Part, Reader, SaveKind, Scan, Writer};
use sql::{Naming, Outcome, SqlContext, prepare};
use store::obj::ops::NameContext;
use store::obj::{JobHandle, ObjectId, ops};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn system() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 9));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut s = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    t.commit().unwrap();
    (s, qsys)
}

fn sql(s: &mut Store, ctx: &mut SqlContext, text: &str) -> Outcome {
    let p = prepare(text, Naming::Sql).unwrap_or_else(|e| panic!("{text}: {e}"));
    let mut t = s.begin().unwrap();
    let o = p
        .execute(&mut t, ctx, &[])
        .unwrap_or_else(|e| panic!("{text}: {e}"));
    t.commit().unwrap();
    o
}

fn rows(s: &mut Store, ctx: &mut SqlContext, text: &str) -> Vec<Vec<String>> {
    match sql(s, ctx, text) {
        Outcome::Rows(rs) => rs
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|v| v.to_string().trim_end().to_string())
                    .collect()
            })
            .collect(),
        o => panic!("{text}: {o:?}"),
    }
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_schema_round_trips_with_its_sql_names() {
    let (mut s, qsys) = system();
    let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext::default());
    ctx.now = Some(NOW);
    sql(&mut s, &mut ctx, "CREATE SCHEMA SALES_HISTORY_SCHEMA");
    sql(
        &mut s,
        &mut ctx,
        "CREATE TABLE SALES_HISTORY_SCHEMA.CUSTOMER_MASTER (CUSTOMER_NUMBER INT NOT NULL, CUSTOMER_NAME VARCHAR(20))",
    );
    sql(
        &mut s,
        &mut ctx,
        "CREATE TABLE SALES_HISTORY_SCHEMA.ORDERS (ORDER_ID INT, CUSTOMER_NUMBER INT, AMOUNT DECIMAL(9, 2))",
    );
    sql(
        &mut s,
        &mut ctx,
        "INSERT INTO SALES_HISTORY_SCHEMA.CUSTOMER_MASTER VALUES (1, 'Alice'), (2, 'Bob')",
    );
    sql(
        &mut s,
        &mut ctx,
        "INSERT INTO SALES_HISTORY_SCHEMA.ORDERS VALUES (10, 1, 5.00), (11, 1, 7.50), (12, 2, 1.25)",
    );
    sql(
        &mut s,
        &mut ctx,
        "CREATE VIEW SALES_HISTORY_SCHEMA.CUSTOMER_TOTALS AS SELECT C.CUSTOMER_NAME, SUM(O.AMOUNT) AS TOTAL FROM SALES_HISTORY_SCHEMA.CUSTOMER_MASTER C JOIN SALES_HISTORY_SCHEMA.ORDERS O ON C.CUSTOMER_NUMBER = O.CUSTOMER_NUMBER GROUP BY C.CUSTOMER_NAME",
    );
    sql(
        &mut s,
        &mut ctx,
        "CREATE INDEX SALES_HISTORY_SCHEMA.ORDERS_BY_CUSTOMER ON SALES_HISTORY_SCHEMA.ORDERS (CUSTOMER_NUMBER) WHERE AMOUNT > 2",
    );
    let catalog = "SELECT TABLE_NAME, SYSTEM_TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'SALES_HISTORY_SCHEMA' AND TABLE_NAME NOT LIKE 'SYS%' ORDER BY TABLE_NAME";
    let before = rows(&mut s, &mut ctx, catalog);
    assert_eq!(
        before,
        [
            row(&["CUSTOMER_MASTER", "CUSTO00001", "T"]),
            row(&["CUSTOMER_TOTALS", "CUSTO00002", "V"]),
            row(&["ORDERS", "ORDERS", "T"])
        ]
    );

    // SAVLIB of the library the schema is.
    let h = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [7; 16],
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVLIB".into(),
        user: "QSYS".into(),
    };
    let mut w = Writer::new(Vec::new(), &h).unwrap();
    let opts = SaveOptions {
        update_history: false,
        ..SaveOptions::default()
    };
    save_library(
        &mut s,
        JobHandle(1),
        &AllowAll,
        qsys,
        "SALES00001",
        &opts,
        &mut w,
    )
    .unwrap();
    let bytes = w.finish(0).unwrap().1;
    // The library and every SQL object carry an `sqlinf` part.
    let mut r = Reader::new(Cursor::new(&bytes[..])).unwrap();
    let mut with_sql = Vec::new();
    while let Some(sc) = r.next_object().unwrap() {
        if let Scan::Object(o) = sc
            && o.parts.contains_key(&Part::SQLINF)
        {
            with_sql.push(qsname::Name::from_bytes(o.descriptor.name).to_string());
        }
    }
    for n in [
        "SALES00001",
        "CUSTO00001",
        "CUSTO00002",
        "ORDERS",
        "ORDER00001",
    ] {
        assert!(
            with_sql.iter().any(|x| x == n),
            "{n} has no sqlinf part: {with_sql:?}"
        );
    }

    // The schema goes, then comes back from the stream.
    sql(&mut s, &mut ctx, "DROP SCHEMA SALES_HISTORY_SCHEMA");
    let mut r = Reader::new(Cursor::new(&bytes[..])).unwrap();
    let rep = restore(
        &mut s,
        JobHandle(1),
        &AllowAll,
        qsys,
        &mut r,
        &RestoreOptions::default(),
    )
    .unwrap();
    assert!(rep.restored() >= 5, "{rep:?}");
    assert_eq!(rows(&mut s, &mut ctx, catalog), before);
    assert_eq!(
        rows(
            &mut s,
            &mut ctx,
            "SELECT CUSTOMER_NAME, TOTAL FROM SALES_HISTORY_SCHEMA.CUSTOMER_TOTALS ORDER BY CUSTOMER_NAME"
        ),
        [row(&["Alice", "12.50"]), row(&["Bob", "1.25"])]
    );
    assert_eq!(
        rows(
            &mut s,
            &mut ctx,
            "SELECT INDEX_NAME, SEARCH_CONDITION FROM QSYS2.SYSINDEXES WHERE INDEX_SCHEMA = 'SALES_HISTORY_SCHEMA'"
        ),
        [row(&["ORDERS_BY_CUSTOMER", "AMOUNT > 2"])]
    );
    assert_eq!(
        rows(
            &mut s,
            &mut ctx,
            "SELECT COUNT(*) FROM SALES_HISTORY_SCHEMA.CUSTOMER_MASTER"
        ),
        [row(&["2"])]
    );
}

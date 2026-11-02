// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 4's `Sql` service against the test system's database: every
//! operation and status, the lanes, cancels before and after a start,
//! every type, commitment control and the rollback at close, replies
//! over a frame, `max_rows` and the catalog's pages.

#![allow(clippy::many_single_char_names)] // short names in the checks

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::sql::{
    Columns, CommitLevel, CursorRef, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables, Naming,
    Prepare, Prepared, Schemas, SetOptions, SqlFailure, SqlValue, StatementRef, TableKind, Tables,
};
use qap_proto::{Envelope, Kind, Op, Status};

use common::{Controls, Run, cfg, start_with};

/// Version 4, signed on as `user`.
fn signed_on(c: &Arc<Controls>, user: &str, password: &str) -> Run {
    let mut r = start_with(cfg(), Arc::clone(c));
    let h: HelloReply = r.client.hello_range(1, 4).body_as().unwrap();
    assert_eq!(h.version, 4);
    r.client.sign_on(user, password);
    r
}

fn bob() -> (Arc<Controls>, Run) {
    let c = Controls::new();
    let r = signed_on(&c, "BOB", "SECRET1");
    (c, r)
}

fn prepare(r: &mut Run, sql: &str) -> Envelope {
    r.client.call(Op::Prepare, &Prepare { sql: sql.into() })
}

fn prepared(r: &mut Run, sql: &str) -> Prepared {
    let e = prepare(r, sql);
    assert_eq!(e.status, Status::Ok, "{sql}: {e:?}");
    e.body_as().unwrap()
}

fn execute(r: &mut Run, statement: u32, rows: Vec<Vec<SqlValue>>) -> Envelope {
    r.client.call(Op::Execute, &Execute { statement, rows })
}

fn executed(r: &mut Run, statement: u32, rows: Vec<Vec<SqlValue>>) -> Executed {
    let e = execute(r, statement, rows);
    assert_eq!(e.status, Status::Ok, "{e:?}");
    e.body_as().unwrap()
}

fn fetch(r: &mut Run, cursor: u32, max_rows: u32) -> Envelope {
    r.client.call(Op::Fetch, &Fetch { cursor, max_rows })
}

fn fetched(r: &mut Run, cursor: u32, max_rows: u32) -> Fetched {
    let e = fetch(r, cursor, max_rows);
    assert_eq!(e.status, Status::Ok, "{e:?}");
    e.body_as().unwrap()
}

/// Every row of a query.
fn query(r: &mut Run, sql: &str, params: Vec<SqlValue>) -> Vec<Vec<SqlValue>> {
    let p = prepared(r, sql);
    let rows = if params.is_empty() { Vec::new() } else { vec![params] };
    let cursor = executed(r, p.statement, rows).cursor.expect("a cursor");
    let f = fetched(r, cursor, 0);
    assert!(f.end);
    assert_eq!(r.client.call(Op::CloseCursor, &CursorRef { cursor }).status, Status::Ok);
    f.rows
}

fn options(r: &mut Run, commit: CommitLevel, naming: Naming, schema: Option<&str>) -> Envelope {
    r.client.call(
        Op::SetOptions,
        &SetOptions {
            commit,
            naming,
            default_schema: schema.map(Into::into),
        },
    )
}

fn failure(e: &Envelope) -> (SqlFailure, String) {
    assert_eq!(e.status, Status::SqlError, "{e:?}");
    (e.body_as().unwrap(), e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default())
}

fn int(v: i64) -> SqlValue {
    SqlValue::Integer { value: v }
}

fn text(s: &str) -> SqlValue {
    SqlValue::Text { text: s.into() }
}

fn msg_id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

#[test]
fn prepare_execute_fetch_close_release() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT * FROM HLINV.ASSETS");
    assert_eq!(p.columns.len(), 12);
    assert!(p.parameters.is_empty());
    assert_eq!((p.columns[0].name.as_str(), p.columns[0].sql_type.as_str(), p.columns[0].length), ("ASSET_TAG", "CHAR", 10));
    assert_eq!((p.columns[8].sql_type.as_str(), p.columns[8].length, p.columns[8].scale), ("DECIMAL", 11, 2));
    let e = executed(&mut r, p.statement, Vec::new());
    assert_eq!(e.updated, 0);
    let cursor = e.cursor.unwrap();
    // max_rows: 5, then the other 7 with the end.
    let a = fetched(&mut r, cursor, 5);
    assert_eq!((a.rows.len(), a.end), (5, false));
    assert_eq!(a.rows[0][0], text("QS000101  "));
    let b = fetched(&mut r, cursor, 0);
    assert_eq!((b.rows.len(), b.end), (7, true));
    assert_eq!(b.rows[6][6], text("SPARE   "));
    let c = fetched(&mut r, cursor, 3);
    assert_eq!((c.rows.len(), c.end), (0, true));
    // Close, release, and NotFound after.
    assert_eq!(r.client.call(Op::CloseCursor, &CursorRef { cursor }).status, Status::Ok);
    let e = r.client.call(Op::CloseCursor, &CursorRef { cursor });
    assert_eq!((e.status, msg_id(&e)), (Status::NotFound, "SQL0501".into()));
    assert_eq!(fetch(&mut r, cursor, 1).status, Status::NotFound);
    let s = StatementRef { statement: p.statement };
    assert_eq!(r.client.call(Op::Release, &s).status, Status::Ok);
    let e = r.client.call(Op::Release, &s);
    assert_eq!((e.status, msg_id(&e)), (Status::NotFound, "SQL0518".into()));
    let e = execute(&mut r, p.statement, Vec::new());
    assert_eq!((e.status, msg_id(&e)), (Status::NotFound, "SQL0518".into()));
    // Commit and roll back with nothing to do.
    assert_eq!(r.client.call(Op::Commit, &()).status, Status::Ok);
    assert_eq!(r.client.call(Op::Rollback, &()).status, Status::Ok);
}

#[test]
fn parameters_where_and_the_view() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT ASSET_TAG, COST FROM HLINV.ASSETS WHERE LOCATION = ? AND STATUS = 'ACTIVE'");
    assert_eq!(p.columns.len(), 2);
    assert_eq!(p.parameters.len(), 1);
    assert_eq!((p.parameters[0].sql_type.as_str(), p.parameters[0].nullable), ("CHAR", true));
    let cursor = executed(&mut r, p.statement, vec![vec![text("QS-BR1-R1")]]).cursor.unwrap();
    let f = fetched(&mut r, cursor, 0);
    assert_eq!(f.rows.len(), 2);
    assert_eq!(f.rows[1][1], SqlValue::Decimal { text: "2700.00".into() });
    assert_eq!(query(&mut r, "SELECT * FROM HLINV.ASSETS_ACTIVE", Vec::new()).len(), 11);
    assert_eq!(query(&mut r, "SELECT * FROM HLINV.HW", Vec::new()).len(), 12);
    // Too few parameters.
    let e = execute(&mut r, p.statement, vec![Vec::new()]);
    assert_eq!(failure(&e).0.sqlstate, "07001");
}

#[test]
fn sql_errors_have_their_states_and_messages() {
    let (_c, mut r) = bob();
    let (f, id) = failure(&prepare(&mut r, "SELEKT * FROM HLINV.ASSETS"));
    assert_eq!((f.sqlstate.as_str(), f.sqlcode, f.row, id.as_str()), ("42601", -104, None, "SQL0104"));
    let (f, id) = failure(&prepare(&mut r, "SELECT * FROM HLINV.NOSUCH"));
    assert_eq!((f.sqlstate.as_str(), f.sqlcode, id.as_str()), ("42704", -204, "SQL0204"));
    let (f, _) = failure(&prepare(&mut r, "SELECT NOSUCH FROM HLINV.ASSETS"));
    assert_eq!(f.sqlstate, "42703");
    // A statement the database does not run: its own syntax error.
    let (f, _) = failure(&prepare(&mut r, "CREATE TABLE HLTEST.X (A INT)"));
    assert_eq!(f.sqlcode, -104);
    // Changing a view.
    let p = prepared(&mut r, "DELETE FROM HLINV.ASSETS_ACTIVE");
    assert_eq!(failure(&execute(&mut r, p.statement, Vec::new())).0.sqlstate, "42807");
}

#[test]
fn a_version_3_peer_has_no_sql() {
    let c = Controls::new();
    let mut r = start_with(cfg(), Arc::clone(&c));
    let h: HelloReply = r.client.hello_range(1, 3).body_as().unwrap();
    assert_eq!(h.version, 3);
    assert!(!h.services.contains(&qap_proto::Service::Sql));
    r.client.sign_on("BOB", "SECRET1");
    for op in [Op::Prepare, Op::ListSchemas, Op::Commit] {
        assert_eq!(r.client.call(op, &()).status, Status::InvalidRequest, "{op:?}");
    }
    // SqlError, which version 3 does not define, would go as Error.
    assert_eq!(Status::SqlError.for_version(3), Status::Error);
}

#[test]
fn not_authorized_to_a_table() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1");
    let e = prepare(&mut r, "SELECT * FROM HLPAY.SALARIES");
    assert_eq!((e.status, msg_id(&e)), (Status::NotAuthorized, "SQL0551".into()));
    let e = r.client.call(
        Op::ListColumns,
        &ListColumns {
            schema: "HLPAY".into(),
            table: "SALARIES".into(),
        },
    );
    assert_eq!(e.status, Status::NotAuthorized);
    // Listed, though.
    let t: Tables = r.client.call(Op::ListTables, &tables("HLPAY", "%", 0, 0)).body_as().unwrap();
    assert_eq!(t.tables[0].name, "SALARIES");
    let mut ann = signed_on(&c, "ANN", "ANNPW01");
    assert_eq!(query(&mut ann, "SELECT * FROM HLPAY.SALARIES", Vec::new()).len(), 2);
}

#[test]
fn naming_and_the_default_schema() {
    let (_c, mut r) = bob();
    let (f, _) = failure(&prepare(&mut r, "SELECT * FROM HLINV/ASSETS"));
    assert_eq!(f.sqlstate, "42833");
    // Unqualified under SQL naming: the profile's schema, BOB.
    let (f, _) = failure(&prepare(&mut r, "SELECT * FROM ASSETS"));
    assert_eq!(f.sqlstate, "42704");
    assert_eq!(options(&mut r, CommitLevel::None, Naming::System, None).status, Status::Ok);
    assert_eq!(query(&mut r, "SELECT * FROM HLINV/ASSETS", Vec::new()).len(), 12);
    assert_eq!(query(&mut r, "SELECT * FROM ASSETS", Vec::new()).len(), 12);
    assert_eq!(options(&mut r, CommitLevel::None, Naming::Sql, Some("HLTEST")).status, Status::Ok);
    assert_eq!(query(&mut r, "SELECT * FROM JRN", Vec::new()).len(), 0);
}

#[test]
fn commitment_control_and_in_use() {
    let (_c, mut r) = bob();
    assert_eq!(options(&mut r, CommitLevel::ReadCommitted, Naming::Sql, None).status, Status::Ok);
    let ins = prepared(&mut r, "INSERT INTO HLTEST.JRN VALUES (?, ?)");
    assert_eq!(ins.parameters.len(), 2);
    assert_eq!(executed(&mut r, ins.statement, vec![vec![int(1), text("one")]]).updated, 1);
    // A transaction is open.
    let e = options(&mut r, CommitLevel::None, Naming::Sql, None);
    assert_eq!((e.status, msg_id(&e)), (Status::InUse, "SQL0752".into()));
    assert_eq!(r.client.call(Op::Rollback, &()).status, Status::Ok);
    assert!(query(&mut r, "SELECT * FROM HLTEST.JRN", Vec::new()).is_empty());
    assert_eq!(executed(&mut r, ins.statement, vec![vec![int(2), text("two")]]).updated, 1);
    assert_eq!(r.client.call(Op::Commit, &()).status, Status::Ok);
    assert_eq!(r.client.call(Op::Rollback, &()).status, Status::Ok);
    assert_eq!(query(&mut r, "SELECT * FROM HLTEST.JRN", Vec::new()), [vec![int(2), text("two")]]);
    assert_eq!(options(&mut r, CommitLevel::None, Naming::Sql, None).status, Status::Ok);
    // Updates and deletes count their rows.
    let up = prepared(&mut r, "UPDATE HLTEST.JRN SET NAME = ? WHERE ID = ?");
    assert_eq!(executed(&mut r, up.statement, vec![vec![text("deux"), int(2)]]).updated, 1);
    assert_eq!(executed(&mut r, up.statement, vec![vec![text("x"), int(9)]]).updated, 0);
    let del = prepared(&mut r, "DELETE FROM HLTEST.JRN");
    assert_eq!(executed(&mut r, del.statement, Vec::new()).updated, 1);
}

#[test]
fn an_unjournaled_table_under_commitment_control() {
    let (_c, mut r) = bob();
    assert_eq!(options(&mut r, CommitLevel::Serializable, Naming::Sql, None).status, Status::Ok);
    let ins = prepared(&mut r, "INSERT INTO HLTEST.NOJRN (ID, NAME) VALUES (?, 'x')");
    let (f, id) = failure(&execute(&mut r, ins.statement, vec![vec![int(1)]]));
    assert_eq!((f.sqlstate.as_str(), f.sqlcode, f.row, id.as_str()), ("55019", -7008, Some(0), "SQL7008"));
    // Without commitment control it goes in.
    assert_eq!(options(&mut r, CommitLevel::None, Naming::Sql, None).status, Status::Ok);
    assert_eq!(executed(&mut r, ins.statement, vec![vec![int(1)]]).updated, 1);
}

#[test]
fn a_multi_row_execute_stops_at_the_failing_row() {
    let (_c, mut r) = bob();
    let ins = prepared(&mut r, "INSERT INTO HLTEST.JRN VALUES (?, ?)");
    let rows = vec![vec![int(1), text("a")], vec![int(2), text("b")], vec![int(1), text("c")], vec![int(3), text("d")]];
    let (f, id) = failure(&execute(&mut r, ins.statement, rows));
    assert_eq!((f.sqlstate.as_str(), f.sqlcode, f.row, id.as_str()), ("23505", -803, Some(2), "SQL0803"));
    let got = query(&mut r, "SELECT ID FROM HLTEST.JRN", Vec::new());
    assert_eq!(got, [vec![int(1)], vec![int(2)]]);
    // A value of the wrong type is its row's failure too.
    let (f, _) = failure(&execute(&mut r, ins.statement, vec![vec![int(5), text("e")], vec![text("six"), text("f")]]));
    assert_eq!((f.sqlstate.as_str(), f.row), ("07006", Some(1)));
}

#[test]
fn uncommitted_changes_roll_back_when_the_connection_closes() {
    let c = Controls::new();
    let mut a = signed_on(&c, "BOB", "SECRET1");
    assert_eq!(options(&mut a, CommitLevel::ReadUncommitted, Naming::Sql, None).status, Status::Ok);
    let ins = prepared(&mut a, "INSERT INTO HLINV.ASSETS (ASSET_TAG, LOCATION, MANUFACTURER, MODEL, ASSET_TYPE, STATUS, CHANGED_TS) VALUES (?, 'QS-HQ-R1', 'DELL', 'R750', 'SERVER', 'ACTIVE', '2026-11-02-10.00.00')");
    assert_eq!(executed(&mut a, ins.statement, vec![vec![text("QS000999")]]).updated, 1);
    drop(a.client);
    assert_eq!(a.server.join().unwrap(), qapd::Closed::Peer);
    let mut b = signed_on(&c, "BOB", "SECRET1");
    assert_eq!(query(&mut b, "SELECT * FROM HLINV.ASSETS", Vec::new()).len(), 12);
}

#[test]
fn every_type_round_trips() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT * FROM HLTEST.ALLTYPES");
    let col = |n: &str| p.columns.iter().find(|c| c.name == n).unwrap().clone();
    assert_eq!((col("C_BITS").sql_type.as_str(), col("C_BITS").ccsid), ("CHAR", 65535));
    assert_eq!(col("C_VARCHAR").ccsid, 37);
    assert_eq!((col("C_TS12").sql_type.as_str(), col("C_TS12").scale), ("TIMESTAMP", 12));
    assert_eq!(col("C_TS").scale, 6);
    assert_eq!(col("C_DECFLOAT").sql_type, "DECFLOAT");
    let rows = query(&mut r, "SELECT * FROM HLTEST.ALLTYPES", Vec::new());
    assert_eq!(rows.len(), 7);
    let at = |name: &str| p.columns.iter().position(|c| c.name == name).unwrap();
    let decfloats: Vec<SqlValue> = rows.iter().map(|r| r[at("C_DECFLOAT")].clone()).collect();
    let d = |t: &str| SqlValue::Decimal { text: t.into() };
    let want = [d("1234.5678"), d("1.5E-30"), d("Infinity"), d("-Infinity"), d("NaN"), d("sNaN"), SqlValue::Null];
    assert_eq!(decfloats, want);
    assert!(matches!(&rows[0][at("C_BITS")], SqlValue::Binary { data } if data.len() == 8 && data[0] == 0));
    assert_eq!(rows[0][at("C_TS12")], SqlValue::Timestamp { ms: 1_793_610_843_123, picos: 456_789_012 });
    assert_eq!(rows[0][at("C_TS")], SqlValue::Timestamp { ms: 1_793_610_843_123, picos: 456_000_000 });
    assert_eq!(rows[1][at("C_DOUBLE")], SqlValue::Float { text: "1e300".into() });
    assert_eq!(rows[0][at("C_BOOLEAN")], SqlValue::Boolean { value: true });
    assert!(rows[6][1..].iter().all(|v| *v == SqlValue::Null));
    let s = query(&mut r, "SELECT C_DECFLOAT FROM HLTEST.ALLTYPES WHERE ID = 6", Vec::new());
    assert_eq!(s, [vec![d("sNaN")]]);
    // Each row back in under another key reads back the same.
    let ins = prepared(&mut r, &format!("INSERT INTO HLTEST.ALLTYPES VALUES ({})", vec!["?"; p.columns.len()].join(", ")));
    assert_eq!(ins.parameters.len(), p.columns.len());
    assert_eq!(ins.parameters[at("C_BITS")].ccsid, 65535);
    let again: Vec<Vec<SqlValue>> = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            if let SqlValue::Integer { value } = &mut row[0] {
                *value += 100;
            }
            row
        })
        .collect();
    assert_eq!(executed(&mut r, ins.statement, again.clone()).updated, 7);
    let back = query(&mut r, "SELECT * FROM HLTEST.ALLTYPES", Vec::new());
    assert_eq!(back[7..], again[..]);
    // A parameter's sNaN and picoseconds go in as they are; a 65535
    // column takes only bytes.
    let one = prepared(&mut r, "INSERT INTO HLTEST.ALLTYPES (ID, C_DECFLOAT, C_TS12) VALUES (?, ?, ?)");
    let ts = SqlValue::Timestamp { ms: 1_000, picos: 1 };
    executed(&mut r, one.statement, vec![vec![int(50), d("sNaN"), ts.clone()]]);
    let got = query(&mut r, "SELECT C_DECFLOAT, C_TS12 FROM HLTEST.ALLTYPES WHERE ID = ?", vec![int(50)]);
    assert_eq!(got, [vec![d("sNaN"), ts]]);
    let bits = prepared(&mut r, "INSERT INTO HLTEST.ALLTYPES (ID, C_BITS) VALUES (?, ?)");
    let (f, _) = failure(&execute(&mut r, bits.statement, vec![vec![int(51), text("ABC")]]));
    assert_eq!(f.sqlstate, "07006");
    executed(&mut r, bits.statement, vec![vec![int(51), SqlValue::Binary { data: vec![0xC1] }]]);
    let got = query(&mut r, "SELECT C_BITS FROM HLTEST.ALLTYPES WHERE ID = 51", Vec::new());
    assert_eq!(got, [vec![SqlValue::Binary { data: vec![0xC1, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40] }]]);
}

#[test]
fn a_fetch_over_a_megabyte_is_split() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT * FROM HLTEST.BIG");
    let cursor = executed(&mut r, p.statement, Vec::new()).cursor.unwrap();
    let id = r.client.request(Op::Fetch, &Fetch { cursor, max_rows: 0 });
    let mut frames = Vec::new();
    loop {
        let f = r.client.recv_frame().unwrap();
        assert_eq!((f.kind, f.request_id, f.status), (Kind::Reply, id, Status::Ok));
        let more = f.more();
        frames.push(f);
        if !more {
            break;
        }
    }
    assert!(frames.len() >= 3, "{} frames", frames.len());
    let body: Vec<u8> = frames.iter().flat_map(|f| f.body.clone()).collect();
    let all: Fetched = ipc::decode(&body).unwrap();
    assert_eq!((all.rows.len(), all.end), (3000, true));
}

#[test]
fn a_started_execute_cancelled_keeps_its_count() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "INSERT INTO HLTEST.SLOWLOG VALUES (?, ?)");
    let rows: Vec<Vec<SqlValue>> = (0..200).map(|i| vec![int(i), text("x")]).collect();
    let id = r.client.request(Op::Execute, &Execute { statement: p.statement, rows });
    std::thread::sleep(Duration::from_millis(300));
    r.client.send(&Envelope::cancel(id));
    let e = r.client.reply(id);
    assert_eq!((e.status, msg_id(&e)), (Status::Cancelled, "SQL0952".into()));
    let ex: Executed = e.body_as().unwrap();
    assert!(ex.updated > 0 && ex.updated < 200, "{}", ex.updated);
    assert_eq!(ex.cursor, None);
    let n = query(&mut r, "SELECT ID FROM HLTEST.SLOWLOG", Vec::new()).len();
    assert_eq!(i64::try_from(n).unwrap(), ex.updated);
}

#[test]
fn a_started_fetch_cancelled_returns_its_rows_and_closes() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT * FROM HLTEST.SLOW");
    let cursor = executed(&mut r, p.statement, Vec::new()).cursor.unwrap();
    let id = r.client.request(Op::Fetch, &Fetch { cursor, max_rows: 0 });
    std::thread::sleep(Duration::from_millis(300));
    r.client.send(&Envelope::cancel(id));
    let e = r.client.reply(id);
    assert_eq!((e.status, msg_id(&e)), (Status::Cancelled, "SQL0952".into()));
    let f: Fetched = e.body_as().unwrap();
    assert!(!f.end);
    assert!(!f.rows.is_empty() && f.rows.len() < 300, "{}", f.rows.len());
    assert_eq!(f.rows[0], [int(1), text("Row 1")]);
    assert_eq!(fetch(&mut r, cursor, 1).status, Status::NotFound);
}

#[test]
fn a_statement_cancelled_before_it_starts_has_no_body() {
    let (_c, mut r) = bob();
    let p = prepared(&mut r, "SELECT * FROM HLTEST.SLOW");
    let cursor = executed(&mut r, p.statement, Vec::new()).cursor.unwrap();
    let long = r.client.request(Op::Fetch, &Fetch { cursor, max_rows: 50 });
    // Behind the fetch in the lane.
    let queued = r.client.request(Op::Prepare, &Prepare { sql: "SELECT * FROM HLINV.ASSETS".into() });
    r.client.send(&Envelope::cancel(queued));
    let got = r.client.replies(&[long, queued]);
    assert_eq!(got[&long].status, Status::Ok);
    let q = &got[&queued];
    assert_eq!(q.status, Status::Cancelled);
    assert!(q.body.is_empty() && q.message.is_none());
}

#[test]
fn a_long_query_holds_up_no_other_connection_and_no_catalog_list() {
    let c = Controls::new();
    let mut a = signed_on(&c, "BOB", "SECRET1");
    let mut b = signed_on(&c, "BOB", "SECRET1");
    let p = prepared(&mut a, "SELECT * FROM HLTEST.SLOW");
    let cursor = executed(&mut a, p.statement, Vec::new()).cursor.unwrap();
    let at = Instant::now();
    let long = a.client.request(Op::Fetch, &Fetch { cursor, max_rows: 0 });
    std::thread::sleep(Duration::from_millis(50));
    // Another connection's command, and this connection's catalog.
    let e = b.client.call(Op::RunCommand, &CommandText { command: "CRTLIB LIB(BESIDE)".into() });
    assert_eq!(e.status, Status::Ok);
    let t = a.client.call(Op::ListTables, &tables("HLINV", "%", 0, 0));
    assert_eq!(t.status, Status::Ok);
    let s = a.client.call(Op::ListSchemas, &ListSchemas { pattern: "HL%".into(), start: 0, max: 0 });
    assert_eq!(s.status, Status::Ok);
    assert!(at.elapsed() < Duration::from_secs(2), "answered while the fetch ran");
    let f: Fetched = a.client.reply(long).body_as().unwrap();
    assert_eq!(f.rows.len(), 300);
    assert!(at.elapsed() >= Duration::from_secs(3));
}

fn tables(schema: &str, pattern: &str, start: u32, max: u32) -> ListTables {
    ListTables {
        schema: schema.into(),
        pattern: pattern.into(),
        kinds: Vec::new(),
        start,
        max,
    }
}

#[test]
fn catalog_lists_page() {
    let (_c, mut r) = bob();
    let page = |r: &mut Run, pattern: &str, start, max| -> Schemas {
        let e = r.client.call(Op::ListSchemas, &ListSchemas { pattern: pattern.into(), start, max });
        assert_eq!(e.status, Status::Ok);
        e.body_as().unwrap()
    };
    let first = page(&mut r, "%", 0, 50);
    assert_eq!((first.total, first.schemas.len()), (146, 50));
    assert_eq!(first.schemas[0].name, "HLINV");
    let last = page(&mut r, "%", 140, 50);
    assert_eq!((last.total, last.schemas.len()), (146, 6));
    assert_eq!(page(&mut r, "ARCH1%", 0, 0).total, 41);
    assert_eq!(page(&mut r, "NOSUCH%", 0, 0).total, 0);
    let t: Tables = r.client.call(Op::ListTables, &tables("HLMANY", "%", 100, 30)).body_as().unwrap();
    assert_eq!((t.total, t.tables.len(), t.tables[0].name.as_str()), (150, 30, "T101"));
    let views = ListTables {
        kinds: vec![TableKind::View, TableKind::Alias],
        ..tables("HLINV", "%", 0, 0)
    };
    let v: Tables = r.client.call(Op::ListTables, &views).body_as().unwrap();
    let names: Vec<(&str, TableKind)> = v.tables.iter().map(|t| (t.name.as_str(), t.kind)).collect();
    assert_eq!(names, [("ASSETS_ACTIVE", TableKind::View), ("HW", TableKind::Alias)]);
    let e = r.client.call(Op::ListTables, &tables("NOSUCH", "%", 0, 0));
    assert_eq!(e.status, Status::NotFound);
    let cols: Columns = r
        .client
        .call(
            Op::ListColumns,
            &ListColumns {
                schema: "HLINV".into(),
                table: "ASSETS".into(),
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(cols.columns.len(), 12);
    assert_eq!((cols.columns[11].name.as_str(), cols.columns[11].label.as_str()), ("CHANGED_TS", "Last change"));
    let e = r.client.call(
        Op::ListColumns,
        &ListColumns {
            schema: "HLINV".into(),
            table: "NOSUCH".into(),
        },
    );
    assert_eq!(e.status, Status::NotFound);
}

#[test]
fn bodies_that_do_not_decode() {
    let (_c, mut r) = bob();
    for op in [Op::SetOptions, Op::Prepare, Op::Execute, Op::Fetch, Op::ListTables] {
        assert_eq!(r.client.call(op, &()).status, Status::InvalidRequest, "{op:?}");
    }
}

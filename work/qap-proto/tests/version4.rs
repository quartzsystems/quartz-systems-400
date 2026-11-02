// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 4: the `Sql` bodies and values, `Attach`, and what each
//! version defines.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::connection::choose_version;
use qap_proto::ops::signon::Attach;
use qap_proto::ops::sql::{
    Column, Columns, CommitLevel, CursorRef, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables,
    MAX_PICOS, Naming, Prepare, Prepared, Schema, Schemas, SetOptions, SqlFailure, SqlValue, StatementRef, Table,
    TableKind, Tables,
};
use qap_proto::{Op, Service, Status, Token, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn column() -> Column {
    Column {
        name: "PURCHASED".into(),
        label: "Purchased".into(),
        sql_type: "TIMESTAMP".into(),
        length: 32,
        scale: 12,
        nullable: true,
        ccsid: 0,
    }
}

fn values() -> Vec<SqlValue> {
    vec![
        SqlValue::Null,
        SqlValue::Integer { value: -42 },
        SqlValue::Decimal { text: "12345.67".into() },
        SqlValue::Decimal { text: "1.5E-30".into() },
        SqlValue::Decimal { text: "-Infinity".into() },
        SqlValue::Decimal { text: "sNaN".into() },
        SqlValue::Float { text: "0.1".into() },
        SqlValue::Text { text: "Grüße".into() },
        SqlValue::Binary { data: vec![0xC1, 0x00, 0xFF] },
        SqlValue::Date { ms: 1_793_404_800_000 },
        SqlValue::Time { ms: 45_296_000 },
        SqlValue::Timestamp {
            ms: 1_793_404_800_123,
            picos: MAX_PICOS,
        },
        SqlValue::Boolean { value: true },
    ]
}

#[test]
fn sql_bodies() {
    for c in [
        CommitLevel::None,
        CommitLevel::ReadUncommitted,
        CommitLevel::ReadCommitted,
        CommitLevel::RepeatableRead,
        CommitLevel::Serializable,
    ]
    .iter()
    .enumerate()
    {
        same(c.1);
        assert_eq!(encode(c.1), [u8::try_from(c.0).unwrap()]);
    }
    assert_eq!((encode(&Naming::Sql), encode(&Naming::System)), (vec![0], vec![1]));
    for (i, k) in TableKind::ALL.iter().enumerate() {
        same(k);
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<TableKind>(&[5]).is_err());
    for (i, v) in values().iter().enumerate() {
        same(v);
        let tag = [0, 1, 2, 2, 2, 2, 3, 4, 5, 6, 7, 8, 9][i];
        assert_eq!(encode(v)[0], tag, "{v:?}");
    }
    assert!(decode::<SqlValue>(&[10]).is_err());
    same(&SetOptions {
        commit: CommitLevel::ReadCommitted,
        naming: Naming::System,
        default_schema: Some("HLINV".into()),
    });
    same(&Prepare {
        sql: "SELECT * FROM HLINV.ASSETS WHERE ID = ?".into(),
    });
    same(&Prepared {
        statement: 3,
        columns: vec![column()],
        parameters: vec![column()],
    });
    same(&Execute {
        statement: 3,
        rows: vec![values(), vec![SqlValue::Null]],
    });
    same(&Executed {
        updated: 12,
        cursor: Some(1),
    });
    same(&Fetch { cursor: 1, max_rows: 100 });
    same(&Fetched {
        rows: vec![values()],
        end: true,
    });
    same(&CursorRef { cursor: 1 });
    same(&StatementRef { statement: 3 });
    same(&SqlFailure {
        sqlstate: "23505".into(),
        sqlcode: -803,
        row: Some(4),
    });
    same(&ListSchemas {
        pattern: "HL%".into(),
        start: 0,
        max: 50,
    });
    same(&Schemas {
        total: 1,
        schemas: vec![Schema {
            name: "HLINV".into(),
            text: "Inventory".into(),
        }],
    });
    same(&ListTables {
        schema: "HLINV".into(),
        pattern: "%".into(),
        kinds: vec![TableKind::Table, TableKind::View],
        start: 10,
        max: 0,
    });
    same(&Tables {
        total: 1,
        tables: vec![Table {
            schema: "HLINV".into(),
            name: "ASSETS".into(),
            kind: TableKind::Table,
            text: "Assets".into(),
        }],
    });
    same(&ListColumns {
        schema: "HLINV".into(),
        table: "ASSETS".into(),
    });
    same(&Columns { columns: vec![column()] });
    same(&Attach { token: Token::default() });
}

#[test]
fn timestamp_layout() {
    // Tag 8, ms as i64, then picos as u32, little-endian.
    let v = SqlValue::Timestamp { ms: 1, picos: 2 };
    assert_eq!(encode(&v), [8, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0]);
}

#[test]
fn flags_are_0_or_1() {
    let mut b = encode(&Fetched { rows: vec![], end: true });
    *b.last_mut().unwrap() = 2;
    assert!(decode::<Fetched>(&b).is_err());
}

#[test]
fn version_4_values() {
    assert_eq!(VERSION, 4);
    assert_eq!(encode(&Service::Sql), [5]);
    assert_eq!(Service::Sql.since(), 4);
    for code in 1..=11 {
        let op = Op::of(Service::Sql, code).unwrap();
        assert_eq!(op.since(), 4, "{op:?}");
        assert!(!op.before_signon());
        assert_eq!(op.in_lane(), code <= 8, "{op:?}");
    }
    assert_eq!(Op::of(Service::Sql, 12), None);
    assert_eq!(Op::of(Service::SignOn, 7), Some(Op::Attach));
    assert_eq!(Op::Attach.since(), 4);
    assert!(Op::Attach.before_signon());
    assert!(!Op::Attach.in_lane());
    for op in [Op::RunCommand, Op::CheckCommand, Op::DescribeCommand] {
        assert!(op.in_lane());
    }
    for op in Op::ALL.into_iter().filter(|o| o.code().0 == Service::Spool) {
        assert!(!op.in_lane());
    }
    for v in 1..=3 {
        assert_eq!(Status::SqlError.for_version(v), Status::Error);
        assert!(!Service::of_version(v).contains(&Service::Sql));
    }
    assert_eq!(Status::SqlError.for_version(4), Status::SqlError);
    assert_eq!(Status::InUse.for_version(4), Status::InUse);
}

#[test]
fn negotiation_of_1_to_4() {
    for max in 1..=4 {
        assert_eq!(choose_version(1, max, VERSION), Some(max));
    }
    assert_eq!(choose_version(1, 9, VERSION), Some(4));
    assert_eq!(choose_version(4, 4, VERSION), Some(4));
    assert_eq!(choose_version(5, 9, VERSION), None);
}

#[test]
fn job_info() {
    use qap_proto::ops::connection::JobInfoReply;
    use qap_proto::types::JobId;
    assert_eq!(Op::of(Service::Connection, 3), Some(Op::JobInfo));
    assert_eq!(Op::JobInfo.since(), 4);
    assert!(Op::JobInfo.before_signon());
    assert!(!Op::JobInfo.in_lane());
    same(&JobInfoReply {
        job: JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: "000123".into(),
        },
        user: "BOB".into(),
    });
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `CALL`'s output values over DRDA on the primary oracle (V7R1M0):
//! a procedure made in `QSTEST` with `IN`, `OUT` and `INOUT` parameters,
//! prepared in a package bound there and run by `EXCSQLSTT` with
//! `OUTEXP`; every reply object is printed (code point and bytes), which
//! is what `tools/conformance/ddm/outexp-primary.txt` holds. The
//! procedure and the package are removed at the end.
//! Ignored: it needs the oracle (`tools/conformance/oracle/.env`).
//!
//! `cargo test -p conformance --test ddm_outexp -- --ignored --nocapture`

use std::fmt::Write as _;
use std::net::TcpStream;
use std::time::Duration;

use ddm::client::{ConnectOpts, Connection};
use ddm::cp::{cmd, obj, param};
use ddm::fdoca::desc::Field;
use ddm::fdoca::{Repr, Writer, rows};
use ddm::sql::{ColType, Kind, Value};
use ddm::wire::Object;
use tn5250_driver::{Target, env::Env};

/// The primary's RDB name.
const RDB: &str = "S062B175";

/// The input `SQLDTA` of `fields` and `values`: early descriptors in
/// the column group, the row layout, and the row.
fn sqldta(fields: &[Field], values: &[Value]) -> Object {
    let mut gda = vec![0x76, 0xD0];
    for f in fields {
        gda.push(f.drda);
        gda.extend(f.len.to_be_bytes());
    }
    let mut dsc = vec![u8::try_from(gda.len() + 1).unwrap()];
    dsc.extend(gda);
    dsc.extend([0x06, 0x71, 0xE4, 0xD0, 0x00, 0x01]);
    let mut w = Writer::new(Repr::default());
    w.u8(rows::PRESENT);
    for (f, v) in fields.iter().zip(values) {
        rows::write_value(&mut w, f, v).unwrap();
    }
    Object::coll(
        obj::SQLDTA,
        vec![
            Object::data(obj::FDODSC, dsc),
            Object::data(obj::FDODTA, w.out),
        ],
    )
}

fn hex(b: &[u8]) -> String {
    let mut s = String::new();
    for (i, c) in b.chunks(16).enumerate() {
        let _ = write!(s, "  {:04x} ", i * 16);
        for x in c {
            let _ = write!(s, " {x:02x}");
        }
        s.push('\n');
    }
    s
}

#[test]
#[ignore = "needs the primary oracle"]
#[allow(clippy::many_single_char_names)] // one short binding per remote object
fn call_output_values_on_the_primary() {
    let e = Env::load().expect("the oracle environment");
    let c = e
        .credentials(Target::Oracle, None)
        .expect("oracle credentials");
    let host = c.host.split(':').next().unwrap_or(&c.host).to_string();
    let s = TcpStream::connect((host.as_str(), 446)).expect("the primary answers 446");
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut o = ConnectOpts::new(RDB, &c.user, &c.password);
    o.collection = "QSTEST".into();
    o.package = "QSDDMOUT".into();
    let mut sql = Connection::connect(s, &o).expect("DRDA connection");
    assert_eq!(sql.bind_dynamic(2).unwrap().sqlcode, 0);
    let made = sql
        .execute_immediate(
            "CREATE OR REPLACE PROCEDURE QSTEST.QSOUTP (IN N INTEGER, OUT W VARCHAR(10), INOUT A DECIMAL(7, 2)) \
             LANGUAGE SQL BEGIN SET W = 'DONE'; SET A = A * N; END",
        )
        .unwrap();
    println!("create: {}", made.sqlcode);
    sql.commit().unwrap();
    let p = sql.prepare("CALL QSTEST.QSOUTP(?, ?, ?)").unwrap();
    println!("prepare: {:?}", p.sqlca.as_ref().map(|x| x.sqlcode));
    let fields: Vec<Field> = [
        ColType::new(Kind::Integer, 0, 10, 0, true),
        ColType::new(Kind::VarChar, 10, 0, 0, true),
        ColType::new(Kind::Decimal, 0, 7, 2, true),
    ]
    .iter()
    .map(Field::of)
    .collect();
    let values = [
        Value::Int(3),
        Value::Null,
        Value::Dec("12.25".parse().unwrap()),
    ];
    let corr = sql.next_corr();
    let command = Object::coll(
        cmd::EXCSQLSTT,
        vec![p.pkg.to_object(), Object::byte(param::OUTEXP, 0xF1)],
    );
    let replies = sql
        .exchange(vec![
            ddm::Dss::request(corr, command),
            ddm::Dss::object(corr, sqldta(&fields, &values)),
        ])
        .unwrap();
    for d in &replies {
        for o in d.objects() {
            println!(
                "reply {:04X} ({} bytes)\n{}",
                o.cp,
                o.bytes().len(),
                hex(o.bytes())
            );
        }
    }
    let gone = sql
        .execute_immediate("DROP PROCEDURE QSTEST.QSOUTP")
        .unwrap();
    println!("drop: {}", gone.sqlcode);
    sql.commit().unwrap();
    // The package is free once the connection has gone.
    drop(sql.into_inner());
    let mut rla = {
        let s = TcpStream::connect((host.as_str(), 446)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        let mut o = ConnectOpts::new(RDB, &c.user, &c.password);
        o.access_rdb = false;
        Connection::connect(s, &o).unwrap()
    };
    let r = rla
        .remote_command("DLTSQLPKG SQLPKG(QSTEST/QSDDMOUT)")
        .unwrap();
    println!("dltsqlpkg: {:?}", r.messages);
}

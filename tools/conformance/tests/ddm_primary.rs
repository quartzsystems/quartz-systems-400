// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's DDM requester against the primary oracle (V7R1M0), which
//! answers 446: SQL through a package bound in `QSTEST`, and the record
//! access a DDM file does (`S38OPEN`, `S38PUTM`, `S38GET`, `S38GETK`,
//! `S38CLOSE`, `S38CMD`) on a table made there for the run. Everything
//! made is removed before the end: the table by SQL, the package by
//! `DLTSQLPKG` through `S38CMD`. Nothing outside `QSTEST` is touched.
//! Ignored: it needs the oracle (`tools/conformance/oracle/.env`).
//!
//! `cargo test -p conformance --test ddm_primary -- --ignored --nocapture`

use std::net::TcpStream;
use std::time::Duration;

use ddm::client::{ConnectOpts, Connection};
use ddm::file::{Rec, Ufcb, ufcb};
use tn5250_driver::{Target, env::Env};

/// The primary's RDB name (its `*LOCAL` entry, `WRKRDBDIRE`).
const RDB: &str = "S062B175";

fn connect(host: &str, user: &str, password: &str, access: bool) -> Connection<TcpStream> {
    let s = TcpStream::connect((host, 446)).expect("the primary answers 446");
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut o = ConnectOpts::new(RDB, user, password);
    o.collection = "QSTEST".into();
    o.package = "QSDDMPKG".into();
    o.access_rdb = access;
    Connection::connect(s, &o).expect("DRDA connection")
}

#[test]
#[ignore = "needs the primary oracle"]
#[allow(clippy::many_single_char_names)] // one short binding per remote object
fn requester_against_the_primary() {
    let e = Env::load().expect("the oracle environment");
    let c = e
        .credentials(Target::Oracle, None)
        .expect("oracle credentials");
    let host = c.host.split(':').next().unwrap_or(&c.host).to_string();

    // SQL: the table, two rows, the query.
    let mut sql = connect(&host, &c.user, &c.password, true);
    assert_eq!(sql.bind_dynamic(6).unwrap().sqlcode, 0);
    let _ = sql.execute_immediate("DROP TABLE QSTEST.QSDDMR");
    for stmt in [
        "CREATE TABLE QSTEST.QSDDMR (NAME CHAR(10) NOT NULL, CITY CHAR(10) NOT NULL)",
        "CREATE INDEX QSTEST.QSDDMRX ON QSTEST.QSDDMR (NAME)",
        "INSERT INTO QSTEST.QSDDMR VALUES ('Ann', 'Oslo'), ('Bob', 'Lima')",
    ] {
        let ca = sql.execute_immediate(stmt).unwrap();
        assert!(!ca.is_error(), "{stmt}: {ca:?}");
    }
    assert_eq!(sql.commit().unwrap().sqlcode, 0);
    drop(sql.into_inner());

    // Record access, as a DDM file does it: no ACCRDB.
    let mut rla = connect(&host, &c.user, &c.password, false);
    let u = Ufcb {
        file: "QSDDMR".into(),
        library: "QSTEST".into(),
        member: "*FIRST".into(),
        flags: ufcb::INPUT | ufcb::OUTPUT,
        arrival: true,
        share: Ufcb::share_for(ufcb::INPUT | ufcb::OUTPUT),
        blocking: 1,
        format: "QSDDMR".into(),
    };
    let f = rla.open_file("F1", &u, 2).expect("S38OPEN");
    assert_eq!(f.feedback.reclen, 20);
    let mut rec = ddm::wire::ebcdic_fixed("Cy", 10);
    rec.extend(ddm::wire::ebcdic_fixed("Rome", 10));
    let w = rla.put(&f, &[Rec::new(rec, 2)]).expect("S38PUTM");
    assert!(w.error().is_none(), "{w:?}");
    let mut names = Vec::new();
    let mut r = rla
        .get(&f, ddm::cp::s38::S38GET, ddm::file::op::FIRST, 0, &[])
        .expect("S38GET");
    while let Some(one) = r.records.first() {
        names.push(ddm::wire::text(&one.data[..10]).trim().to_string());
        r = rla
            .get(&f, ddm::cp::s38::S38GET, ddm::file::op::NEXT, 0, &[])
            .expect("S38GET next");
    }
    assert!(r.at_end(), "{r:?}");
    assert_eq!(names, ["Ann", "Bob", "Cy"]);
    rla.close_file(&f).expect("S38CLOSE");

    // By key, through the index.
    let k = Ufcb {
        file: "QSDDMRX".into(),
        flags: ufcb::INPUT,
        arrival: false,
        share: Ufcb::share_for(ufcb::INPUT),
        ..u
    };
    let fx = rla.open_file("F2", &k, 2).expect("S38OPEN keyed");
    let got = rla
        .get_key(&fx, &ddm::wire::ebcdic_fixed("Bob", 10), 1, false)
        .expect("S38GETK");
    let city = got
        .records
        .first()
        .map(|r| ddm::wire::text(&r.data[10..]).trim().to_string());
    rla.close_file(&fx).expect("S38CLOSE");

    // Leave QSTEST as it was.
    for cmd in [
        "DLTF FILE(QSTEST/QSDDMRX)",
        "DLTF FILE(QSTEST/QSDDMR)",
        "DLTSQLPKG SQLPKG(QSTEST/QSDDMPKG)",
    ] {
        let r = rla.remote_command(cmd).expect("S38CMD");
        assert!(r.error().is_none(), "{cmd}: {:?}", r.messages);
    }
    assert_eq!(city.as_deref(), Some("Lima"), "S38GETK answered {got:?}");
}

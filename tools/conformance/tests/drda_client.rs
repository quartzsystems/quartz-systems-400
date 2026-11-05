// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's SQL connections (`net/drdar`) against the primary oracle
//! (V7R1M0) as the remote server: `CONNECT TO` its RDB, queries and a
//! cursor on a QSTEST table it reads, `CURRENT SERVER`, `COMMIT`,
//! `SET CONNECTION` back to the local server, `DISCONNECT`. Read only:
//! the package the server asks for is bound in QSTEST and dropped at the
//! end. Ignored: it needs the oracle (`tools/conformance/oracle/.env`).
//!
//! `cargo test -p conformance --test drda_client -- --ignored --nocapture`

#![allow(clippy::result_large_err)] // the SQLCA a program sees
#![allow(clippy::many_single_char_names)] // short bindings for the connection parts

use std::net::TcpStream;
use std::time::Duration;

use ddm::client::{ConnectOpts, Connection};
use drdar::{Connector, Current, Remote, Routed, Server, Sessions};
use sql::embed::{Desc, Op, Request};
use sql::{Sqlca, Value};
use tn5250_driver::{Target, env::Env};

/// The primary's RDB name (its `*LOCAL` entry).
const RDB: &str = "S062B175";

struct Tcp {
    host: String,
}

impl Connector for Tcp {
    fn local(&self) -> String {
        "QS400".into()
    }

    fn connect(&mut self, rdb: &str, user: Option<(&str, &str)>) -> Result<Box<dyn Server>, Sqlca> {
        let (u, p) = user.expect("the test gives a user");
        let s = TcpStream::connect((self.host.as_str(), 446)).map_err(|e| Sqlca { sqlcode: -30_080, sqlstate: "08001".into(), sqlerrmc: e.to_string(), ..Sqlca::default() })?;
        s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        let mut o = ConnectOpts::new(rdb, u, p);
        o.collection = "QSTEST".into();
        o.package = "QSDRDAR".into();
        let c = Connection::connect(s, &o).map_err(|e| Sqlca { sqlcode: -30_082, sqlstate: "08001".into(), sqlerrmc: e.to_string(), ..Sqlca::default() })?;
        Ok(Box::new(Remote::new(c)))
    }
}

fn req(op: Op, text: &str, inputs: Vec<Value>) -> Request {
    Request { scope: "1:M".into(), desc: Desc { op, name: "C1".into(), sql_naming: true, text: text.into(), ..Desc::default() }, inputs }
}

fn done(r: Routed) -> sql::embed::Reply {
    match r {
        Routed::Done(r) => r,
        Routed::Local => panic!("left to the local server"),
    }
}

#[test]
#[ignore = "needs the primary oracle"]
fn connect_to_the_primary_and_read_qstest() {
    let e = Env::load().expect("the oracle environment");
    let c = e.credentials(Target::Oracle, None).expect("oracle credentials");
    let host = c.host.split(':').next().unwrap_or(&c.host).to_string();
    let mut net = Tcp { host };
    let mut s = Sessions::default();
    let r = done(s.route(&mut net, &req(Op::Exec, &format!("CONNECT TO {RDB} USER ? USING ?"), vec![Value::Str(c.user.clone()), Value::Str(c.password.clone())])));
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    assert_eq!(s.current, Current::Remote(RDB.into()));

    let r = done(s.route(&mut net, &req(Op::Query, "VALUES CURRENT SERVER", vec![])));
    println!("CURRENT SERVER: {:?}", r.rows);
    assert_eq!(r.rows, vec![vec![Value::Str(RDB.into())]]);

    let r = done(s.route(&mut net, &req(Op::Query, "SELECT COUNT(*) FROM QSTEST.DBM1", vec![])));
    println!("COUNT: {:?} {:?}", r.sqlca.sqlcode, r.rows);
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);

    let r = done(s.route(&mut net, &req(Op::Open, "SELECT K, V FROM QSTEST.DBM1 ORDER BY K", vec![])));
    assert_eq!(r.sqlca.sqlcode, 0, "{:?}", r.sqlca);
    let mut rows = Vec::new();
    loop {
        let r = done(s.route(&mut net, &req(Op::Fetch, "", vec![])));
        if r.sqlca.sqlcode != 0 {
            assert_eq!(r.sqlca.sqlcode, 100);
            break;
        }
        rows.extend(r.rows);
    }
    println!("rows: {rows:?}");
    assert!(!rows.is_empty());
    assert_eq!(done(s.route(&mut net, &req(Op::Close, "", vec![]))).sqlca.sqlcode, 0);

    // The server's own errors come back as its SQLCA.
    let r = done(s.route(&mut net, &req(Op::Query, "SELECT * FROM QSTEST.NOSUCH", vec![])));
    assert_eq!(r.sqlca.sqlcode, -204, "{:?}", r.sqlca);

    assert!(matches!(s.route(&mut net, &req(Op::Exec, "COMMIT", vec![])), Routed::Local));
    assert!(matches!(s.route(&mut net, &req(Op::Exec, "SET CONNECTION QS400", vec![])), Routed::Done(_)));
    assert_eq!(s.current, Current::Local);
    assert!(matches!(s.route(&mut net, &req(Op::Query, "VALUES 1", vec![])), Routed::Local));

    // Disconnecting a server that is not the current one keeps the local one current.
    assert_eq!(done(s.route(&mut net, &req(Op::Exec, &format!("DISCONNECT {RDB}"), vec![]))).sqlca.sqlcode, 0);
    assert_eq!(s.current, Current::Local);
    assert!(s.servers().is_empty());

    // Leave QSTEST as it was: the package the server asked for goes.
    let st = TcpStream::connect((net.host.as_str(), 446)).unwrap();
    st.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut o = ConnectOpts::new(RDB, &c.user, &c.password);
    o.access_rdb = false;
    let mut cmd = Connection::connect(st, &o).expect("DDM connection");
    let r = cmd.remote_command("DLTSQLPKG SQLPKG(QSTEST/QSDRDAR)").expect("S38CMD");
    assert!(r.error().is_none(), "{:?}", r.messages);
}

// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's connections over fake servers: CONNECT, SET CONNECTION,
//! RELEASE, DISCONNECT, COMMIT across them, DUW and RUW, statements
//! routed to the current server.

use std::cell::RefCell;
use std::rc::Rc;

use drdar::{Connector, Current, Method, Routed, Server, Sessions};
use sql::embed::{Desc, Op, Request};
use sql::{Sqlca, Value};

type Log = Rc<RefCell<Vec<String>>>;

struct Fake {
    rdb: String,
    log: Log,
}

impl Server for Fake {
    fn query(&mut self, text: &str) -> Result<Vec<Vec<Value>>, Sqlca> {
        self.log
            .borrow_mut()
            .push(format!("{} query {text}", self.rdb));
        Ok(vec![
            vec![Value::Str(self.rdb.clone())],
            vec![Value::int(2)],
        ])
    }
    fn execute(&mut self, text: &str) -> Sqlca {
        self.log
            .borrow_mut()
            .push(format!("{} execute {text}", self.rdb));
        Sqlca::rows(1)
    }
    fn commit(&mut self) -> Sqlca {
        self.log.borrow_mut().push(format!("{} commit", self.rdb));
        Sqlca::default()
    }
    fn rollback(&mut self) -> Sqlca {
        self.log.borrow_mut().push(format!("{} rollback", self.rdb));
        Sqlca::default()
    }
}

struct Net {
    log: Log,
}

impl Connector for Net {
    fn local(&self) -> String {
        "QS400".into()
    }
    fn connect(&mut self, rdb: &str, user: Option<(&str, &str)>) -> Result<Box<dyn Server>, Sqlca> {
        if rdb == "NOSUCH" {
            return Err(Sqlca {
                sqlcode: -950,
                sqlstate: "42705".into(),
                ..Sqlca::default()
            });
        }
        self.log
            .borrow_mut()
            .push(format!("connect {rdb} {}", user.map_or("-", |u| u.0)));
        Ok(Box::new(Fake {
            rdb: rdb.into(),
            log: self.log.clone(),
        }))
    }
}

fn req(op: Op, text: &str, inputs: Vec<Value>) -> Request {
    Request {
        scope: "1:M".into(),
        desc: Desc {
            op,
            name: "C1".into(),
            text: text.into(),
            ..Desc::default()
        },
        inputs,
    }
}

fn code(r: &Routed) -> i32 {
    match r {
        Routed::Done(r) => r.sqlca.sqlcode,
        Routed::Local => 1,
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn duw_connections_switch_release_and_commit() {
    let log = Log::default();
    let mut net = Net { log: log.clone() };
    let mut s = Sessions::default();
    assert_eq!(
        code(&s.route(
            &mut net,
            &req(Op::Exec, "SELECT 1 FROM SYSIBM.SYSDUMMY1", vec![])
        )),
        1
    );
    assert_eq!(
        code(&s.route(
            &mut net,
            &req(
                Op::Exec,
                "CONNECT TO REM1 USER ? USING ?",
                vec![Value::Str("bob".into()), Value::Str("pw".into())]
            )
        )),
        0
    );
    assert_eq!(s.current, Current::Remote("REM1".into()));
    // A second CONNECT to the same server is refused under DUW.
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM1", vec![]))),
        -842
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM2", vec![]))),
        0
    );
    // Statements go to the current server.
    let Routed::Done(r) = s.route(&mut net, &req(Op::Exec, "UPDATE T SET A = 1", vec![])) else {
        panic!("local")
    };
    assert_eq!(r.sqlca.sqlcode, 0);
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "SET CONNECTION REM1", vec![]))),
        0
    );
    let Routed::Done(r) = s.route(&mut net, &req(Op::Query, "VALUES CURRENT SERVER", vec![]))
    else {
        panic!("local")
    };
    assert_eq!(r.sqlca.sqlcode, -811); // two rows from the fake
    assert_eq!(r.rows, vec![vec![Value::Str("REM1".into())]]);
    // Cursors read the server's rows.
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Open, "SELECT X FROM T", vec![]))),
        0
    );
    let Routed::Done(r) = s.route(&mut net, &req(Op::Fetch, "", vec![])) else {
        panic!("local")
    };
    assert_eq!(r.rows.len(), 1);
    assert_eq!(code(&s.route(&mut net, &req(Op::Fetch, "", vec![]))), 0);
    assert_eq!(code(&s.route(&mut net, &req(Op::Fetch, "", vec![]))), 100);
    assert_eq!(code(&s.route(&mut net, &req(Op::Close, "", vec![]))), 0);
    // Host variables are not sent to a remote server.
    assert_eq!(
        code(&s.route(
            &mut net,
            &req(
                Op::Query,
                "SELECT A FROM T WHERE B = ?",
                vec![Value::int(1)]
            )
        )),
        -84
    );
    // RELEASE: the connection goes at the commit, which reaches both
    // servers and then the local one.
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "RELEASE REM2", vec![]))),
        0
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "RELEASE NOSUCH", vec![]))),
        -843
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "COMMIT", vec![]))),
        1
    );
    assert_eq!(s.servers(), vec![("REM1".to_string(), true)]);
    let l = log.borrow().join("\n");
    assert!(
        l.contains("connect REM1 BOB")
            && l.contains("REM2 execute UPDATE T SET A = 1")
            && l.contains("REM1 commit")
            && l.contains("REM2 commit"),
        "{l}"
    );
    // Back to the local server.
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "SET CONNECTION QS400", vec![]))),
        0
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Query, "VALUES 1", vec![]))),
        1
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "SET CONNECTION REM9", vec![]))),
        -843
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "DISCONNECT ALL", vec![]))),
        0
    );
    assert_eq!(s.current, Current::None);
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Query, "VALUES 1", vec![]))),
        -843
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT RESET", vec![]))),
        0
    );
    assert_eq!(s.current, Current::Local);
}

#[test]
fn ruw_has_one_connection_and_keeps_changes() {
    let log = Log::default();
    let mut net = Net { log };
    let mut s = Sessions::default();
    s.method = Method::Ruw;
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM1", vec![]))),
        0
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "DELETE FROM T", vec![]))),
        0
    );
    // A change not committed keeps the connection.
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM2", vec![]))),
        -752
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "ROLLBACK", vec![]))),
        1
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM2", vec![]))),
        0
    );
    assert_eq!(s.servers(), vec![("REM2".to_string(), true)]);
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO NOSUCH", vec![]))),
        -950
    );
    assert_eq!(
        code(&s.route(&mut net, &req(Op::Exec, "CONNECT TO REM2 USER X", vec![]))),
        -104
    );
}

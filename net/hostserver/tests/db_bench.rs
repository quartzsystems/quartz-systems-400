// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Host server SQL fetch rate: the database server over loopback TCP,
//! the Phase 3 SQL engine over a store in memory, and the request
//! sequence `JTOpen` uses for a query: seed exchange and start server,
//! prepare and describe (`0x1803`, the text in `0x3831`), open,
//! describe and fetch (`0x180E`), fetches (`0x180B`) with a 32 KiB
//! buffer (`0x3834`, `JTOpen`'s default block size) until end of data,
//! close. Replies are not compressed. Ignored by default; run with
//! `cargo test --release -p hostserver --test db_bench -- --ignored --nocapture`
//! and `QS400_HS_BENCH_ROWS=N` for the table size (default 10,000; the
//! engine's `CREATE TABLE` member holds at most 13,000 records).

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

mod common;

use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Instant;

use common::mem::session;
use hostserver::database::{Database, DbConfig};
use hostserver::{Auth, NoExits, Proof};
use hostsvr::cp::{db_reply, db_request, signon};
use hostsvr::database::{id, ors};
use hostsvr::database::param::DbString;
use hostsvr::frame::{Frame, Param};
use sql::embed::{Desc, Embedded, Op, Request};

const DATABASE: u16 = 0xE004;
const BUFFER: u32 = 32 * 1024;

/// Every proof is good.
struct Yes;

impl Auth for Yes {
    fn password_level(&mut self) -> u8 {
        0
    }
    fn seed(&mut self) -> [u8; 8] {
        [7; 8]
    }
    fn start(&mut self, _user: &str, _proof: Proof<'_>) -> Result<(), u32> {
        Ok(())
    }
    fn job_name(&mut self) -> String {
        "000001/QUSER/QZDASOINIT".into()
    }
}

fn exec(s: &mut impl Embedded, text: &str) {
    let r = s.run(&Request {
        scope: "LOAD:".into(),
        desc: Desc {
            op: Op::Exec,
            sql_naming: true,
            text: text.into(),
            ..Desc::default()
        },
        inputs: Vec::new(),
    });
    assert!(!r.sqlca.is_error(), "{text}: {} {}", r.sqlca.sqlcode, r.sqlca.sqlerrmc);
}

/// The server: a session with `rows` rows in `QSTEST.BENCH`; the load
/// time goes back on `done` once the server listens.
fn server(rows: usize, done: mpsc::Sender<f64>) -> SocketAddr {
    let l = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        let t = Instant::now();
        let mut s = session();
        exec(
            &mut s,
            "CREATE TABLE QSTEST.BENCH (ID INTEGER NOT NULL, CODE CHAR(10), AMT DECIMAL(11, 2), NOTE VARCHAR(50))",
        );
        for start in (0..rows).step_by(100) {
            let vals: Vec<String> = (start..(start + 100).min(rows))
                .map(|i| format!("({i}, 'C{:04}', {}.{:02}, 'row {i} of the bench table')", i % 997, i % 10_000, i % 100))
                .collect();
            exec(&mut s, &format!("INSERT INTO QSTEST.BENCH VALUES {}", vals.join(", ")));
        }
        done.send(t.elapsed().as_secs_f64()).unwrap();
        let cfg = DbConfig {
            rdb: "QS400T".into(),
            job: ("QZDASOINIT".into(), "QUSER".into(), "000001".into()),
        };
        let mut db = Database::new(Yes, s, NoExits, cfg);
        let (mut stream, _) = l.accept().unwrap();
        let _ = stream.set_nodelay(true);
        let _ = hostserver::io::serve(&mut stream, &mut db);
    });
    addr
}

/// A database request's 20-byte template.
fn template(ors_bits: u32, rpb: u16) -> Vec<u8> {
    let mut t = vec![0u8; 20];
    t[0..4].copy_from_slice(&ors_bits.to_be_bytes());
    t[14..16].copy_from_slice(&rpb.to_be_bytes());
    t
}

struct Conn {
    s: TcpStream,
}

impl Conn {
    fn ask(&mut self, f: &Frame) -> Frame {
        self.s.write_all(&f.encode()).unwrap();
        let b = hostserver::io::read_frame(&mut self.s).unwrap().expect("a reply");
        Frame::decode(&b).unwrap()
    }
}

/// The reply's error class and return code.
fn class(f: &Frame) -> (u16, i32) {
    let t = &f.template;
    (u16::from_be_bytes([t[14], t[15]]), i32::from_be_bytes([t[16], t[17], t[18], t[19]]))
}

/// Rows in the reply's result data.
fn rows_in(f: &Frame) -> usize {
    f.param(db_reply::EXTENDED_RESULT_DATA)
        .filter(|p| p.data.len() >= 8)
        .map_or(0, |p| u32::from_be_bytes([p.data[4], p.data[5], p.data[6], p.data[7]]) as usize)
}

#[test]
#[ignore = "bench: run in release with --ignored --nocapture"]
fn jdbc_style_fetch() {
    let rows: usize = std::env::var("QS400_HS_BENCH_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000);
    let (tx, rx) = mpsc::channel();
    let addr = server(rows, tx);
    let load = rx.recv().unwrap();
    let s = TcpStream::connect(addr).unwrap();
    s.set_nodelay(true).unwrap();
    let mut c = Conn { s };

    let seed = c.ask(&Frame::new(DATABASE, 0x7001, vec![1; 8], Vec::new()));
    assert_eq!(seed.header.req_rep_id, 0xF001);
    let mut user = vec![0x40u8; 10];
    user[..6].copy_from_slice(&ccsid::to_ebcdic("QSTEST").unwrap());
    let start = c.ask(&Frame::new(
        DATABASE,
        0x7002,
        vec![1, 1],
        vec![Param::new(signon::USER_ID, user), Param::new(signon::PASSWORD, vec![0; 8])],
    ));
    assert_eq!(&start.template[..4], &[0, 0, 0, 0], "start server");

    let text: Vec<u8> = "SELECT ID, CODE, AMT, NOTE FROM QSTEST.BENCH"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    let stmt = DbString { ccsid: 13488, long: true, text };
    let ask = ors::RETURN_DATA | ors::SQLCA;
    let t = Instant::now();
    let r = c.ask(&Frame::new(
        DATABASE,
        id::PREPARE_DESCRIBE,
        template(ask | ors::DATA_FORMAT, 1),
        vec![Param::new(db_request::EXTENDED_STATEMENT_TEXT, stmt.encode())],
    ));
    assert_eq!(class(&r).0, 0, "prepare: {:?}", class(&r));
    let buffer = Param::u32(db_request::BUFFER_SIZE, BUFFER);
    let r = c.ask(&Frame::new(
        DATABASE,
        id::OPEN_DESCRIBE_FETCH,
        template(ask | ors::RESULT_DATA | ors::DATA_FORMAT, 1),
        vec![buffer.clone()],
    ));
    let mut got = rows_in(&r);
    let mut trips = 1;
    let mut end = class(&r) == (2, 700);
    assert!(class(&r).0 == 0 || end, "open: {:?}", class(&r));
    while !end {
        let r = c.ask(&Frame::new(DATABASE, id::FETCH, template(ask | ors::RESULT_DATA, 1), vec![buffer.clone()]));
        trips += 1;
        got += rows_in(&r);
        end = class(&r) == (1, 100);
        assert!(class(&r).0 == 0 || end, "fetch: {:?}", class(&r));
    }
    let r = c.ask(&Frame::new(DATABASE, id::CLOSE, template(ask, 1), Vec::new()));
    assert_eq!(class(&r).0, 0, "close");
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(got, rows);

    println!("host server SQL fetch: {rows} rows (INTEGER, CHAR(10), DECIMAL(11,2), VARCHAR(50)), loopback TCP");
    println!("  load (multi-row INSERT, not timed below) {load:.3} s");
    println!("  prepare + open/fetch + {} fetches + close: {secs:.3} s", trips - 1);
    println!("  {:.0} rows/s, {:.1} rows per reply", rows as f64 / secs, rows as f64 / f64::from(trips));
}
